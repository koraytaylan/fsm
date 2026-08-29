//! Durable single-writer store for the `fsm` statechart engine.
//!
//! `fsm-core` is the pure engine: it folds records into state and steps
//! instances without touching the filesystem or a clock. This crate is the
//! shell around it — an append-only hash-chained journal, the machine and
//! instance store folded from that journal, disposable snapshots, and the one
//! wall-clock read in the system.
//!
//! Embedders that keep their own persistence do not need this crate; they use
//! `fsm-core` directly. See `docs/EMBEDDING.md` for both loops.
//!
//! # Concurrency contract
//!
//! Every operation on [`store::Store`] is **synchronous and blocking**, and a
//! store is a **single-writer** resource guarded by a process-wide advisory
//! lock. Callers on an async runtime must own a `Store` from one dedicated
//! blocking thread (a writer actor) rather than sharing it across tasks. See
//! `docs/EMBEDDING.md` for measured append latency and the actor pattern.

#![forbid(unsafe_code)]
#![allow(
    clippy::result_large_err,
    clippy::collapsible_if,
    clippy::collapsible_match
)]

pub mod archive;
pub mod base;
pub mod clock;
pub mod journal_io;
pub mod seal_pin;
pub mod seal_safety;
pub mod snapshot;
pub mod store;

// Persistence units are parsed with the default JSON limits. Keeping the I/O
// ceiling derived from that parser contract prevents the reader and writer
// from accepting different byte ranges if the default ever changes.
pub(crate) const PERSISTENCE_READ_CAP: usize = fsm_core::json::JsonLimits::DEFAULT.max_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistenceCreate {
    RequireExisting,
    CreateIfMissing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistenceWriteMode {
    Update,
    Append,
    Replace,
}

fn persistence_input_error(path: &std::path::Path, message: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{}: {message}", path.display()),
    )
}

/// Return whether a persistence directory exists after rejecting a symlink or
/// non-directory observed at its final path component.
///
/// Portable `std` cannot retain a directory handle for later path-relative
/// operations, so a hostile concurrent replacement after this check remains
/// outside its guarantee. Ancestors (including a caller-supplied data-dir
/// symlink) are likewise outside this leaf-directory check.
pub(crate) fn persistence_directory_exists(path: &std::path::Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() => {
            Err(persistence_input_error(
                path,
                "persistence directory path must name a non-symlink directory",
            ))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Create a missing persistence directory while rejecting a static symlink or
/// non-directory both before and after creation.
///
/// As with [`persistence_directory_exists`], portable `std` cannot prevent a
/// hostile concurrent replacement after the final metadata check.
pub(crate) fn ensure_persistence_directory(path: &std::path::Path) -> std::io::Result<()> {
    if persistence_directory_exists(path)? {
        return Ok(());
    }
    std::fs::create_dir_all(path)?;
    if persistence_directory_exists(path)? {
        Ok(())
    } else {
        Err(persistence_input_error(
            path,
            "persistence directory disappeared while creating it",
        ))
    }
}

/// Open a writable persistence leaf without following a symlink observed at
/// the path. Missing leaves use `create_new`, and replacement truncates only
/// after the opened handle is verified against the pre-open metadata.
///
/// Portable `std` has no atomic no-follow open. The pre/post checks reject
/// static path attacks and Unix inode swaps observed during open, but a hostile
/// concurrent replacement remains outside the cross-platform guarantee.
pub(crate) fn open_regular_file_for_write(
    path: &std::path::Path,
    create: PersistenceCreate,
    mode: PersistenceWriteMode,
) -> std::io::Result<std::fs::File> {
    let before = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            return Err(persistence_input_error(
                path,
                "persistence output must be a regular, non-symlink file",
            ));
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if before.is_none() && create == PersistenceCreate::RequireExisting {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{}: persistence output does not exist", path.display()),
        ));
    }

    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    if mode == PersistenceWriteMode::Append {
        options.append(true);
    }
    if before.is_none() {
        options.create_new(true);
    }
    let file = options.open(path)?;
    let after = file.metadata()?;
    if !after.file_type().is_file() {
        return Err(persistence_input_error(
            path,
            "persistence output changed to a non-regular file",
        ));
    }
    #[cfg(unix)]
    if let Some(before) = &before {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(persistence_input_error(
                path,
                "persistence output changed while opening",
            ));
        }
    }
    if mode == PersistenceWriteMode::Replace {
        file.set_len(0)?;
    }
    Ok(file)
}

