//! Sealing a journal prefix into a detached archive.
//!
//! # The ordering is the durability contract
//!
//! **Copy, then seal, then remove.** Before the seal record is appended,
//! nothing in the chain references any of the new files, so an interrupted run
//! leaves inert bytes a re-run overwrites. After it, the removed segments are
//! already in the archive and their records are below the seal, so the loader
//! skips them by sequence and a re-run finishes the removal. An implementation
//! that moved segments before appending the seal would have a window where the
//! records are gone and nothing says they were sealed, and a store interrupted
//! in that window never opens again.
//!
//! Every prefix of [`seal_and_archive`]'s numbered steps leaves a store that
//! opens. The steps stay in one function, read top to bottom, because
//! `CONTRIBUTING.md` requires that of a crash-safe sequence and because the
//! order *is* the argument.
//!
//! # The operation creates its cut; it does not search for one
//!
//! A valid cut satisfies two conditions at once. Its record must be a
//! `state_checkpoint`, so the base derives from proven state rather than from
//! whatever the writer asserted. And it must be the **last record of a
//! segment**, because `should_rotate` fires on size and a segment the cut fell
//! inside could only be archived by splitting it — which means rewriting
//! published bytes, which this project never does. Nothing in the store
//! produces a sequence meeting both by chance, so the operation makes one out
//! of two primitives that already exist: append a `state_checkpoint`, then
//! `Journal::force_rotate`.
//!
//! # A seal moves whole segments, and the cut is a segment boundary
//!
//! Plan 0017 wanted the cut to be a `state_checkpoint` as well, so that the
//! base derives from state a fold already proved rather than state the writer
//! asserted. Where the operation can cut at the head it still does exactly
//! that, and the checkpoint costs nothing. But it cannot always cut at the
//! head, and there the requirement has to give:
//!
//! Only a **pending effect** pins the cut, and a live store almost always has
//! one — the executor settles each within a tick, so at any instant a few are
//! in flight and their emitting records are near the head. A rule that
//! required a checkpoint cut would then refuse every seal a running store ever
//! asked for, which is the same mistake the first shape of the carry rule made
//! and was discarded for. So the cut is a **segment boundary**: the highest
//! one strictly below the pin. The operation creates a fresh boundary at the
//! head when the pin allows it, and otherwise seals as many whole segments as
//! the pin does allow.
//!
//! What is lost by cutting below the head is one invariant, and it is worth
//! writing down. A seal record is appended wherever the head is, so when the
//! cut is not the head the seal is **not** adjacent to the prefix it seals:
//! `sealed_last_hash` is the hash of the record at the cut rather than the
//! seal's own `prev`, and the first live record after a seal is an ordinary
//! record rather than the seal. Neither weakens anything. The chain still runs
//! unbroken from the cut through the seal, the loader still starts from the
//! pair the base supplies and still refuses a first record whose `prev`
//! disagrees, and the seal still commits both roots. `record/body_shape.rs`
//! asserts the join only in the adjacent case, which is where it is a fact
//! rather than a coincidence.
//!
//! `--before-seq N` is an assertion rather than a choice: it names the
//! sequence the seal will seal **through**, exactly as `--dry-run` reported
//! it, and the operation refuses if the answer has moved since. That is
//! `expect_seq`'s pattern, and it is what stops a preview and a run from
//! disagreeing about which prefix moved.

use std::collections::BTreeMap;
use std::path::Path;

use fsm_core::json::Value;
use fsm_core::record::{Record, RecordKind, genesis_uses_historical_definition_limits};
use fsm_core::replay::{NopSink, STATE_ROOT_FORMAT, StoreState, fold_with};

use crate::archive::{ArchivedSegment, Manifest};
use crate::base::{self, DefinitionLimits};
use crate::seal_pin;
use crate::seal_safety::{self, CarryDecision};

use super::{ErrorObj, Store};

/// The base state file a sealed store opens from.
pub const BASE_FILE: &str = "BASE";

/// Where a seal's prefix ends.
///
/// Both variants are segment-final. The first is a boundary this operation
/// creates — a `state_checkpoint` followed by a rotation, which is the better
/// cut because the base then derives from state a fold already proved. The
/// second is a boundary that already exists, taken when the pin forbids the
/// head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cut {
    CreateAtHead(u64),
    Existing(u64),
}

