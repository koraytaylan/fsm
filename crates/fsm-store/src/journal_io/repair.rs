use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use fsm_core::replay::{NopSink, fold_with};

use super::classify::{classify, replay_health};
use super::init::{stamp_store_version, write_genesis_unlocked};
use super::load::{load_records, load_records_before_offset};
use super::paths::{acquire_lock, sync_dir};
use super::types::{JournalHealth, JournalIoError};
use super::verify::refuse_incompatible_store_format;
use super::{DetectedStoreFormat, detect_store_format, journal_dir};

#[derive(Debug)]
pub struct RepairReport {
    pub quarantined: PathBuf,
    pub bytes: u64,
    pub truncated_to_seq: u64,
}

#[derive(Debug)]
/// Failure to perform the explicitly mutating torn-tail repair operation.
pub enum RepairError {
    /// The journal is healthy and needs no torn-tail repair.
    NothingToRepair,
    /// Journal content has a health problem that this repair cannot change.
    Interior(JournalHealth),
    /// Authoritative bytes could not be read before repair.
    ReadIo(String),
    /// Repair output could not be written durably.
    WriteIo(String),
}

pub fn repair_truncate_torn_tail(dir: &Path) -> Result<RepairReport, RepairError> {
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(RepairError::Interior(h));
    }
    let jdir = journal_dir(dir);
    let _lock = acquire_lock(&jdir).map_err(|error| match error {
        JournalIoError::Locked { pid } => {
            RepairError::Interior(JournalHealth::LockIo(format!("locked {pid}")))
        }
        other => RepairError::WriteIo(other.to_string()),
    })?;
    let migrating = matches!(
        detect_store_format(dir),
        DetectedStoreFormat::Migratable { .. }
    );
    let health = classify(dir);
    match health {
        JournalHealth::Ok => Err(RepairError::NothingToRepair),
        JournalHealth::TornTail {
            segment,
            offset,
            bytes,
        } => {
            let path = jdir.join(&segment);
            let tail =
                crate::read_regular_range_capped(&path, offset, bytes, crate::PERSISTENCE_READ_CAP)
                    .map_err(|error| RepairError::ReadIo(error.to_string()))?;
            let chain =
                load_records_before_offset(dir, &segment, offset).map_err(RepairError::ReadIo)?;
            if let Err(e) = fold_with(chain.clone(), &mut NopSink) {
                return Err(RepairError::Interior(replay_health(e)));
            }
            let qdir = jdir.join("quarantine");
            crate::ensure_persistence_directory(&qdir)
                .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            let seg_first = segment
                .strip_prefix("seg-")
                .and_then(|s| s.strip_suffix(".jsonl"))
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let first_bad = chain.last().map(|r| r.seq + 1).unwrap_or(seg_first);
            let mut qpath = qdir.join(format!("{segment}-tail-{first_bad}.bin"));
            let qf = match OpenOptions::new().write(true).create_new(true).open(&qpath) {
                Ok(file) => file,
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or(0);
                    qpath = qdir.join(format!(
                        "{segment}-tail-{first_bad}-{}-{nonce}.bin",
                        std::process::id()
                    ));
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&qpath)
                        .map_err(|error| RepairError::WriteIo(error.to_string()))?
                }
                Err(error) => return Err(RepairError::WriteIo(error.to_string())),
            };
            {
                let mut qf = qf;
                qf.write_all(&tail)
                    .map_err(|e| RepairError::WriteIo(e.to_string()))?;
                qf.sync_all()
                    .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            }
            sync_dir(&qdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            let f = crate::open_regular_file_for_write(
                &path,
                crate::PersistenceCreate::RequireExisting,
                crate::PersistenceWriteMode::Update,
            )
            .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            f.set_len(offset)
                .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            f.sync_all()
                .map_err(|e| RepairError::WriteIo(e.to_string()))?;
            sync_dir(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            if offset == 0 && seg_first == 0 {
                write_genesis_unlocked(&jdir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            }
            let after = classify(dir);
            if !matches!(after, JournalHealth::Ok) {
                return Err(RepairError::Interior(after));
            }
            let kept = load_records(dir).map_err(RepairError::ReadIo)?;
            if let Err(e) = fold_with(kept, &mut NopSink) {
                return Err(RepairError::Interior(replay_health(e)));
            }
            if migrating {
                // A successful repair has folded the complete retained journal
                // under current semantics — the migration success condition.
                stamp_store_version(dir).map_err(|e| RepairError::WriteIo(e.to_string()))?;
            }
            Ok(RepairReport {
                quarantined: qpath,
                bytes,
                truncated_to_seq: chain
                    .last()
                    .map(|r| r.seq)
                    .unwrap_or(seg_first.saturating_sub(1)),
            })
        }
        other => Err(RepairError::Interior(other)),
    }
}
