//! The diagnostic tools: what the journal can prove about itself.
//!
//! Each one wraps a function the store already has and the CLI already
//! calls, returning its value unchanged — a diagnosis that differs between
//! two surfaces is a diagnosis nobody can trust.
//!
//! Plan 0014, workstream 0066.

use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use super::super::dispatch::str_arg;

/// Why one journaled step did what it did.
///
/// `explain_seq` reconstructs every candidate transition, each guard's
/// verdict, the block pipeline with its before and after values, and the
/// invariant results — and its value is returned verbatim, because
/// projecting selected fields here is how this tool and `fsm explain --json`
/// would start disagreeing.
pub(in crate::mcp::tools) fn run_explain_step(
    store: &mut Store,
    _clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let instance_id = str_arg(args, "instance_id").unwrap_or("");
    let seq = args
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|n| n.parse::<u64>().ok())
        .ok_or_else(|| {
            ErrorObj::new("req/args_invalid", "seq must be a journal sequence number")
                .hint("read one from instance_history")
        })?;
    store.explain_seq(instance_id, seq)
}

/// The health names `docs/SPEC.md` §Recovery uses, and nothing else.
///
/// A new word for an existing condition would make two vocabularies for one
/// posture, and an operator reading a remedy needs the one the table names.
fn health_name(health: &fsm_store::journal_io::JournalHealth) -> &'static str {
    use fsm_store::journal_io::JournalHealth as H;
    match health {
        H::Ok => "Ok",
        H::TornTail { .. } => "TornTail",
        H::ChainBroken { .. } => "ChainBroken",
        H::StateHashMismatch { .. } | H::ReplayMismatch { .. } => "StateHashMismatch",
        H::NonCanonical { .. } => "NonCanonical",
        H::LockIo(_) => "LockIo",
        // A store this build cannot read and a journal with no genesis are
        // both "the store is not readable as it stands", which is `StoreIo`
        // in the table. Their messages say which.
        H::MissingGenesis | H::VersionMismatch { .. } | H::StoreIo(_) => "StoreIo",
        // Their own names, because their remedies are their own: neither is a
        // filesystem fault and neither is repairable in this directory.
        H::BaseMissing => "BaseMissing",
        H::BaseMismatch { .. } => "BaseMismatch",
    }
}

/// The remedy SPEC's recovery table prescribes, or `None` where the posture
/// is "no repair". Verbatim, and never run: a diagnostic that repaired
/// things would be a repair tool wearing a diagnostic's name.
fn remedy(health: &fsm_store::journal_io::JournalHealth) -> Option<&'static str> {
    use fsm_store::journal_io::JournalHealth as H;
    match health {
        H::TornTail { .. } => Some("fsm repair --truncate-torn-tail"),
        H::VersionMismatch { .. } => Some("upgrade fsm, or recreate the data directory"),
        H::MissingGenesis => Some("restore the journal from backup or recreate the data directory"),
        H::StoreIo(_) => Some("repair the filesystem or input fault"),
        H::BaseMissing => Some(
            "restore the journal from backup, or restore the BASE the seal that removed its              segments wrote",
        ),
        // `ChainBroken`, `StateHashMismatch` and `NonCanonical` are interior
        // damage: the table says refuse, no repair.
        _ => None,
    }
}

/// The seq a health names, when it names one.
fn first_bad_seq(health: &fsm_store::journal_io::JournalHealth) -> Option<u64> {
    use fsm_store::journal_io::JournalHealth as H;
    match health {
        H::ChainBroken { seq, .. }
        | H::StateHashMismatch { seq }
        | H::NonCanonical { seq, .. }
        | H::ReplayMismatch { seq, .. } => Some(*seq),
        _ => None,
    }
}

/// Check that the journal is what it says it is.
///
/// Read through `open_read_only`, so it takes no lock and is safe beside a
/// live writer: verification that stopped the writer would be a verification
/// nobody runs.
pub(in crate::mcp::tools) fn run_journal_verify(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    run_journal_verify_with(
        store,
        clock,
        args,
        &crate::mcp::progress::ProgressReporter::discarding(),
        &crate::mcp::cancel::CancelFlag::default(),
    )
}