impl Cut {
    fn seq(self) -> u64 {
        match self {
            Self::CreateAtHead(seq) | Self::Existing(seq) => seq,
        }
    }
}

/// What a seal did, or what a preview says one would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealReport {
    pub sealed_through_seq: u64,
    pub sealed_last_hash: String,
    pub archive_id: String,
    pub records_sealed: u64,
    pub segments: Vec<String>,
    pub keys_carried: usize,
    pub keys_dropped: usize,
    /// Absent on a dry run, which appends nothing.
    pub seal_record_seq: Option<u64>,
}

fn refused(message: String, hint: &str) -> ErrorObj {
    ErrorObj::new("store/archive_refused", message).hint(hint.to_string())
}

fn write_error(what: &str, path: &Path, error: impl std::fmt::Display) -> ErrorObj {
    ErrorObj::new("io/write", format!("{what} {}: {error}", path.display()))
}

/// Whether the store's machines were admitted under the historical ceiling.
///
/// Read from the genesis record, which is below every possible cut — which is
/// exactly why the base file has to carry the answer forward.
fn definition_limits(records: &[Record]) -> DefinitionLimits {
    let historical = records.first().is_some_and(|record| {
        record.seq == 0
            && record.kind == RecordKind::Genesis
            && genesis_uses_historical_definition_limits(&record.body)
    });
    if historical {
        DefinitionLimits::Historical
    } else {
        DefinitionLimits::Current
    }
}

/// The journal's segments in sequence order, as `(name, first_seq)`.
fn segment_names(journal_dir: &Path) -> Result<Vec<(String, u64)>, ErrorObj> {
    let entries =
        std::fs::read_dir(journal_dir).map_err(|error| write_error("list", journal_dir, error))?;
    let mut segments: Vec<(String, u64)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(first) = name
            .strip_prefix("seg-")
            .and_then(|rest| rest.strip_suffix(".jsonl"))
            .and_then(|digits| digits.parse::<u64>().ok())
        else {
            continue;
        };
        segments.push((name, first));
    }
    segments.sort_by_key(|(_, first)| *first);
    Ok(segments)
}

/// The segments a cut at `cut` seals, each with the sequence range it holds.
///
/// The cut is the last record of its segment by construction, so the split is
/// clean: every segment whose first sequence is at or below the cut is sealed
/// whole, and the one the operation just rotated into is not.
fn sealed_segments(journal_dir: &Path, cut: u64) -> Result<Vec<(String, u64, u64)>, ErrorObj> {
    let all = segment_names(journal_dir)?;
    let mut out = Vec::new();
    for (index, (name, first)) in all.iter().enumerate() {
        if *first > cut {
            break;
        }
        let last = all
            .get(index + 1)
            .map(|(_, next_first)| next_first.saturating_sub(1))
            .unwrap_or(cut)
            .min(cut);
        out.push((name.clone(), *first, last));
    }
    Ok(out)
}

impl Store {
    /// Seal everything up to a checkpoint this operation creates, move those
    /// bytes into `archive_dir`, and leave a `journal_sealed` record saying so.
    ///
    /// `expect_cut` asserts which sequence the seal will seal through; see the
    /// module doc. `Ok` is returned only after the commit point.
    pub fn seal_and_archive(
        &mut self,
        archive_dir: &Path,
        expect_cut: Option<u64>,
    ) -> Result<SealReport, ErrorObj> {
        self.seal_and_archive_on(&mut crate::clock::GlobalClock, archive_dir, expect_cut)
    }

