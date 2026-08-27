use std::path::Path;

use fsm_core::record::{verify_line, zeros};
use fsm_core::replay::{NopSink, fold_with};

use super::classify::{classify, replay_health};
use super::load::load_records;
use super::types::JournalHealth;
use super::{
    DetectedStoreFormat, STORE_VERSION, detect_store_format, journal_dir, journal_segment_paths,
};

/// How many records pass between two calls to a verification callback.
///
/// Small enough that a cancelled call stops promptly and a progress bar
/// moves; large enough that the callback is not the cost of verifying.
pub const BATCH: u64 = 256;

/// What a caller wants after another batch of records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walk {
    /// Keep verifying.
    Continue,
    /// Stop here. What has been verified stays verified; the segment's
    /// status is whatever the records already read said it was.
    Stop,
}

#[derive(Debug, Clone)]
pub struct SegmentProgress {
    pub segment: String,
    pub records: u64,
    pub first_seq: Option<u64>,
    pub last_seq: Option<u64>,
    pub status: String,
}

pub struct VerifyReport {
    pub health: JournalHealth,
    pub records: u64,
    pub machines: u64,
    pub instances: u64,
    pub instance_hashes: std::collections::BTreeMap<String, String>,
    pub segments: Vec<SegmentProgress>,
    pub store_version: Option<String>,
    pub migratable: bool,
}

pub fn refuse_incompatible_store_format(dir: &Path) -> Result<(), JournalHealth> {
    match detect_store_format(dir) {
        DetectedStoreFormat::Incompatible { found } => {
            Err(JournalHealth::VersionMismatch { found })
        }
        DetectedStoreFormat::Unreadable { err } => Err(JournalHealth::StoreIo(format!(
            "cannot inspect store format: {err}"
        ))),
        _ => Ok(()),
    }
}

pub fn verify_segments(dir: &Path) -> Vec<SegmentProgress> {
    verify_segments_with(dir, &mut |_, _| Walk::Continue)
}

/// The same walk, reporting to a caller every [`BATCH`] records and stopping
/// when it says so.
///
/// The callback is handed the running record count and the last verified
/// seq. Nothing about what verification *decides* depends on it: this is the
/// same loop, with a place to stand.
pub fn verify_segments_with(
    dir: &Path,
    on_batch: &mut dyn FnMut(u64, u64) -> Walk,
) -> Vec<SegmentProgress> {
    let jdir = journal_dir(dir);
    let segs = match journal_segment_paths(&jdir) {
        Ok(segments) => segments,
        Err(_) => {
            return vec![SegmentProgress {
                segment: "journal".into(),
                records: 0,
                first_seq: None,
                last_seq: None,
                status: "metadata-failure".into(),
            }];
        }
    };
    let mut out = Vec::new();
    let mut expect_seq = 0u64;
    let mut expect_prev = zeros();
    let mut walked = 0u64;
    let mut stopped = false;
    for (si, path) in segs.iter().enumerate() {
        if stopped {
            break;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut reader = match crate::CappedLineReader::open(path, crate::PERSISTENCE_READ_CAP) {
            Ok(reader) => reader,
            Err(_) => {
                out.push(SegmentProgress {
                    segment: name,
                    records: 0,
                    first_seq: None,
                    last_seq: None,
                    status: "metadata-failure".into(),
                });
                continue;
            }
        };
        let last_seg = si + 1 == segs.len();
        let mut records = 0u64;
        let mut first = None;
        let mut last = None;
        let mut status = "ok".to_string();
        let mut saw_line = false;
        loop {
            let line = match reader.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(_) => {
                    status = "metadata-failure".into();
                    break;
                }
            };
            saw_line = true;
            if !line.terminated {
                status = if last_seg { "torn" } else { "broken" }.into();
                break;
            }
            match verify_line(&line.bytes, expect_seq, &expect_prev) {
                Ok(rec) => {
                    records += 1;
                    walked += 1;
                    if first.is_none() {
                        first = Some(rec.seq);
                    }
                    last = Some(rec.seq);
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
                    if walked.is_multiple_of(BATCH) && on_batch(walked, rec.seq) == Walk::Stop {
                        stopped = true;
                        break;
                    }
                }
                Err(_) => {
                    status = "broken".into();
                    break;
                }
            }
        }
        if records == 0 && status == "ok" && !saw_line {
            status = "empty".into();
        }
        out.push(SegmentProgress {
            segment: name,
            records,
            first_seq: first,
            last_seq: last,
            status,
        });
    }
    // One last report, so a caller sees the final count even when the walk
    // ended mid-batch — and can still stop a caller-visible operation that
    // has nothing left to do.
    if !stopped {
        let last_seq = expect_seq.saturating_sub(1);
        let _ = on_batch(walked, last_seq);
    }
    out
}

pub fn verify(dir: &Path) -> VerifyReport {
    let fmt = detect_store_format(dir);
    let store_version = match &fmt {
        DetectedStoreFormat::Current => Some(STORE_VERSION.to_string()),
        DetectedStoreFormat::Migratable { found } | DetectedStoreFormat::Incompatible { found } => {
            Some(found.clone())
        }
        DetectedStoreFormat::Empty | DetectedStoreFormat::Unreadable { .. } => None,
    };
    let migratable = matches!(&fmt, DetectedStoreFormat::Migratable { .. });
    let empty = |health: JournalHealth| VerifyReport {
        health,
        records: 0,
        machines: 0,
        instances: 0,
        instance_hashes: Default::default(),
        segments: verify_segments(dir),
        store_version: store_version.clone(),
        migratable,
    };
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return empty(h);
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return empty(health);
    }
    let recs = match load_records(dir) {
        Ok(r) => r,
        Err(e) => return empty(JournalHealth::StoreIo(e)),
    };
    match fold_with(recs.clone(), &mut NopSink) {
        Ok(st) => {
            let mut instance_hashes = std::collections::BTreeMap::new();
            for (id, inst) in &st.instances {
                let mid = st.instance_machines.get(id).cloned().unwrap_or_default();
                instance_hashes.insert(
                    id.clone(),
                    fsm_core::hashes::state_hash(&mid, id, st.last_seq, inst),
                );
            }
            VerifyReport {
                health: JournalHealth::Ok,
                records: recs.len() as u64,
                machines: st.machines.len() as u64,
                instances: st.instances.len() as u64,
                instance_hashes,
                segments: verify_segments(dir),
                store_version,
                migratable,
            }
        }
        Err(e) => VerifyReport {
            health: replay_health(e),
            records: recs.len() as u64,
            machines: 0,
            instances: 0,
            instance_hashes: Default::default(),
            segments: verify_segments(dir),
            store_version,
            migratable,
        },
    }
}