/// The same check, reporting where it has got to and stopping when asked.
///
/// This is the first genuine consumer of both: verification is the one read
/// in this system whose cost grows with the journal, so it is the one place
/// a progress token and a cancellation both earn their keep.
pub(in crate::mcp::tools) fn run_journal_verify_with(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
    progress: &crate::mcp::progress::ProgressReporter,
    cancel: &crate::mcp::cancel::CancelFlag,
) -> Result<Value, ErrorObj> {
    let data_dir = store.data_dir.clone();
    verify_report(&data_dir, args, clock, progress, cancel)
}

/// Verify one data directory, whether or not anybody could open it.
///
/// Taking a path rather than a `Store` is the point: the store you most want
/// verified is the one that will not open, and a diagnostic that needs a
/// healthy store to report an unhealthy one is no diagnostic at all. Plan
/// 0014's degraded serve mode calls this with no store behind it.
pub fn verify_report(
    data_dir: &std::path::Path,
    args: &Value,
    clock: &mut dyn Clock,
    progress: &crate::mcp::progress::ProgressReporter,
    cancel: &crate::mcp::cancel::CancelFlag,
) -> Result<Value, ErrorObj> {
    let bound = |name: &str| -> Option<u64> {
        args.get(name)
            .and_then(Value::as_num)
            .and_then(|n| n.parse::<u64>().ok())
    };
    let from_seq = bound("from_seq").unwrap_or(0);
    let to_seq = bound("to_seq");
    if let Some(to) = to_seq
        && to < from_seq
    {
        return Err(
            ErrorObj::new("req/args_invalid", "to_seq is before from_seq")
                .hint("give a window that runs forwards, or omit both to check everything"),
        );
    }

    let mut last_seen = 0u64;
    let mut cancelled = false;
    // The walk starts at the journal's beginning whatever `from_seq` says,
    // because a chain is only checkable from its anchor. `from_seq` bounds
    // what is *counted*; `to_seq` ends the walk at the first batch boundary
    // past it — a batch is the granularity at which this can stop at all.
    let segments = fsm_store::journal_io::verify_segments_with(data_dir, &mut |walked, seq| {
        last_seen = seq;
        if cancel.cancelled() {
            cancelled = true;
            return fsm_store::journal_io::Walk::Stop;
        }
        progress.report(
            clock.now_ms(),
            walked,
            None,
            Some(&format!("verified through seq {seq}")),
            false,
        );
        match to_seq {
            Some(to) if seq >= to => fsm_store::journal_io::Walk::Stop,
            _ => fsm_store::journal_io::Walk::Continue,
        }
    });
    if cancelled {
        return Err(crate::mcp::cancel::CancelFlag::refusal());
    }

    // The conclusion is the store's own, unchanged: this tool decides
    // nothing about health, it only reports what verification found.
    let health = fsm_store::journal_io::classify(data_dir);
    let walked: u64 = segments.iter().map(|segment| segment.records).sum();
    let counted = match to_seq {
        None if from_seq == 0 => walked,
        // Seqs are contiguous, so the window's size is arithmetic rather
        // than a second count — and it stays honest when the walk stopped
        // early, because `last_seen` is where it stopped.
        _ => {
            let end = to_seq.unwrap_or(last_seen).min(last_seen);
            end.saturating_sub(from_seq).saturating_add(1).min(walked)
        }
    };
    progress.report(
        clock.now_ms(),
        counted,
        Some(counted),
        Some("verification complete"),
        true,
    );

    let mut out = std::collections::BTreeMap::from([
        (
            "health".to_string(),
            Value::Str(health_name(&health).to_string()),
        ),
        (
            "verified_records".to_string(),
            Value::Num(counted.to_string()),
        ),
        ("message".to_string(), Value::Str(health.message())),
        (
            "segments".to_string(),
            Value::Arr(
                segments
                    .iter()
                    .map(|segment| {
                        Value::Obj(std::collections::BTreeMap::from([
                            ("segment".to_string(), Value::Str(segment.segment.clone())),
                            (
                                "records".to_string(),
                                Value::Num(segment.records.to_string()),
                            ),
                            ("status".to_string(), Value::Str(segment.status.clone())),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]);
    if let Some(seq) = first_bad_seq(&health) {
        out.insert("first_bad_seq".to_string(), Value::Num(seq.to_string()));
        // SPEC's blast radius for interior damage, in SPEC's words.
        out.insert(
            "blast_radius".to_string(),
            Value::Str(format!("records ≥ {seq} unverifiable")),
        );
    }
    if let Some(remedy) = remedy(&health) {
        out.insert("remedy".to_string(), Value::Str(remedy.to_string()));
    }
    Ok(Value::Obj(out))
}

/// Re-execute the journal and see whether today's engine agrees with it.
pub(in crate::mcp::tools) fn run_journal_replay(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let data_dir = store.data_dir.clone();
    replay_report(
        &data_dir,
        args,
        clock,
        &crate::mcp::progress::ProgressReporter::discarding(),
        &crate::mcp::cancel::CancelFlag::default(),
    )
}

pub(in crate::mcp::tools) fn run_journal_replay_with(
    store: &mut Store,
    clock: &mut dyn Clock,
    args: &Value,
    progress: &crate::mcp::progress::ProgressReporter,
    cancel: &crate::mcp::cancel::CancelFlag,
) -> Result<Value, ErrorObj> {
    let data_dir = store.data_dir.clone();
    replay_report(&data_dir, args, clock, progress, cancel)
}

/// What a fold reports as it goes.
///
/// The sink the pure fold already calls per record is the seam: replay needs
/// no new one, and the cadence matches `journal_verify`'s so the two tools
/// feel alike from a client.
struct Watcher<'a> {
    clock: &'a mut dyn Clock,
    progress: &'a crate::mcp::progress::ProgressReporter,
    cancel: &'a crate::mcp::cancel::CancelFlag,
    seen: u64,
    total: u64,
    cancelled: bool,
}

impl fsm_core::replay::RecordSink for Watcher<'_> {
    fn on_record(
        &mut self,
        record: &fsm_core::record::Record,
        _state: &fsm_core::replay::StoreState,
    ) {
        self.seen += 1;
        if !self.seen.is_multiple_of(fsm_store::journal_io::BATCH) {
            return;
        }
        if self.cancel.cancelled() {
            self.cancelled = true;
            return;
        }
        self.progress.report(
            self.clock.now_ms(),
            self.seen,
            Some(self.total.max(self.seen)),
            Some(&format!("replayed through seq {}", record.seq)),
            false,
        );
    }
}

/// Replay one data directory, whether or not anybody could open it.
///
/// **Replay is not verification.** Verification checks the bytes and the
/// chain: that nothing was edited. Replay re-executes the engine and checks
/// that the outcomes the journal recorded are the outcomes the engine
/// produces today. A store can verify perfectly and still fail replay — that
/// is the engine's semantics having drifted, and it is the one failure this
/// catches and the other cannot.
pub fn replay_report(
    data_dir: &std::path::Path,
    args: &Value,
    clock: &mut dyn Clock,
    progress: &crate::mcp::progress::ProgressReporter,
    cancel: &crate::mcp::cancel::CancelFlag,
) -> Result<Value, ErrorObj> {
    let to_seq = args
        .get("to_seq")
        .and_then(Value::as_num)
        .and_then(|n| n.parse::<u64>().ok());
    // The intact prefix, not the whole journal: a torn final record is a
    // fact about the store, and "I cannot answer at all" is a worse answer
    // than "I replayed the twelve records that are whole".
    //
    // A journal that will not load even that far is reported rather than
    // refused, for the same reason: a diagnosis that declines to diagnose is
    // no use to the caller holding a broken store.
    let records = match fsm_store::journal_io::load_intact_prefix(data_dir) {
        Ok(records) => records,
        Err(error) => {
            return Ok(Value::Obj(std::collections::BTreeMap::from([
                ("replayed_records".to_string(), Value::Num("0".into())),
                ("matches".to_string(), Value::Bool(false)),
                (
                    "message".to_string(),
                    Value::Str(format!("the journal could not be read: {error}")),
                ),
            ])));
        }
    };
    let records: Vec<fsm_core::record::Record> = match to_seq {
        Some(to) => records.into_iter().filter(|r| r.seq <= to).collect(),
        None => records,
    };
    let total = records.len() as u64;
    let mut watcher = Watcher {
        clock,
        progress,
        cancel,
        seen: 0,
        total,
        cancelled: false,
    };
    let folded = fsm_core::replay::fold_with(records, &mut watcher);
    let seen = watcher.seen;
    if watcher.cancelled {
        return Err(crate::mcp::cancel::CancelFlag::refusal());
    }
    progress.report(
        clock.now_ms(),
        seen,
        Some(total),
        Some("replay complete"),
        true,
    );

    let mut out = std::collections::BTreeMap::from([(
        "replayed_records".to_string(),
        Value::Num(seen.to_string()),
    )]);
    match folded {
        Ok(state) => {
            out.insert("matches".to_string(), Value::Bool(true));
            // The root is what makes two stores comparable — two runs, two
            // machines, a store against its backup — and comparing is most
            // of what this tool is for.
            out.insert(
                "state_root".to_string(),
                Value::Str(fsm_core::replay::state_root_at(&state, state.last_seq)),
            );
            out.insert(
                "message".to_string(),
                Value::Str(format!(
                    "{seen} records replayed; every recorded outcome is the outcome the engine produces today"
                )),
            );
        }
        Err(fsm_core::replay::ReplayError::StateHashMismatch {
            seq,
            expected,
            found,
        }) => {
            out.insert("matches".to_string(), Value::Bool(false));
            // The *earliest* divergence: a difference propagates, so every
            // later one is a consequence and only the first is a clue.
            out.insert(
                "first_divergence_seq".to_string(),
                Value::Num(seq.to_string()),
            );
            out.insert(
                "message".to_string(),
                Value::Str(format!(
                    "seq {seq} recorded state hash {expected} and replays as {found}"
                )),
            );
        }
        Err(other) => {
            out.insert("matches".to_string(), Value::Bool(false));
            if let Some(seq) = replay_error_seq(&other) {
                out.insert(
                    "first_divergence_seq".to_string(),
                    Value::Num(seq.to_string()),
                );
            }
            out.insert("message".to_string(), Value::Str(format!("{other:?}")));
        }
    }
    Ok(Value::Obj(out))
}

/// The seq a replay failure names, when it names one.
fn replay_error_seq(error: &fsm_core::replay::ReplayError) -> Option<u64> {
    use fsm_core::replay::ReplayError as E;
    match error {
        E::StateHashMismatch { seq, .. } | E::FieldMismatch { seq, .. } => Some(*seq),
        _ => None,
    }
}

/// What is wrong with this store, and the exact command that fixes it.
pub(in crate::mcp::tools) fn run_store_doctor(
    store: &mut Store,
    _clock: &mut dyn Clock,
    _args: &Value,
) -> Result<Value, ErrorObj> {
    let data_dir = store.data_dir.clone();
    Ok(doctor_report(&data_dir))
}

/// Diagnose one data directory, open or not.
///
/// This is the tool that exists for the store that will not open, so it
/// takes a path and never needs one that does. `remedy` carries SPEC's
/// recovery command **verbatim** where the table prescribes one and is
/// absent where the posture is "no repair" — the tool never runs it, because
/// a person decides whether to destroy anything.
pub fn doctor_report(data_dir: &std::path::Path) -> Value {
    let diagnosis = fsm_store::journal_io::diagnose(data_dir);
    let mut out = std::collections::BTreeMap::from([
        (
            "health".to_string(),
            Value::Str(health_name(&diagnosis.health).to_string()),
        ),
        (
            "message".to_string(),
            Value::Str(diagnosis.health.message()),
        ),
        ("version".to_string(), Value::Str(diagnosis.version.clone())),
        ("readable".to_string(), Value::Bool(diagnosis.readable)),
        (
            "records".to_string(),
            Value::Num(diagnosis.records.to_string()),
        ),
        (
            "segments".to_string(),
            Value::Arr(
                diagnosis
                    .segments
                    .iter()
                    .map(|segment| {
                        Value::Obj(std::collections::BTreeMap::from([
                            ("segment".to_string(), Value::Str(segment.segment.clone())),
                            (
                                "records".to_string(),
                                Value::Num(segment.records.to_string()),
                            ),
                            ("status".to_string(), Value::Str(segment.status.clone())),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "snapshot".to_string(),
            Value::Obj(std::collections::BTreeMap::from([
                (
                    "present".to_string(),
                    Value::Bool(diagnosis.snapshot_seq.is_some()),
                ),
                (
                    "seq".to_string(),
                    match diagnosis.snapshot_seq {
                        Some(seq) => Value::Num(seq.to_string()),
                        None => Value::Null,
                    },
                ),
                (
                    "records_behind".to_string(),
                    Value::Num(diagnosis.snapshot_behind.to_string()),
                ),
                // Presence alone tells an operator nothing; how far behind it
                // is tells them whether every open is paying for the tail.
                (
                    "stale".to_string(),
                    Value::Bool(
                        diagnosis.snapshot_seq.is_some()
                            && diagnosis.snapshot_behind >= STALE_AFTER,
                    ),
                ),
            ])),
        ),
        (
            "writer_lock".to_string(),
            Value::Obj(std::collections::BTreeMap::from([
                ("held".to_string(), Value::Bool(diagnosis.writer_lock_held)),
                (
                    "holder".to_string(),
                    match diagnosis.writer_lock_holder {
                        Some(pid) => Value::Num(pid.to_string()),
                        None => Value::Null,
                    },
                ),
            ])),
        ),
    ]);
    if let Some(found) = &diagnosis.migration_required_from {
        out.insert(
            "migration_required_from".to_string(),
            Value::Str(found.clone()),
        );
    }
    // Only when the store could be read at all. An always-present empty list
    // would read as "checked, and there are none".
    if diagnosis.readable {
        out.insert("orphans".to_string(), Value::Arr(diagnosis.orphans.clone()));
    }
    if let Some(remedy) = remedy(&diagnosis.health) {
        out.insert("remedy".to_string(), Value::Str(remedy.to_string()));
    }
    Value::Obj(out)
}

/// How far behind the journal a snapshot may fall before it is worth saying
/// so. One rotation's worth of records: below that, re-reading the tail is
/// cheaper than anybody's attention.
const STALE_AFTER: u64 = 1_000;

/// Leave a note in the audit trail.
///
/// **Changes no logical state.** It claims a `request_id`, it writes one
/// `annotated` record, it appears in the history — and it moves nothing.
/// "Annotate" reads like it might do more, and a caller who believed it
/// advanced a workflow would find out at the worst possible moment.
pub(in crate::mcp::tools) fn run_instance_annotate(
    store: &mut Store,
    _clock: &mut dyn Clock,
    args: &Value,
) -> Result<Value, ErrorObj> {
    let instance_id = str_arg(args, "instance_id").unwrap_or("").to_string();
    let request_id = str_arg(args, "request_id").unwrap_or("").to_string();
    let note = str_arg(args, "note").unwrap_or("");
    // A replay writes nothing, which is how this tells one from a first
    // call: the store's own answer is the same either way.
    let before = store.journal.last_seq;
    let written = store.annotate(&instance_id, &request_id, note)?;
    let duplicate = store.journal.last_seq == before;

    let mut out = written.as_obj().cloned().unwrap_or_default();
    out.insert("duplicate".to_string(), Value::Bool(duplicate));
    out.insert("instance_id".to_string(), Value::Str(instance_id.clone()));
    // The seq the note landed at — the original one on a replay, which is
    // the record a caller would go and read.
    if let Some(slot) = store.state.dedup.get(&request_id) {
        out.insert("seq".to_string(), Value::Num(slot.seq.to_string()));
    }
    // And the view, unchanged, so a caller can confirm the note landed
    // *and* that nothing moved, without a second call.
    if let Ok(view) = store.instance_report(&instance_id)
        && let Some(fields) = view.as_obj()
    {
        for (name, value) in fields {
            out.entry(name.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(Value::Obj(out))
}