    /// What a seal would do, without taking the lock or writing anything.
    ///
    /// Opens nothing and appends nothing — in particular it appends no
    /// checkpoint and performs no rotation, so a preview from a monitoring
    /// session leaves the data directory byte-identical.
    pub fn preview_seal(&self, expect_cut: Option<u64>) -> Result<SealReport, ErrorObj> {
        let cut = self.prospective_cut(expect_cut)?.seq();
        // The base state is built only to run the carry rule over it; a
        // preview reports the partition, not the file.
        let (_base_state, carried) = self.prospective_base(cut)?;
        Ok(SealReport {
            sealed_through_seq: cut,
            // The checkpoint does not exist yet, so neither does its hash. A
            // preview reports the prefix and the partition, not a value that
            // would be a guess.
            sealed_last_hash: String::new(),
            archive_id: String::new(),
            records_sealed: cut + 1,
            segments: sealed_segments(&crate::journal_io::journal_dir(&self.data_dir), cut)?
                .into_iter()
                .map(|(name, _, _)| name)
                .collect(),
            keys_carried: carried.carried_count(),
            keys_dropped: carried.dropped_count(),
            seal_record_seq: None,
        })
    }

    /// The cut a seal would take, after the pin and the caller's assertion.
    fn prospective_cut(&self, expect_cut: Option<u64>) -> Result<Cut, ErrorObj> {
        let cut = self.choose_cut()?;
        if let Some(expected) = expect_cut
            && expected != cut.seq()
        {
            return Err(refused(
                format!(
                    "`--before-seq {expected}` names a sequence this seal would not seal through: \
                     this run would seal through {}",
                    cut.seq()
                ),
                "the flag asserts which prefix moves, exactly as `--dry-run` reported it. Re-run \
                 the preview and pass the sequence it names, or omit the flag",
            ));
        }
        Ok(cut)
    }

    /// The highest admissible cut: a fresh boundary at the head when the pin
    /// allows one, otherwise the highest existing segment boundary below it.
    fn choose_cut(&self) -> Result<Cut, ErrorObj> {
        let head_cut = self.journal.last_seq.saturating_add(1);
        let Err(pinned) = seal_pin::admissible(head_cut, &self.state, &self.records) else {
            return Ok(Cut::CreateAtHead(head_cut));
        };
        let pin = seal_pin::pin(&self.state, &self.records)
            .expect("a refused cut has a pin")
            .seq;
        let boundaries = self.segment_boundaries()?;
        let highest = boundaries.into_iter().rfind(|seq| *seq < pin);
        match highest {
            // Sealing less is the useful answer; refusing because a workflow is
            // mid-flight is not.
            Some(seq) => Ok(Cut::Existing(seq)),
            None => Err(pinned),
        }
    }

    /// The last sequence of every complete segment but the active one.
    ///
    /// A cut has to be segment-final: `should_rotate` fires on size, so a
    /// segment the cut fell inside could only be archived by splitting it,
    /// which means rewriting published bytes.
    fn segment_boundaries(&self) -> Result<Vec<u64>, ErrorObj> {
        let segments = segment_names(&crate::journal_io::journal_dir(&self.data_dir))?;
        Ok(segments
            .iter()
            .skip(1)
            .map(|(_, first)| first.saturating_sub(1))
            .collect())
    }

    /// The state the base file would hold, and the ledger partition it carries.
    fn prospective_base(&self, cut: u64) -> Result<(StoreState, CarryDecision), ErrorObj> {
        // Fold exactly the prefix the archive will hold. For a head cut that is
        // every record; for a boundary cut it is fewer, and reusing
        // `self.state` would describe a state the archive does not contain.
        let prefix: Vec<Record> = self
            .records
            .iter()
            .filter(|record| record.seq <= cut)
            .cloned()
            .collect();
        let mut base_state = fold_with(prefix, &mut NopSink)
            .map_err(|error| ErrorObj::new("store/chain_broken", format!("{error:?}")))?;
        base_state.last_seq = cut;
        let carried = seal_safety::carry_at_cut(
            &base_state,
            &self.records,
            definition_limits(&self.records),
        )?;
        base_state.dedup = carried.carried.clone();
        Ok((base_state, carried))
    }

