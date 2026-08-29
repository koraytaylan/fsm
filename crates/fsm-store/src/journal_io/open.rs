use std::path::Path;

use fsm_core::record::{Record, zeros};
use fsm_core::replay::{RecordSink, StoreState, fold_with};

use super::classify::{classify, replay_health};
use super::init::{stamp_store_version, write_genesis_unlocked, write_version_durable};
use super::load::{FinalTailPolicy, load_records_with_active_meta};
use super::paths::{acquire_lock, seg_name};
use super::types::{Journal, JournalHealth, JournalIoError, OpenError, Seg};
use super::verify::refuse_incompatible_store_format;
use super::{DetectedStoreFormat, chain_start, detect_store_format, journal_dir};

pub fn open(
    dir: &Path,
    sink: &mut impl RecordSink,
) -> Result<(Journal, StoreState, crate::snapshot::OpenPath), OpenError> {
    let jdir = journal_dir(dir);
    // Existing output directories are validated as writer destinations before
    // format probing. Missing directories are not created until an existing
    // incompatible VERSION marker has been refused without mutation.
    crate::persistence_directory_exists(&jdir)
        .map_err(|error| OpenError::WriteIo(error.to_string()))?;
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(OpenError::Health(h));
    }
    crate::ensure_persistence_directory(&jdir)
        .map_err(|error| OpenError::WriteIo(error.to_string()))?;
    let lock = acquire_lock(&jdir).map_err(|e| match e {
        JournalIoError::Locked { pid } => {
            OpenError::Health(JournalHealth::LockIo(format!("locked {pid}")))
        }
        other => OpenError::WriteIo(other.to_string()),
    })?;
    if let Err(h) = refuse_incompatible_store_format(dir) {
        return Err(OpenError::Health(h));
    }
    let fmt = detect_store_format(dir);
    if matches!(fmt, DetectedStoreFormat::Empty) {
        write_version_durable(dir).map_err(|e| OpenError::WriteIo(e.to_string()))?;
    }
    let migrating = matches!(fmt, DetectedStoreFormat::Migratable { .. });
    let health = classify(dir);
    if matches!(health, JournalHealth::MissingGenesis) {
        // Auto-genesis only completes a store this build created (fresh dir,
        // or a crash between VERSION and genesis). A Migratable dir missing
        // its journal is lost data, not a store to re-create over.
        if migrating {
            return Err(OpenError::Health(JournalHealth::MissingGenesis));
        }
        write_genesis_unlocked(&jdir).map_err(|e| OpenError::WriteIo(e.to_string()))?;
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok) {
        return Err(OpenError::Health(health));
    }
    let start = chain_start(dir);
    let (recs, (name, first, bytes, count)) =
        load_records_with_active_meta(dir, FinalTailPolicy::Reject, &start)
            .map_err(OpenError::ReadIo)?;
    let (state, open_path) = if !start.is_origin() {
        sealed_state(dir, &recs, sink, true)?
    } else if migrating {
        // Migration ignores snapshot caches and folds the complete journal
        // under current semantics before certifying the store.
        let n = recs.len();
        let state =
            fold_with(recs.clone(), sink).map_err(|e| OpenError::Health(replay_health(e)))?;
        (
            state,
            crate::snapshot::OpenPath {
                replayed_records: n,
                used_snapshot: false,
                snapshot_seq: None,
            },
        )
    } else {
        crate::snapshot::open_state(dir, recs.clone(), sink)
            .map_err(|e| OpenError::Health(replay_health(e)))?
    };
    if migrating {
        stamp_store_version(dir).map_err(|e| OpenError::WriteIo(e.to_string()))?;
    }
    let last = recs.last();
    let path = jdir.join(&name);
    let seg = crate::open_regular_file_for_write(
        &path,
        crate::PersistenceCreate::CreateIfMissing,
        crate::PersistenceWriteMode::Append,
    )
    .map_err(|e| OpenError::WriteIo(e.to_string()))?;
    Ok((
        Journal {
            dir: dir.to_path_buf(),
            seg: Seg::File(seg),
            seg_name: name,
            seg_first_seq: first,
            seg_bytes: bytes,
            seg_records: count,
            last_seq: last.map(|r| r.seq).unwrap_or(0),
            last_hash: last.map(|r| r.hash.clone()).unwrap_or_else(zeros),
            poisoned: false,
            _lock: Some(lock),
            mem_records: None,
        },
        state,
        open_path,
    ))
}

