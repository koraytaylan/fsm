//! Closure-only original-run recovery; original result publications refuse.

use crate::{error::ExecError, run::native_client::NativeShutdown};
use fsm_core::json::Value;
use fsm_store::{clock::Clock, store::Store};
use std::time::{Duration, Instant};

/// Reconcile one current original orphan under a healthy durable writer.
///
/// This bounded closure-only path refuses active or missing runner leases and
/// original result publications; it loads no handlers and starts no new work.
/// Complete or partial results require the original authenticated recovery path.
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
    let mut closure =
        NativeShutdown::start_reconciliation(store, &claim, timeout).map_err(&deferred)?;
    loop {
        let proven = closure.poll().map_err(&deferred)?.is_some();
        if proven && closure.reap().map_err(&deferred)? {
            return closure.settle_interrupted(store, clock);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                deferred("run reconciliation deadline; original ownership remains uncertain".into())
            })?;
        std::thread::sleep(remaining.min(Duration::from_millis(5)));
    }
}
