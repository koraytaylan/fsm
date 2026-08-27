//! Append-only journal segments with per-record fsync.

//! `journal/LOCK` is released by the OS on process death; the pid metadata is
//! diagnostic only and is never trusted for liveness. There is deliberately no
//! stale-lock heuristic.
//!
//! Lock semantics differ by platform and the difference is visible in one
//! place. Unix `flock` is advisory, so the pid metadata can be read while the
//! lock is held; Windows `LockFileEx` is mandatory and refuses that read. The
//! contention path already tolerates it — the pid falls back to 0 and the
//! caller still gets `Locked` — so on Windows a busy store reports that it is
//! owned without naming the owner. Exclusion itself is identical.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

mod classify;
mod init;
mod journal_impl;
mod load;
mod open;
mod paths;
mod repair;
#[cfg(test)]
mod tests;
mod types;
mod verify;

pub use classify::{Diagnosis, classify, diagnose};
pub use init::init;
pub use load::{load_intact_prefix, load_records};
pub use open::open;
pub(crate) use open::open_read_only;
pub use paths::journal_dir;
pub use repair::{RepairError, RepairReport, repair_truncate_torn_tail};
pub use types::{Journal, JournalHealth, JournalIoError, OpenError};
pub use verify::{
    BATCH, SegmentProgress, VerifyReport, Walk, refuse_incompatible_store_format, verify,
    verify_segments, verify_segments_with,
};

const ROTATE_RECORDS: u32 = 65_536;
const ROTATE_BYTES: u64 = 64 * 1024 * 1024;
// VERSION 6 adds the `state_checkpoint` record used to bind explicit
// snapshots without rewriting prior journal records. VERSION 7 adds
// `request_fp` to every record that claims a `request_id`. VERSION 8 adds
// parallel active configurations and durable deadline schedules, with explicit
// state-hash and state-root format discriminators. Formats 1–7 (and a journal
// with no VERSION marker) are best-effort migrated on open by folding the
// complete journal and stamping 8. Records are never rewritten, so legacy
// records retain their historical hash material and request-id behavior.
/// Current marker written to a store's `VERSION` file.
pub const STORE_VERSION: &str = "9";

/// On-disk store format as detected before opening. Public because store
/// diagnostics (`fsm store status`, `fsm store repair`) report it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedStoreFormat {
    Current,
    Empty,
    Migratable { found: String },
    Incompatible { found: String },
    Unreadable { err: String },
}

fn is_seg_file_name(name: &str) -> bool {
    name.starts_with("seg-") && name.ends_with(".jsonl")
}

pub(super) fn journal_segment_paths(jdir: &Path) -> Result<Vec<PathBuf>, String> {
    match crate::persistence_directory_exists(jdir) {
        Ok(true) => {}
        Ok(false) => return Err(format!("journal directory {} is absent", jdir.display())),
        Err(error) => {
            return Err(format!(
                "inspect journal directory {}: {error}",
                jdir.display()
            ));
        }
    }
    let entries = fs::read_dir(jdir)
        .map_err(|error| format!("read journal directory {}: {error}", jdir.display()))?;
    let mut segments = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "read journal directory entry in {}: {error}",
                jdir.display()
            )
        })?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_seg_file_name)
        {
            segments.push(path);
        }
    }
    segments.sort();
    Ok(segments)
}

fn has_journal_segments(dir: &Path) -> Result<bool, String> {
    let jdir = journal_dir(dir);
    match crate::persistence_directory_exists(&jdir) {
        Ok(false) => Ok(false),
        Ok(true) => journal_segment_paths(&jdir).map(|segments| !segments.is_empty()),
        Err(error) => Err(format!(
            "read journal directory {}: {error}",
            jdir.display()
        )),
    }
}

/// Every store version this build can fold and stamp forward.
///
/// Opening one folds the complete journal using each record's own
/// `state_format` discriminator and stamps the current version on success; a
/// failed fold refuses and leaves `VERSION` alone, and interior records are
/// never rewritten.
fn is_migratable_version(v: &str) -> bool {
    matches!(v, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8")
}

/// Classify an on-disk store directory without opening or locking it.
pub fn detect_store_format(dir: &Path) -> DetectedStoreFormat {
    let ver = dir.join("VERSION");
    match crate::read_regular_string_capped(&ver, crate::PERSISTENCE_READ_CAP) {
        Ok(t) => {
            let t = t.trim();
            if t == STORE_VERSION {
                return DetectedStoreFormat::Current;
            }
            if is_migratable_version(t) {
                return DetectedStoreFormat::Migratable {
                    found: t.to_string(),
                };
            }
            return DetectedStoreFormat::Incompatible {
                found: t.to_string(),
            };
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return DetectedStoreFormat::Unreadable {
                err: error.to_string(),
            };
        }
    }
    match has_journal_segments(dir) {
        Ok(true) => DetectedStoreFormat::Migratable { found: "1".into() },
        Ok(false) => DetectedStoreFormat::Empty,
        Err(err) => DetectedStoreFormat::Unreadable { err },
    }
}

pub fn should_rotate(seg_bytes: u64, seg_records: u32) -> bool {
    seg_records >= ROTATE_RECORDS || seg_bytes >= ROTATE_BYTES
}
