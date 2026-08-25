//! The driver: one tick, and the loop around it.
//!
//! Two entry points, because two callers own the writer differently.
//! [`tick_with`] works against a writer handle it is *lent*, which is how an
//! embedded MCP server drives the same loop on the handle it already holds;
//! [`tick`] opens the writer itself and drops it before returning, so the
//! executor never holds the single-writer lock across a sleep.

use std::path::Path;

use fsm_store::clock::Clock;
use fsm_store::store::Store;

use crate::config::HandlerTable;
use crate::error::ExecError;
use crate::run::{Pipeline, Runner};
use crate::sched::Scheduler;
use crate::watch::Watcher;

/// Run one tick against a writer the caller owns, returning its action lines.
pub fn tick_with(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    store: &mut Store,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> Vec<String> {
    let _ = (watcher, scheduler, runner, pipeline, store, clock, now_ms);
    unimplemented!("task 3802")
}

/// Run one tick, opening the writer only if the tick has something to write.
pub fn tick(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    data_dir: &Path,
    clock: &mut dyn Clock,
    now_ms: i64,
) -> Vec<String> {
    let _ = (
        watcher, scheduler, runner, pipeline, data_dir, clock, now_ms,
    );
    unimplemented!("task 3802")
}

/// Tick, emit, sleep, repeat — the whole executor loop, with no async runtime.
pub fn run(
    data_dir: &Path,
    table: HandlerTable,
    poll_interval_ms: u64,
    clock: &mut dyn Clock,
) -> Result<(), ExecError> {
    let _ = (data_dir, table, poll_interval_ms, clock);
    unimplemented!("task 3901")
}