/// Open a persistence input after rejecting a symlink or non-regular file
/// observed at the path, then verify that the opened handle still names the
/// inspected regular file. This safely rejects static FIFOs/devices and
/// symlinks to endless inputs. Portable `std` has no atomic no-follow open, so
/// a hostile concurrent path replacement remains outside this check's
/// guarantee.
pub(crate) fn open_regular_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let before = std::fs::symlink_metadata(path)?;
    if before.file_type().is_symlink() || !before.file_type().is_file() {
        return Err(persistence_input_error(
            path,
            "persistence input must be a regular, non-symlink file",
        ));
    }
    let file = std::fs::File::open(path)?;
    let after = file.metadata()?;
    if !after.file_type().is_file() {
        return Err(persistence_input_error(
            path,
            "persistence input changed to a non-regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(persistence_input_error(
                path,
                "persistence input changed while opening",
            ));
        }
    }
    Ok(file)
}

/// Read one regular persistence file with a hard bound that is checked before
/// allocating from its metadata and enforced again while reading in case the
/// file grows concurrently.
pub(crate) fn read_regular_file_capped(
    path: &std::path::Path,
    cap: usize,
) -> std::io::Result<Vec<u8>> {
    let mut file = open_regular_file(path)?;
    read_open_file_capped(&mut file, path, cap)
}

pub(crate) fn read_open_file_capped(
    file: &mut std::fs::File,
    path: &std::path::Path,
    cap: usize,
) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    let len = file.metadata()?.len();
    if !file.metadata()?.file_type().is_file() {
        return Err(persistence_input_error(
            path,
            "persistence input must be a regular file",
        ));
    }
    if len > cap as u64 {
        return Err(persistence_input_error(
            path,
            &format!("persistence input exceeds {cap} bytes"),
        ));
    }
    let mut bytes = Vec::with_capacity(len as usize);
    let mut chunk = [0u8; 8192];
    loop {
        let remaining = cap.saturating_sub(bytes.len());
        if remaining == 0 {
            let mut extra = [0u8; 1];
            if file.read(&mut extra)? != 0 {
                return Err(persistence_input_error(
                    path,
                    &format!("persistence input exceeds {cap} bytes"),
                ));
            }
            return Ok(bytes);
        }
        let take = remaining.min(chunk.len());
        let read = file.read(&mut chunk[..take])?;
        if read == 0 {
            return Ok(bytes);
        }
        bytes
            .try_reserve_exact(read)
            .map_err(|error| persistence_input_error(path, &error.to_string()))?;
        bytes.extend_from_slice(&chunk[..read]);
    }
}

pub(crate) fn read_regular_string_capped(
    path: &std::path::Path,
    cap: usize,
) -> std::io::Result<String> {
    String::from_utf8(read_regular_file_capped(path, cap)?)
        .map_err(|error| persistence_input_error(path, &error.to_string()))
}

