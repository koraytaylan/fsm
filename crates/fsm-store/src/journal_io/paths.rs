use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use fsm_core::json::{JsonLimits, Value, parse};

use super::types::JournalIoError;

pub fn journal_dir(data: &Path) -> PathBuf {
    data.join("journal")
}

/// How many interrupted `flock` calls to ride out before giving up.
///
/// An interruption is a signal, not contention, so retrying is the correct
/// answer and a small bound is enough: signals do not arrive in unbounded
/// bursts, and a bound keeps a pathological host from spinning here forever.
const LOCK_INTERRUPTION_RETRIES: u32 = 8;

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
    // `try_lock` reports three different things, and collapsing them tells an
    // operator the store is busy when it is not. "Someone holds it" is
    // `WouldBlock`; anything else is the *call* failing. In particular, a
    // signal arriving while the process reaps a child interrupts `flock`, and
    // that became reachable the moment an executor started spawning handlers
    // beside its own writer: reporting `store/lock` for it would blame a
    // second writer that does not exist, and would end an `--exclusive`
    // executor run outright. An interrupted call is retried; a real I/O
    // failure is reported as one.
    let mut interruptions = 0;
    loop {
        match f.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {
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
            Err(std::fs::TryLockError::Error(error))
                if error.kind() == std::io::ErrorKind::Interrupted
                    && interruptions < LOCK_INTERRUPTION_RETRIES =>
            {
                interruptions += 1;
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(io_err("lock", &path, error));
            }
        }
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