/// Fold a sealed store's live suffix onto the base its seal authenticates.
///
/// A base that is present and wrong **refuses**. There is no fallback to a
/// complete fold, because there is nothing to fall back to: the records the
/// base replaced are in the archive, not in this directory. A fallback here
/// would silently serve a store assembled from state nobody authenticated,
/// which is the single worst thing this plan could introduce.
fn sealed_state(
    dir: &Path,
    records: &[Record],
    sink: &mut impl RecordSink,
    writable: bool,
) -> Result<(StoreState, crate::snapshot::OpenPath), OpenError> {
    let (base, _seal) = crate::base::open_from_base(dir, records).map_err(|error| {
        OpenError::Health(JournalHealth::BaseMismatch {
            detail: error.message,
        })
    })?;
    // The cache fast path survives sealing: a cache *above* the seal is still
    // bound and used, because everything that binds one can be re-derived from
    // the base plus the live records. A cache at or below it is skipped.
    if writable {
        crate::snapshot::open_state_from(dir, records.to_vec(), sink, base)
    } else {
        crate::snapshot::open_state_read_only_from(dir, records.to_vec(), sink, base)
    }
    .map_err(|error| OpenError::Health(replay_health(error)))
}

/// Open a store for inspection without creating anything, taking the writer
/// lock, stamping a migrated VERSION, or opening a segment for append.
pub(crate) fn open_read_only(
    dir: &Path,
    sink: &mut impl RecordSink,
) -> Result<(Journal, StoreState, crate::snapshot::OpenPath, Vec<Record>), OpenError> {
    let format = detect_store_format(dir);
    if matches!(format, DetectedStoreFormat::Empty) {
        return Ok((
            Journal {
                dir: dir.to_path_buf(),
                seg: Seg::ReadOnly,
                seg_name: seg_name(0),
                seg_first_seq: 0,
                seg_bytes: 0,
                seg_records: 0,
                last_seq: 0,
                last_hash: zeros(),
                poisoned: false,
                _lock: None,
                mem_records: None,
            },
            StoreState::default(),
            crate::snapshot::OpenPath::default(),
            Vec::new(),
        ));
    }
    if let Err(health) = refuse_incompatible_store_format(dir) {
        return Err(OpenError::Health(health));
    }
    let health = classify(dir);
    if !matches!(health, JournalHealth::Ok | JournalHealth::TornTail { .. }) {
        return Err(OpenError::Health(health));
    }
    let start = chain_start(dir);
    let (records, (name, first, bytes, count)) =
        load_records_with_active_meta(dir, FinalTailPolicy::Ignore, &start)
            .map_err(OpenError::ReadIo)?;
    let migrating = matches!(format, DetectedStoreFormat::Migratable { .. });
    let (state, open_path) = if !start.is_origin() {
        sealed_state(dir, &records, sink, false)?
    } else if migrating {
        let count = records.len();
        let state = fold_with(records.clone(), sink)
            .map_err(|error| OpenError::Health(replay_health(error)))?;
        (
            state,
            crate::snapshot::OpenPath {
                replayed_records: count,
                used_snapshot: false,
                snapshot_seq: None,
            },
        )
    } else {
        crate::snapshot::open_state_read_only(dir, records.clone(), sink)
            .map_err(|error| OpenError::Health(replay_health(error)))?
    };
    let last = records.last();
    Ok((
        Journal {
            dir: dir.to_path_buf(),
            seg: Seg::ReadOnly,
            seg_name: name,
            seg_first_seq: first,
            seg_bytes: bytes,
            seg_records: count,
            last_seq: last.map(|record| record.seq).unwrap_or(0),
            last_hash: last.map(|record| record.hash.clone()).unwrap_or_else(zeros),
            poisoned: false,
            _lock: None,
            mem_records: None,
        },
        state,
        open_path,
        records,
    ))
}
