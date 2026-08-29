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
    BATCH, SealInfo, SealVerdict, SegmentProgress, VerifyReport, Walk,
    refuse_incompatible_store_format, seal_at, verify, verify_segments, verify_segments_with,
    verify_with_archive,
};

const ROTATE_RECORDS: u32 = 65_536;
const ROTATE_BYTES: u64 = 64 * 1024 * 1024;
// VERSION 6 adds the `state_checkpoint` record used to bind explicit
// snapshots without rewriting prior journal records. VERSION 7 adds
// `request_fp` to every record that claims a `request_id`. VERSION 8 adds
// parallel active configurations and durable deadline schedules, with explicit
// state-hash and state-root format discriminators. VERSION 9 adds the
// composition records and `fsm.state/3`. VERSION 10 adds the `journal_sealed`
// record and the `fsm.base/1` state file a sealed store opens from.
//
// Every earlier version (and a journal with no VERSION marker) is best-effort
// migrated on open by folding the complete journal with snapshot caches
// ignored, then stamping the current one. Records are never rewritten, so
// legacy records retain their historical hash material and request-id
// behavior.
//
// The 9-to-10 step converts **nothing**. A pre-10 store has no seal record and
// no base file, so migrating it is a stamp — which is exactly why it is said
// here rather than left as an empty arm somebody later fills in.
/// Current marker written to a store's `VERSION` file.
pub const STORE_VERSION: &str = "10";

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
/// Whether a `VERSION` older than the current one is migrated forward.
///
/// The 9-to-10 step is a **stamp and nothing else**, and that is worth saying
/// out loud: a pre-10 store has no seal record and no base state file, so
/// there is nothing to convert. A migration arm that does no work is one a
/// later reader assumes was left unfinished and helpfully completes.
fn is_migratable_version(v: &str) -> bool {
    matches!(v, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

/// Classify an on-disk store directory without opening or locking it.
pub fn detect_store_format(dir: &Path) -> DetectedStoreFormat {
    // A path that exists and is not a directory is not a store, and saying so
    // here is the only portable way to say it: reading `<file>/VERSION`
    // fails with `ENOTDIR` on Unix, which falls to `Unreadable` below, but
    // with `NotFound` on Windows, which would read as "no store here yet"
    // and open an empty one over somebody's file.
    if dir.exists() && !dir.is_dir() {
        return DetectedStoreFormat::Unreadable {
            err: format!("{} is not a directory", dir.display()),
        };
    }
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

/// Where a journal's chain begins.
///
/// An unsealed journal begins at the origin: sequence 0 with a zero
/// predecessor. A sealed one begins just past its cut, with the hash of the
/// record at the cut as its predecessor — both read from the base state file,
/// and both re-checked against the seal record the live suffix carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainStart {
    pub expect_seq: u64,
    pub expect_prev: String,
}

impl Default for ChainStart {
    fn default() -> Self {
        Self {
            expect_seq: 0,
            expect_prev: fsm_core::record::zeros(),
        }
    }
}

impl ChainStart {
    pub fn is_origin(&self) -> bool {
        *self == Self::default()
    }
}

/// The chain start a data directory's base state file declares.
///
/// Every reader in this module resolves it rather than assuming the origin, so
/// a sealed store loads, classifies, and verifies correctly through the paths
/// that already exist instead of through a second set of them.
pub fn chain_start(dir: &Path) -> ChainStart {
    // A base is consulted only when the live journal actually starts above the
    // origin. Before a seal's commit point an interrupted run can have written
    // `BASE` already, and that file is **inert**: nothing in the chain
    // references it, every record it describes is still on disk, and a loader
    // that trusted it would skip segments it has — turning a survivable
    // interruption into a store that does not open. That is the other half of
    // "before step 7 the new files are inert".
    if journal_starts_at_origin(dir) {
        return ChainStart::default();
    }
    match crate::base::read_header(dir) {
        Ok(Some(header)) => ChainStart {
            expect_seq: header.seq.saturating_add(1),
            expect_prev: header.last_hash,
        },
        // An unreadable base is not silently treated as absent: the open path
        // refuses it with `store/base_mismatch` after the load, and a
        // classification that guessed the origin here would report a chain
        // break instead of the real fault.
        Ok(None) | Err(_) => ChainStart::default(),
    }
}

/// Whether the lowest-numbered segment on disk begins at sequence zero.
fn journal_starts_at_origin(dir: &Path) -> bool {
    let Ok(segments) = journal_segment_paths(&journal_dir(dir)) else {
        return true;
    };
    segments
        .first()
        .and_then(|path| segment_first_seq(path))
        .is_none_or(|first| first == 0)
}

/// The first sequence a segment file's name declares.
pub(crate) fn segment_first_seq(path: &Path) -> Option<u64> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("seg-"))
        .and_then(|rest| rest.strip_suffix(".jsonl"))
        .and_then(|digits| digits.parse::<u64>().ok())
}

/// Drop the segments a seal already archived.
///
/// Between appending the seal record and removing the copied segments there is
/// a window, and a store interrupted inside it opens with sealed segments still
/// on disk. Their records are below the seal and already in the archive, so the
/// loader skips them **by sequence** — which is the other half of why
/// copy-then-seal-then-remove is safe. Segments are whole and a cut is always
/// segment-final, so a leftover sealed segment lies entirely below the start.
pub(crate) fn live_segments(segments: Vec<PathBuf>, start: &ChainStart) -> Vec<PathBuf> {
    segments
        .into_iter()
        .filter(|path| segment_first_seq(path).is_none_or(|first| first >= start.expect_seq))
        .collect()
}

pub fn should_rotate(seg_bytes: u64, seg_records: u32) -> bool {
    seg_records >= ROTATE_RECORDS || seg_bytes >= ROTATE_BYTES
}
