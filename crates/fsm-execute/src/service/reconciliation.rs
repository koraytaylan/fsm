//! Original-run recovery using authenticated completion or guarded closure.

use crate::{
    error::ExecError,
    run::native_client::{NativeExecution, NativeShutdown, completion_published},
};
use fsm_core::json::Value;
use fsm_store::{clock::Clock, store::Store};
use std::time::{Duration, Instant};

/// Reconcile one current original orphan under a healthy durable writer.
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
    let claim = store
        .state
        .execution
        .unresolved()
        .find(|(claim, _)| claim.run_id() == run_id)
        .map(|(claim, _)| claim.clone())
        .ok_or_else(|| deferred("original run is not currently retained by this writer".into()))?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .filter(|_| !timeout.is_zero())
        .ok_or_else(|| deferred("run reconciliation deadline invalid".into()))?;
    if completion_published(&store.data_dir, &claim).map_err(&deferred)? {
        let mut original = NativeExecution::recover(store, &claim, timeout)?;
        loop {
            if original.observe()? && original.reap().map_err(&deferred)? {
                return original.settle(store, clock);
            }
            wait_for_original(deadline).map_err(&deferred)?;
        }
    }
    let mut closure =
        NativeShutdown::start_reconciliation(store, &claim, timeout).map_err(&deferred)?;
    loop {
        let proven = closure.poll().map_err(&deferred)?.is_some();
        if proven && closure.reap().map_err(&deferred)? {
            return closure.settle_interrupted(store, clock);
        }
        wait_for_original(deadline).map_err(&deferred)?;
    }
}

fn wait_for_original(deadline: Instant) -> Result<(), String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or("run reconciliation deadline; original ownership remains uncertain")?;
    std::thread::sleep(remaining.min(Duration::from_millis(5)));
    Ok(())
}
