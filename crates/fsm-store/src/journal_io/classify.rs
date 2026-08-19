use std::path::Path;

use fsm_core::record::{RecordError, RecordKind, verify_line, zeros};

use super::types::JournalHealth;
use super::{journal_dir, journal_segment_paths};

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
