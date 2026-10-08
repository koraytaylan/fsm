//! Original-run recovery using authenticated completion or guarded closure.

use crate::{
    error::ExecError,
    run::native_client::{NativeExecution, NativeShutdown, completion_published},
};
use fsm_core::json::Value;
use fsm_core::record::{
    RecordKind,
    execution::{Claim, Settlement},
};
use fsm_store::{clock::Clock, store::Store};
use std::time::{Duration, Instant};

/// Reconcile one original orphan or replay its settlement under a healthy writer.
///
/// Published completions use startup's authenticated original-result recovery;
/// otherwise guarded closure refuses active or missing runner leases and partial
/// publications; neither path loads current handlers or starts new work.
pub fn reconcile_run(
    store: &mut Store,
    clock: &mut dyn Clock,
    run_id: u64,
    timeout: Duration,
) -> Result<Value, ExecError> {
    let deferred = |reason: String| {
        ExecError::new("exec/inflight_deferred", reason).hint(
            "retain the original run; recover original results or restore its authority facilities before retrying reconciliation",
        )
    };
    if store.journal.is_memory() || store.journal.is_read_only() || store.journal.poisoned {
        return Err(ExecError::new(
            "exec/mode",
            "run reconciliation requires a healthy durable writer",
        ));
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .filter(|_| !timeout.is_zero())
        .ok_or_else(|| deferred("run reconciliation deadline invalid".into()))?;
    let claim = store
        .state
        .execution
        .unresolved()
        .find(|(claim, _)| claim.run_id() == run_id)
        .map(|(claim, _)| claim.clone());
    let Some(claim) = claim else {
        return replay_original_settlement(store, run_id)?.ok_or_else(|| {
            deferred(
                "original run has neither current ownership nor exact settlement replay".into(),
            )
        });
    };
    if completion_published(&store.data_dir, &claim).map_err(deferred)? {
        let mut original = NativeExecution::recover(store, &claim, timeout)?;
        loop {
            if original.observe()? && original.reap().map_err(deferred)? {
                return original.settle(store, clock);
            }
            wait_for_original(deadline).map_err(deferred)?;
        }
    }
    let mut closure =
        NativeShutdown::start_reconciliation(store, &claim, timeout).map_err(deferred)?;
    loop {
        let proven = closure.poll().map_err(deferred)?.is_some();
        if proven && closure.reap().map_err(deferred)? {
            return closure.settle_interrupted(store, clock);
        }
        wait_for_original(deadline).map_err(deferred)?;
    }
}

fn replay_original_settlement(store: &mut Store, run_id: u64) -> Result<Option<Value>, ExecError> {
    let unavailable = || {
        ExecError::new("exec/inflight_deferred", "original settlement replay material is unavailable")
        .hint("retain the original journal history and request ledger; absence never authorizes native closure")
    };
    let identity = Value::Num(run_id.to_string());
    let Some(settled) = store.records.iter().rev().find(|record| {
        record.kind == RecordKind::ExecutionSettled && record.body.get("run_id") == Some(&identity)
    }) else {
        return Ok(None);
    };
    let request_id = settled
        .body
        .get("request_id")
        .and_then(Value::as_str)
        .ok_or_else(unavailable)?
        .to_owned();
    let disposition = match settled.body.get("disposition").and_then(Value::as_str) {
        Some("acked") => Settlement::Acked,
        Some("attempted") => Settlement::Attempted,
        Some("interrupted") => Settlement::Interrupted,
        _ => return Err(unavailable()),
    };
    let original = store
        .records
        .iter()
        .rev()
        .find(|record| {
            record.kind == RecordKind::ExecutionClaimed
                && record.seq < settled.seq
                && record.body.get("run_id") == Some(&identity)
        })
        .ok_or_else(unavailable)?;
    let fields = [
        "run_id",
        "instance_id",
        "effect_id",
        "attempt",
        "handler_fingerprint",
        "retry",
        "domain",
    ]
    .into_iter()
    .map(|field| {
        original
            .body
            .get(field)
            .cloned()
            .map(|value| (field.into(), value))
            .ok_or_else(unavailable)
    })
    .collect::<Result<std::collections::BTreeMap<String, Value>, _>>()?;
    let claim = Claim::from_value(&Value::Obj(fields)).map_err(|_| unavailable())?;
    if ["instance_id", "effect_id"]
        .into_iter()
        .any(|field| original.body.get(field) != settled.body.get(field))
    {
        return Err(unavailable());
    }
    store
        .replay_execution_settlement(&claim, disposition, &request_id)
        .map_err(|error| ExecError::store(&error))?
        .map(Some)
        .ok_or_else(unavailable)
}

fn wait_for_original(deadline: Instant) -> Result<(), String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("run reconciliation deadline; original ownership remains uncertain")?;
    std::thread::sleep(remaining.min(Duration::from_millis(5)));
    Ok(())
}
