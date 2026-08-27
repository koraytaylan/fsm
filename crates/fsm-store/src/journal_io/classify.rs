use std::path::Path;

use fsm_core::record::{RecordError, RecordKind, verify_line, zeros};

use super::types::JournalHealth;
use super::{journal_dir, journal_segment_paths};

/// Everything a diagnosis reports about one data directory.
///
/// One computation with two renderers: `fsm doctor` prints it and
/// `store_doctor` returns it. Two implementations of "what is wrong with
/// this store" would eventually disagree, and an operator holding two
/// answers has no answer.
#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub health: JournalHealth,
    /// The store format found on disk, empty when there is none.
    pub version: String,
    /// Present when the format is older but migratable.
    pub migration_required_from: Option<String>,
    /// Whether a read-only open succeeds. A `false` here is the case the
    /// degraded serve mode exists for.
    pub readable: bool,
    pub records: u64,
    pub segments: Vec<super::verify::SegmentProgress>,
    /// How many snapshot caches are on disk, and how far behind the newest
    /// one is. A snapshot's *presence* tells an operator nothing; how stale
    /// it is tells them whether opens are paying for the journal tail.
    pub snapshots: usize,
    pub snapshot_seq: Option<u64>,
    pub snapshot_behind: u64,
    /// Whether somebody holds the writer, and which process if the lock file
    /// says.
    pub writer_lock_held: bool,
    pub writer_lock_holder: Option<u32>,
    /// Running children nobody references. Reported, never settled: an open
    /// must not write.
    pub orphans: Vec<fsm_core::json::Value>,
}

/// Diagnose one data directory without opening it for writing.
///
/// Never requires a healthy open: the store most in need of a diagnosis is
/// the one that will not open, which is exactly the case the degraded serve
/// mode serves.
pub fn diagnose(dir: &Path) -> Diagnosis {
    let health = classify(dir);
    let format = super::detect_store_format(dir);
    let version = match &format {
        super::DetectedStoreFormat::Current => super::STORE_VERSION.to_string(),
        super::DetectedStoreFormat::Migratable { found }
        | super::DetectedStoreFormat::Incompatible { found } => found.clone(),
        super::DetectedStoreFormat::Empty | super::DetectedStoreFormat::Unreadable { .. } => {
            String::new()
        }
    };
    let migration_required_from = match &format {
        super::DetectedStoreFormat::Migratable { found } => Some(found.clone()),
        _ => None,
    };
    let segments = super::verify::verify_segments(dir);
    let records = segments.iter().map(|segment| segment.records).sum();
    let snaps = crate::snapshot::listed_snaps(dir);
    let snapshot_seq = snaps.iter().map(|(seq, _)| *seq).max();
    let last_seq = segments
        .iter()
        .filter_map(|segment| segment.last_seq)
        .max()
        .unwrap_or(0);
    let snapshot_behind = snapshot_seq
        .map(|seq| last_seq.saturating_sub(seq))
        .unwrap_or(0);
    let (writer_lock_held, writer_lock_holder) = writer_lock(dir);
    let store = crate::store::Store::open_read_only(dir);
    let readable = store.is_ok();
    let orphans = store
        .map(|store| store.orphaned_children())
        .unwrap_or_default();
    Diagnosis {
        health,
        version,
        migration_required_from,
        readable,
        records,
        segments,
        snapshots: snaps.len(),
        snapshot_seq,
        snapshot_behind,
        writer_lock_held,
        writer_lock_holder,
        orphans,
    }
}

/// Whether a writer holds the store, by asking the lock rather than by
/// taking it.
///
/// The probe is a **shared** lock, held for the length of this call. An
/// exclusive holder makes it fail, which is the answer wanted; the cost is
/// that a writer starting up inside this instant could see the store as
/// busy. That window is microseconds long and only opens while somebody is
/// running a diagnosis — the alternative, reading a pid and guessing whether
/// that process is alive, is not portable and would be a guess.
fn writer_lock(dir: &Path) -> (bool, Option<u32>) {
    let path = journal_dir(dir).join("LOCK");
    let Ok(file) = std::fs::File::open(&path) else {
        return (false, None);
    };
    match file.try_lock_shared() {
        Ok(()) => {
            let _ = file.unlock();
            (false, None)
        }
        Err(std::fs::TryLockError::WouldBlock) => {
            let holder = std::fs::read(&path)
                .ok()
                .and_then(|bytes| {
                    fsm_core::json::parse(&bytes, &fsm_core::json::JsonLimits::DEFAULT).ok()
                })
                .and_then(|v| {
                    v.get("pid")
                        .and_then(fsm_core::json::Value::as_num)
                        .and_then(|pid| pid.parse().ok())
                })
                // A server diagnosing the store it is itself holding would
                // otherwise report its own pid, which tells the operator
                // nothing they did not know and makes the answer differ
                // between two identical runs. "Held, by somebody else" is
                // the fact worth naming.
                .filter(|pid| *pid != std::process::id());
            (true, holder)
        }
        // A lock call that failed for any other reason says nothing about
        // whether anybody holds it, and inventing an answer here would be
        // worse than admitting there is none.
        Err(_) => (false, None),
    }
}

