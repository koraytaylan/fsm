use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use fsm_core::json::{JsonLimits, Value, parse};

use super::types::JournalIoError;

pub fn journal_dir(data: &Path) -> PathBuf {
    data.join("journal")
}

/// Attach the operation and path to an IO failure.
///
/// `io::Error` alone renders as bare OS text — "Access is denied. (os error 5)"
/// names neither the file nor what was attempted, which leaves an operator, and
/// anyone reading a CI log, with nothing to act on. Every IO failure on the
/// open path carries both.
pub(super) fn io_err(op: &str, path: &Path, e: impl std::fmt::Display) -> JournalIoError {
    JournalIoError::Io(format!("{op} {}: {e}", path.display()))
}

pub(super) fn acquire_lock(jdir: &Path) -> Result<File, JournalIoError> {
    crate::ensure_persistence_directory(jdir).map_err(|e| io_err("create journal dir", jdir, e))?;
    let path = jdir.join("LOCK");
    let mut f = crate::open_regular_file_for_write(
        &path,
        crate::PersistenceCreate::CreateIfMissing,
        crate::PersistenceWriteMode::Update,
    )
    .map_err(|e| io_err("open lock", &path, e))?;
    if f.try_lock().is_err() {
        let buf = crate::read_open_file_capped(&mut f, &path, crate::PERSISTENCE_READ_CAP)
            .unwrap_or_default();
        let pid = parse(&buf, &JsonLimits::DEFAULT)
            .ok()
            .and_then(|v| {
                v.get("pid")
                    .and_then(Value::as_num)
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0);
        return Err(JournalIoError::Locked { pid });
    }
    f.set_len(0)
        .map_err(|e| io_err("truncate lock", &path, e))?;
    let pid = std::process::id();
    let ts = crate::clock::now_ms();
    let line = format!("{{\"pid\":{pid},\"started_ts\":{ts}}}\n");
    f.write_all(line.as_bytes())
        .map_err(|e| io_err("write lock", &path, e))?;
    f.sync_all().map_err(|e| io_err("sync lock", &path, e))?;
    Ok(f)
}

pub(super) fn sync_dir(dir: &Path) -> Result<(), JournalIoError> {
    crate::sync_dir(dir).map_err(|e| io_err("sync dir", dir, e))
}

pub(super) fn seg_name(first: u64) -> String {
    format!("seg-{first:020}.jsonl")
}
