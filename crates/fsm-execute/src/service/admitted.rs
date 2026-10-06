//! Native observation and original completion application without admission.

use crate::{
    error::ExecError,
    run::{Pipeline, Runner},
    sched::Scheduler,
    watch::Watcher,
};
use fsm_store::{clock::Clock, store::Store};

/// Observe retained original native work and apply at most one completion action.
/// This never schedules new effects, preparations, bound entry, retries or machine
/// deadlines; original completion events/handoffs remain eligible for delivery.
/// It requires the healthy original writer and is not an independent stop/report
/// driver; hosts must keep transport observation progressing without that writer.
pub fn observe_admitted_with(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    store: &mut Store,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> Result<Vec<String>, ExecError> {
    #[cfg(target_os = "linux")]
    return observe_linux(watcher, scheduler, runner, pipeline, store, clock, now_ms);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (watcher, scheduler, runner, pipeline, store, clock, now_ms);
        Err(ExecError::new(
            "exec/mode",
            "admitted lifecycle requires a supported native runner",
        ))
    }
}

#[cfg(target_os = "linux")]
fn observe_linux(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    store: &mut Store,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> Result<Vec<String>, ExecError> {
    use std::os::unix::fs::MetadataExt;
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        || !runner.uses_native_admission()
    {
        return Err(ExecError::new(
            "exec/mode",
            "admitted lifecycle requires a supported native runner",
        ));
    }
    runner.finished_effects();
    if store.journal.is_memory() || store.journal.is_read_only() || store.journal.poisoned {
        return Err(ExecError::new(
            "exec/mode",
            "admitted lifecycle requires the original healthy native writer",
        ));
    }
    let (mut observation, snapshot) = watcher.scan_snapshot(now_ms)?;
    let writer = std::fs::metadata(&store.data_dir).map_err(|_| deferred())?;
    let observed = std::fs::metadata(&snapshot.data_dir).map_err(|_| deferred())?;
    if snapshot.journal.poisoned || writer.dev() != observed.dev() || writer.ino() != observed.ino()
    {
        return Err(deferred());
    }
    runner.recover_native_owners(&snapshot, &mut observation, scheduler)?;
    if !runner.accepts_native_writer(store) {
        return Err(deferred());
    }
    let mut lines: Vec<String> = observation
        .unresolved
        .iter()
        .map(super::error_line)
        .collect();
    drop(snapshot);
    runner.finished_effects();
    runner.release_native_preparations(scheduler);
    if let Some(result) = runner.apply_native_completed(store, clock, pipeline, scheduler) {
        lines.push(result.unwrap_or_else(|error| super::error_line(&error)));
    }
    Ok(lines)
}

#[cfg(target_os = "linux")]
fn deferred() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "admitted lifecycle requires the original physical store",
    )
}