    pub fn seal_and_archive_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        archive_dir: &Path,
        expect_cut: Option<u64>,
    ) -> Result<SealReport, ErrorObj> {
        // 1. Take the writer lock — held since `Store::open` — and refuse a
        //    read-only store. Refuse the archive directory before anything
        //    else: an operator who mistypes a path should not discover a new
        //    directory holding their history.
        self.ensure_writable()?;
        if !crate::persistence_directory_exists(archive_dir)
            .map_err(|error| write_error("inspect", archive_dir, error))?
        {
            return Err(refused(
                format!("{} does not exist", archive_dir.display()),
                "create the archive directory first: `fsm` never creates one, so a mistyped `--to` \
                 cannot quietly become the place your history went",
            ));
        }
        crate::archive::refuse_existing_manifest(archive_dir)?;

        // 2. Establish the cut. It is the sequence the checkpoint below will
        //    take, checked against the caller's assertion and against the pin
        //    *before* anything is appended, so a refused seal writes nothing.
        let cut = self.prospective_cut(expect_cut)?;
        let (mut base_state, carried) = self.prospective_base(cut.seq())?;
        let sealed_last_hash = match cut {
            Cut::CreateAtHead(seq) => {
                let checkpoint = self.append_cut_checkpoint(clock, seq)?;
                self.journal
                    .force_rotate()
                    .map_err(|error| Self::journal_write_error(error, None))?;
                checkpoint.hash
            }
            Cut::Existing(seq) => self
                .records
                .iter()
                .find(|record| record.seq == seq)
                .map(|record| record.hash.clone())
                .ok_or_else(|| {
                    ErrorObj::new(
                        "store/chain_broken",
                        format!("the journal holds no record at the cut, seq {seq}"),
                    )
                })?,
        };
        let cut = cut.seq();
        base_state.last_hash = sealed_last_hash.clone();

        // 3. The carried ledger is already decided; the base is now complete.
        let journal_dir = crate::journal_io::journal_dir(&self.data_dir);
        let segments = sealed_segments(&journal_dir, cut)?;
        let roots = base::base_roots(&base_state);

        // 4. Write MANIFEST, then fsync it and its directory.
        let mut described = Vec::new();
        for (name, first_seq, last_seq) in &segments {
            let (digest, bytes) = crate::archive::file_digest(&journal_dir.join(name))?;
            described.push(ArchivedSegment {
                name: name.clone(),
                first_seq: *first_seq,
                last_seq: *last_seq,
                sha256: digest,
                bytes,
            });
        }
        let first_seq = described
            .first()
            .map(|segment| segment.first_seq)
            .unwrap_or(0);
        let first_prev_hash = self
            .records
            .iter()
            .find(|record| record.seq == first_seq)
            .map(|record| record.prev.clone())
            .unwrap_or_else(fsm_core::record::zeros);
        let manifest = Manifest {
            sealed_through_seq: cut,
            sealed_last_hash: sealed_last_hash.clone(),
            first_seq,
            first_prev_hash,
            records: cut.saturating_sub(first_seq) + 1,
            segments: described,
        };
        let mut manifest_bytes = fsm_core::canon::canon_bytes(&manifest.to_value());
        manifest_bytes.push(b'\n');
        let manifest_path = crate::archive::manifest_path(archive_dir);
        crate::write_durable(&manifest_path, &manifest_bytes)
            .map_err(|error| write_error("write", &manifest_path, error))?;
        // Windows has no portable directory fsync. The store's existing
        // position is inherited rather than mitigated: classify and repair on
        // the next open. This operation adds no new platform assumption.
        crate::sync_dir(archive_dir).map_err(|error| write_error("sync", archive_dir, error))?;

        // 5. **Copy** each sealed segment into the archive, fsync it, then read
        //    the copy back and check its digest against the manifest. Copying
        //    rather than moving is the whole safety argument: until step 7 the
        //    live journal is untouched and every new file is inert.
        for segment in &manifest.segments {
            let source = journal_dir.join(&segment.name);
            let destination = archive_dir.join(&segment.name);
            let bytes =
                std::fs::read(&source).map_err(|error| write_error("read", &source, error))?;
            crate::write_durable(&destination, &bytes)
                .map_err(|error| write_error("write", &destination, error))?;
            let (digest, size) = crate::archive::file_digest(&destination)?;
            if digest != segment.sha256 || size != segment.bytes {
                return Err(write_error(
                    "verify the archived copy of",
                    &destination,
                    "the copy does not match the manifest",
                ));
            }
        }
        crate::sync_dir(archive_dir).map_err(|error| write_error("sync", archive_dir, error))?;

        // 6. Write BASE durably. `write_durable` writes a temporary file,
        //    fsyncs it, and renames; the directory fsync follows.
        let base_path = journal_dir.join(BASE_FILE);
        let mut base_bytes = fsm_core::canon::canon_bytes(&base::encode(
            &base_state,
            definition_limits(&self.records),
        ));
        base_bytes.push(b'\n');
        crate::write_durable(&base_path, &base_bytes)
            .map_err(|error| write_error("write", &base_path, error))?;
        crate::sync_dir(&journal_dir).map_err(|error| write_error("sync", &journal_dir, error))?;

        // 7. **Append the seal record. This is the commit point.** Before this
        //    line the store is unsealed and every file written above is inert;
        //    after it the store is sealed.
        let seal_record =
            self.append_seal_record(clock, cut, &sealed_last_hash, &roots, &manifest)?;

        // 8. Remove the now-copied segments from the live journal.
        for segment in &manifest.segments {
            let path = journal_dir.join(&segment.name);
            std::fs::remove_file(&path).map_err(|error| write_error("remove", &path, error))?;
        }
        crate::sync_dir(&journal_dir).map_err(|error| write_error("sync", &journal_dir, error))?;

        // 9. Drop every snapshot cache at or below the seal: it can no longer
        //    be validated against records that are present.
        drop_snapshots_through(&self.data_dir, cut);

        Ok(SealReport {
            sealed_through_seq: cut,
            sealed_last_hash,
            archive_id: manifest.archive_id(),
            records_sealed: manifest.records,
            segments: manifest
                .segments
                .iter()
                .map(|segment| segment.name.clone())
                .collect(),
            keys_carried: carried.carried_count(),
            keys_dropped: carried.dropped_count(),
            seal_record_seq: Some(seal_record.seq),
        })
    }

    /// Append the `state_checkpoint` that becomes the cut.
    fn append_cut_checkpoint(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        cut: u64,
    ) -> Result<Record, ErrorObj> {
        let record = self
            .journal
            .append_at(
                RecordKind::StateCheckpoint,
                Value::Obj(BTreeMap::from([
                    (
                        "state_root".into(),
                        Value::Str(crate::snapshot::materialize_state_root_at(&self.state, cut)),
                    ),
                    (
                        "state_root_format".into(),
                        Value::Str(STATE_ROOT_FORMAT.into()),
                    ),
                ])),
                clock.now_ms(),
            )
            .map_err(|error| Self::journal_write_error(error, None))?;
        self.note_record(&record);
        Ok(record)
    }

    fn append_seal_record(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        cut: u64,
        sealed_last_hash: &str,
        roots: &base::BaseRoots,
        manifest: &Manifest,
    ) -> Result<Record, ErrorObj> {
        let body = Value::Obj(BTreeMap::from([
            ("sealed_through_seq".into(), Value::Num(cut.to_string())),
            (
                "sealed_last_hash".into(),
                Value::Str(format!("sha256:{sealed_last_hash}")),
            ),
            (
                "base_state_root".into(),
                Value::Str(roots.state_root.clone()),
            ),
            (
                "state_root_format".into(),
                Value::Str(STATE_ROOT_FORMAT.into()),
            ),
            (
                "base_dedup_fp_root".into(),
                Value::Str(roots.dedup_fp_root.clone()),
            ),
            (
                "base_dedup_format".into(),
                Value::Str(fsm_core::hashes::BASE_DEDUP_FORMAT.into()),
            ),
            ("archive_id".into(), Value::Str(manifest.archive_id())),
            (
                "records_sealed".into(),
                Value::Num(manifest.records.to_string()),
            ),
        ]));
        let record = self
            .journal
            .append_at(RecordKind::JournalSealed, body, clock.now_ms())
            .map_err(|error| Self::journal_write_error(error, None))?;
        self.note_record(&record);
        Ok(record)
    }
}

/// Remove every snapshot cache at or below `cut`.
///
/// Best effort by design: a snapshot is a disposable cache, so failing to
/// remove one is not a reason to fail a committed seal. A stale cache is
/// skipped on open by the seq floor `8101` adds.
fn drop_snapshots_through(data_dir: &Path, cut: u64) {
    for (seq, path) in crate::snapshot::listed_snaps(data_dir) {
        if seq <= cut {
            let _ = std::fs::remove_file(&path);
        }
    }
}