pub fn classify(dir: &Path) -> JournalHealth {
    let jdir = journal_dir(dir);
    let segs = match journal_segment_paths(&jdir) {
        Ok(segments) => segments,
        Err(error) => return JournalHealth::StoreIo(error),
    };
    if segs.is_empty() {
        return JournalHealth::MissingGenesis;
    }
    let mut expect_seq = 0u64;
    let mut expect_prev = zeros();
    let mut saw_record = false;
    for (si, path) in segs.iter().enumerate() {
        let last_seg = si + 1 == segs.len();
        let segn = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut reader = match crate::CappedLineReader::open(path, crate::PERSISTENCE_READ_CAP) {
            Ok(reader) => reader,
            Err(error) => {
                return JournalHealth::StoreIo(format!(
                    "read journal segment {}: {error}",
                    path.display()
                ));
            }
        };
        loop {
            let line = match reader.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(error) => {
                    return JournalHealth::StoreIo(format!(
                        "read journal segment {}: {error}",
                        path.display()
                    ));
                }
            };
            if !line.terminated {
                if last_seg {
                    return JournalHealth::TornTail {
                        segment: segn.clone(),
                        offset: line.start,
                        bytes: line.end.saturating_sub(line.start),
                    };
                }
                return JournalHealth::ChainBroken {
                    seq: expect_seq,
                    segment: segn.clone(),
                    offset: line.start,
                    expected: expect_prev.clone(),
                    found: "unterminated record in a non-final segment".into(),
                };
            }
            match verify_line(&line.bytes, expect_seq, &expect_prev) {
                Ok(rec) => {
                    if rec.seq == 0 && rec.kind != RecordKind::Genesis {
                        return JournalHealth::ChainBroken {
                            seq: 0,
                            segment: segn.clone(),
                            offset: line.start,
                            expected: "genesis".into(),
                            found: format!("{:?}", rec.kind),
                        };
                    }
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
                    saw_record = true;
                }
                Err(RecordError::NonCanonical { seq, .. }) => {
                    return JournalHealth::NonCanonical {
                        seq,
                        segment: segn.clone(),
                        offset: line.start,
                    };
                }
                Err(RecordError::SeqGap { seq, expected }) => {
                    return JournalHealth::ChainBroken {
                        seq: expected,
                        segment: segn.clone(),
                        offset: line.start,
                        expected: expected.to_string(),
                        found: seq.to_string(),
                    };
                }
                Err(RecordError::HashMismatch { seq }) => {
                    return JournalHealth::ChainBroken {
                        seq,
                        segment: segn.clone(),
                        offset: line.start,
                        expected: expect_prev.clone(),
                        found: "hash".into(),
                    };
                }
                Err(RecordError::Parse { .. })
                | Err(RecordError::PrevMismatch { .. })
                | Err(RecordError::BodyInvalid { .. }) => {
                    return JournalHealth::ChainBroken {
                        seq: expect_seq,
                        segment: segn.clone(),
                        offset: line.start,
                        expected: expect_prev.clone(),
                        found: "parse".into(),
                    };
                }
            }
        }
    }
    if !saw_record {
        return JournalHealth::MissingGenesis;
    }
    JournalHealth::Ok
}

pub(super) fn replay_health(e: fsm_core::replay::ReplayError) -> JournalHealth {
    match e {
        fsm_core::replay::ReplayError::StateHashMismatch { seq, .. } => {
            JournalHealth::StateHashMismatch { seq }
        }
        fsm_core::replay::ReplayError::FieldMismatch { seq, field } => {
            JournalHealth::ReplayMismatch {
                seq,
                field: field.into(),
            }
        }
        fsm_core::replay::ReplayError::MicrostepMismatch { seq, index } => {
            JournalHealth::ReplayMismatch {
                seq,
                field: format!("microsteps[{index}]"),
            }
        }
        fsm_core::replay::ReplayError::UnknownMachine { seq } => JournalHealth::ReplayMismatch {
            seq,
            field: "machine".into(),
        },
        fsm_core::replay::ReplayError::UnknownInstance { seq } => JournalHealth::ReplayMismatch {
            seq,
            field: "instance".into(),
        },
    }
}
