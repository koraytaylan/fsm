use std::path::Path;

use fsm_core::record::{verify_line, zeros};
use fsm_core::replay::{NopSink, fold_with};

use super::classify::{classify, replay_health};
use super::load::load_records;
use super::types::JournalHealth;
use super::{
    DetectedStoreFormat, STORE_VERSION, detect_store_format, journal_dir, journal_segment_paths,
};

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
    for (si, path) in segs.iter().enumerate() {
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
                    if first.is_none() {
                        first = Some(rec.seq);
                    }
                    last = Some(rec.seq);
                    expect_seq = rec.seq + 1;
                    expect_prev = rec.hash;
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