pub(crate) fn read_regular_range_capped(
    path: &std::path::Path,
    offset: u64,
    len: u64,
    cap: usize,
) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    if len > cap as u64 {
        return Err(persistence_input_error(
            path,
            &format!("persistence input range exceeds {cap} bytes"),
        ));
    }
    let mut file = open_regular_file(path)?;
    let file_len = file.metadata()?.len();
    let end = offset
        .checked_add(len)
        .ok_or_else(|| persistence_input_error(path, "persistence input range overflow"))?;
    if end > file_len {
        return Err(persistence_input_error(
            path,
            "persistence input range is past end of file",
        ));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; len as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(crate) struct CappedLine {
    pub bytes: Vec<u8>,
    pub start: u64,
    pub end: u64,
    pub terminated: bool,
}

/// Streaming reader for journal segments. It never drains an overlong line:
/// callers reject immediately, so a concurrently growing input cannot turn an
/// error path into an endless read.
pub(crate) struct CappedLineReader {
    reader: std::io::BufReader<std::fs::File>,
    path: std::path::PathBuf,
    offset: u64,
    cap: usize,
}

impl CappedLineReader {
    pub(crate) fn open(path: &std::path::Path, cap: usize) -> std::io::Result<Self> {
        Ok(Self {
            reader: std::io::BufReader::new(open_regular_file(path)?),
            path: path.to_path_buf(),
            offset: 0,
            cap,
        })
    }

    pub(crate) fn next_line(&mut self) -> std::io::Result<Option<CappedLine>> {
        use std::io::BufRead;

        let start = self.offset;
        let mut bytes = Vec::new();
        loop {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                return if bytes.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(CappedLine {
                        bytes,
                        start,
                        end: self.offset,
                        terminated: false,
                    }))
                };
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let take = newline.unwrap_or(available.len());
            if bytes.len().saturating_add(take) > self.cap {
                return Err(persistence_input_error(
                    &self.path,
                    &format!("journal record exceeds {} bytes", self.cap),
                ));
            }
            bytes
                .try_reserve_exact(take)
                .map_err(|error| persistence_input_error(&self.path, &error.to_string()))?;
            bytes.extend_from_slice(&available[..take]);
            let consumed = take + usize::from(newline.is_some());
            self.reader.consume(consumed);
            self.offset = self.offset.saturating_add(consumed as u64);
            if newline.is_some() {
                return Ok(Some(CappedLine {
                    bytes,
                    start,
                    end: self.offset,
                    terminated: true,
                }));
            }
        }
    }

    pub(crate) fn position(&self) -> u64 {
        self.offset
    }
}

/// Write `bytes` to `path` and fsync them before returning.
///
/// The obvious spelling — `fs::write`, then reopen with `File::open` and
/// `sync_all` — is broken on Windows: `File::open` asks for read access, and
/// `FlushFileBuffers` requires a handle that can write, so the flush fails with
/// `ERROR_ACCESS_DENIED`. On Unix `fsync` on a read-only descriptor is fine,
/// which is why it survived this long.
///
/// Keeping the write handle and syncing it also closes the file before the
/// caller renames it into place, which Windows requires and Unix does not care
/// about. Both hazards live here once instead of at every durable-write site.
///
/// Public because a durable write is not only a store concern: `fsm machine
/// test`'s regeneration rewrites a committed case file, and truncating that in
/// place would risk the evidence file the whole feature exists to keep
/// reviewable. One implementation, not two.
pub fn write_durable(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = open_regular_file_for_write(
        path,
        PersistenceCreate::CreateIfMissing,
        PersistenceWriteMode::Replace,
    )?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Flush a *directory* so that a file created or renamed inside it survives a
/// crash, not just the file's own contents.
///
/// Unix only, and deliberately a no-op elsewhere. On Windows there is no
/// portable equivalent: `File::open` on a directory fails outright with
/// `ERROR_ACCESS_DENIED`, and obtaining a flushable directory handle needs
/// `FILE_FLAG_BACKUP_SEMANTICS`, which `std` does not expose.
///
/// What this does **not** affect on any platform: journal record durability.
/// Every append fsyncs the segment *file* before returning, and that works
/// identically everywhere. What is weaker on Windows is only the durability of
/// the enclosing directory entry after a create or rename — segment rotation,
/// snapshot installation, and the request-id allocation file. A crash inside
/// that window can leave the entry missing even though the bytes were flushed.
/// The store classifies and repairs that on the next open rather than trusting
/// it, so the consequence is a recovery step, not silent loss.
pub(crate) fn sync_dir(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
