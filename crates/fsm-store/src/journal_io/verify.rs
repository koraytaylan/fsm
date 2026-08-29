use std::path::Path;

use fsm_core::record::verify_line;
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

/// What a verification of a sealed store actually walked.
///
/// The rule this project cannot compromise on: **a verification that did not
/// read the sealed bytes never reports the same thing as one that did.** So
/// this is three values and not a boolean beside an optional field, because a
/// caller can overlook an optional field and cannot overlook an enum arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealVerdict {
    /// The store is not sealed. Exactly today's answer, and today's exit code.
    Unsealed,
    /// Sealed, and the archive was not presented. The live suffix was walked
    /// in full and the seal was checked against the base; the prefix was not
    /// read at all — not partially, not optimistically.
    PrefixNotPresented,
    /// Sealed, and the archive was presented and walked: the manifest, every
    /// segment digest, and the record at the cut hashing to `sealed_last_hash`.
    /// The only verdict that may report what a complete walk reports.
    PrefixWalked,
    /// Sealed, and the archive that was presented is not this store's — it
    /// does not verify, or it seals a different prefix.
    ///
    /// **The store is healthy in this verdict.** A mistyped `--with-archive`
    /// path is a fault in the argument, not in the data directory, and
    /// reporting it as `base_mismatch` tells an operator their store is
    /// unopenable and beyond repair when nothing is wrong with it. What was
    /// not read is still not read, so this exits like
    /// [`SealVerdict::PrefixNotPresented`] and says why in `archive_detail`.
    PrefixNotMatched,
}

/// Everything a sealed store's verification says about its seal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealInfo {
    pub sealed_through_seq: u64,
    pub sealed_last_hash: String,
    pub archive_id: String,
    pub records_sealed: u64,
    pub verdict: SealVerdict,
    /// Where the archive was read from, when it was. A log line that does not
    /// record which bytes were walked is a log line that proves nothing.
    pub archive_dir: Option<String>,
    /// Why a presented archive was not walked, when one was presented and the
    /// verdict is [`SealVerdict::PrefixNotMatched`].
    pub archive_detail: Option<String>,
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
    /// Absent for an unsealed store, which is byte-identical to before.
    pub seal: Option<SealInfo>,
}

impl VerifyReport {
    /// The verdict, as one word, for a caller that reports rather than branches.
    pub fn seal_verdict(&self) -> SealVerdict {
        self.seal
            .as_ref()
            .map(|seal| seal.verdict)
            .unwrap_or(SealVerdict::Unsealed)
    }
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
    let start = super::chain_start(dir);
    let segs = super::live_segments(segs, &start);
    let mut out = Vec::new();
    let mut expect_seq = start.expect_seq;
    let mut expect_prev = start.expect_prev;
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
    verify_with_archive(dir, None)
}

/// The same verification, additionally walking a presented archive.
///
/// Absent an archive directory the sealed prefix is not read **at all**. That
/// is the whole point of the middle verdict: an operator who wants the
/// complete claim has to present the bytes it is a claim about.
pub fn verify_with_archive(dir: &Path, archive_dir: Option<&Path>) -> VerifyReport {
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
        seal: None,
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
    let seal = match seal_of(dir, &recs, archive_dir) {
        Ok(seal) => seal,
        Err(health) => return empty(health),
    };
    // Two different questions, and only one of them is about the seal record.
    // *Whether a prefix is sealed* is answered by that record. *Where the fold
    // starts* is answered by what is on disk: between the commit point and the
    // removal both the base and the copied segments are present, and folding
    // the records onto the base would apply the prefix twice.
    let folded = if super::chain_start(dir).is_origin() {
        fold_with(recs.clone(), &mut NopSink)
    } else {
        match crate::base::open_from_base(dir, &recs) {
            Ok(opened) => fsm_core::replay::fold_from(opened.state, recs.clone(), &mut NopSink),
            Err(error) => {
                return empty(JournalHealth::BaseMismatch {
                    detail: error.message,
                });
            }
        }
    };
    match folded {
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
                seal,
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
            seal,
        },
    }
}

/// The seal a store carries, without verifying or folding anything.
///
/// `verify` answers this on its way past, and callers that have already
/// walked a store should take it from the report they already hold. This is
/// for the ones that have not: it loads the live records and reads the seal
/// record out of them, which is the whole cost. Asking `verify` for a seal
/// runs a classify, a load, a full fold, and a segment walk to return two
/// scalars — on a large journal that is the difference between a diagnostic an
/// operator runs and one they learn not to.
pub fn seal_at(dir: &Path) -> Option<SealInfo> {
    let records = load_records(dir).ok()?;
    seal_of(dir, &records, None).ok().flatten()
}

/// The seal a store carries, and what verifying it actually read.
pub(crate) fn seal_of(
    dir: &Path,
    records: &[fsm_core::record::Record],
    archive_dir: Option<&Path>,
) -> Result<Option<SealInfo>, JournalHealth> {
    // A store carries a seal when its live journal holds a seal record —
    // which is true from the commit point onward, including the window before
    // the copied segments are removed, where the store is *also* still
    // complete. "Is a prefix sealed" and "does the fold start from the base"
    // are two questions, and only the second depends on what is on disk.
    if !records
        .iter()
        .any(|record| record.kind == fsm_core::record::RecordKind::JournalSealed)
    {
        return Ok(None);
    }
    let seal = crate::base::open_from_base(dir, records)
        .map_err(|error| JournalHealth::BaseMismatch {
            detail: error.message,
        })?
        .seal;
    let mut info = SealInfo {
        sealed_through_seq: seal.sealed_through_seq,
        sealed_last_hash: seal.sealed_last_hash.clone(),
        archive_id: seal.archive_id.clone(),
        records_sealed: seal.records_sealed,
        verdict: SealVerdict::PrefixNotPresented,
        archive_dir: None,
        archive_detail: None,
    };
    let Some(archive) = archive_dir else {
        return Ok(Some(info));
    };
    // From here on, every disagreement is about the *presented directory*.
    // None of it says anything about this store, which has already been
    // classified `Ok` and folded, so none of it may condemn it.
    let mut refuse = |detail: String| {
        info.verdict = SealVerdict::PrefixNotMatched;
        info.archive_dir = Some(archive.display().to_string());
        info.archive_detail = Some(detail);
        Ok(Some(info.clone()))
    };
    let manifest = match crate::archive::verify(archive) {
        Ok(manifest) => manifest,
        Err(error) => {
            return refuse(format!(
                "the presented archive does not verify: {}",
                error.message
            ));
        }
    };
    if manifest.sealed_through_seq != seal.sealed_through_seq
        || manifest.sealed_last_hash != seal.sealed_last_hash
    {
        return refuse(format!(
            "the presented archive seals through seq {} at {}, and this store's seal names seq \
             {} at {}",
            manifest.sealed_through_seq,
            manifest.sealed_last_hash,
            seal.sealed_through_seq,
            seal.sealed_last_hash
        ));
    }
    if manifest.archive_id() != seal.archive_id {
        return refuse("the presented archive is not the one this store's seal names".into());
    }
    info.verdict = SealVerdict::PrefixWalked;
    info.archive_dir = Some(archive.display().to_string());
    Ok(Some(info))
}
