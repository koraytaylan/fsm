use std::path::Path;

use fsm_core::record::{Record, verify_line, zeros};

use super::paths::seg_name;
use super::{journal_dir, journal_segment_paths};

pub fn load_records(dir: &Path) -> Result<Vec<Record>, String> {
    load_records_with_active_meta(dir, FinalTailPolicy::Reject).map(|(records, _)| records)
}

/// Load the authoritative prefix, stopping at a torn final record instead of
/// refusing the whole journal.
///
/// For a diagnosis only. An *open* must reject a torn tail — a store whose
/// last record is half-written is not a store to serve from — but "how much
/// of this journal is intact" is exactly the question a damaged store is
/// asked, and refusing to answer it is not an answer.
pub fn load_intact_prefix(dir: &Path) -> Result<Vec<Record>, String> {
    load_records_with_active_meta(dir, FinalTailPolicy::Ignore).map(|(records, _)| records)
}

type ActiveSegmentMeta = (String, u64, u64, u32);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FinalTailPolicy {
    Reject,
    Ignore,
}

/// Load and verify one authoritative record prefix while deriving its active
/// segment metadata from those same bounded reads. A read-only caller must not
/// re-list or re-stat the journal afterward: a live writer may advance between
/// independent observations.
pub(super) fn load_records_with_active_meta(
    dir: &Path,
    final_tail_policy: FinalTailPolicy,
) -> Result<(Vec<Record>, ActiveSegmentMeta), String> {
    let jdir = journal_dir(dir);
    let segs = journal_segment_paths(&jdir)?;
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    let mut active = (seg_name(0), 0, 0, 0);
    for (index, path) in segs.iter().enumerate() {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| seg_name(0));
        let first = name
            .strip_prefix("seg-")
            .and_then(|value| value.strip_suffix(".jsonl"))
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let mut segment_records = 0u32;
        let mut visible_bytes = None;
        let mut reader = crate::CappedLineReader::open(path, crate::PERSISTENCE_READ_CAP)
            .map_err(|error| format!("read journal segment {}: {error}", path.display()))?;
        while let Some(line) = reader
            .next_line()
            .map_err(|error| format!("read journal segment {}: {error}", path.display()))?
        {
            if !line.terminated {
                if index + 1 == segs.len() && final_tail_policy == FinalTailPolicy::Ignore {
                    visible_bytes = Some(line.start);
                    break;
                }
                return Err(format!(
                    "unterminated journal record in {} at offset {}",
                    path.display(),
                    line.start
                ));
            }
            if line.bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            let rec = verify_line(&line.bytes, expect, &prev).map_err(|e| format!("{e:?}"))?;
            expect = rec.seq + 1;
            prev = rec.hash.clone();
            out.push(rec);
            segment_records = segment_records.saturating_add(1);
        }
        if index + 1 == segs.len() {
            active = (
                name,
                first,
                visible_bytes.unwrap_or_else(|| reader.position()),
                segment_records,
            );
        }
    }
    Ok((out, active))
}

pub(super) fn load_records_before_offset(
    dir: &Path,
    torn_segment: &str,
    offset: u64,
) -> Result<Vec<Record>, String> {
    let jdir = journal_dir(dir);
    let segs = journal_segment_paths(&jdir)?;
    let mut out = Vec::new();
    let mut expect = 0u64;
    let mut prev = zeros();
    for path in segs {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name > torn_segment {
            break;
        }
        let is_torn = name == torn_segment;
        let mut reader = crate::CappedLineReader::open(&path, crate::PERSISTENCE_READ_CAP)
            .map_err(|error| format!("read journal segment {}: {error}", path.display()))?;
        loop {
            if is_torn && reader.position() >= offset {
                break;
            }
            let Some(line) = reader
                .next_line()
                .map_err(|error| format!("read journal segment {}: {error}", path.display()))?
            else {
                break;
            };
            if is_torn && line.end > offset {
                return Err("stale torn-tail offset".into());
            }
            if line.bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
                continue;
            }
            let rec = verify_line(&line.bytes, expect, &prev).map_err(|e| format!("{e:?}"))?;
            expect = rec.seq + 1;
            prev = rec.hash.clone();
            out.push(rec);
        }
    }
    Ok(out)
}
