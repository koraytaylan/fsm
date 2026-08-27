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
