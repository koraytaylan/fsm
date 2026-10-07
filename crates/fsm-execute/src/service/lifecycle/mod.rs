//! Explicitly driven native lifecycle with an independently waitable control.

mod paired;
pub use paired::PairedNativeExecutor;

mod closures;
mod control;
#[cfg(all(test, target_os = "linux"))]
mod tests;
use closures::Closures;
pub use control::{ExecutorControl, ExecutorPhase, ShutdownMode, ShutdownReport, ShutdownRequest};

use crate::{
    config::HandlerTable,
    error::ExecError,
    run::{Pipeline, Runner},
    sched::Scheduler,
    watch::Watcher,
};
use fsm_store::{clock::Clock, store::Store};
/// Owns the original writer and native execution components.
///
/// The host must drive `tick` or `poll`; control waits never drive journal or
/// native I/O, and a stalled host produces an uncertain deadline report.
/// This opt-in library driver does not install the native authority.
pub struct OwnedNativeExecutor {
    watcher: Watcher,
    scheduler: Scheduler,
    runner: Runner,
    pipeline: Pipeline,
    store: Option<Store>,
    control: ExecutorControl,
    closures: Closures,
}

impl OwnedNativeExecutor {
    /// Take the sole healthy durable writer without allocating a native domain.
    pub fn new(store: Store, table: HandlerTable) -> Result<Self, ExecError> {
        let watcher = Watcher::with_handlers(store.data_dir.clone(), &table);
        let scheduler = Scheduler::new(table);
        Self::from_owned_parts(store, watcher, scheduler, Runner::new_native()?)
    }

    /// Transfer an existing native runner, including its original local owners.
    pub fn from_owned_parts(
        store: Store,
        watcher: Watcher,
        scheduler: Scheduler,
        runner: Runner,
    ) -> Result<Self, ExecError> {
        let admission = runner.native_admission_control()?;
        if store.journal.is_memory() || store.journal.is_read_only() || store.journal.poisoned {
            return Err(ExecError::new(
                "exec/mode",
                "owned lifecycle requires a healthy durable writer",
            ));
        }
        Ok(Self {
            watcher,
            scheduler,
            runner,
            pipeline: Pipeline,
            store: Some(store),
            control: ExecutorControl::new(admission),
            closures: Closures::default(),
        })
    }

    /// Borrow the original handler table used by this driver's scheduler.
    pub fn handler_table(&self) -> &HandlerTable {
        self.scheduler.handler_table()
    }

    /// Clone control metadata without borrowing the execution worker.
    pub fn control(&self) -> ExecutorControl {
        self.control.clone()
    }

    /// Inspect retained local readiness without I/O, clocks or ownership changes.
    ///
    /// A ready preparation, bound entry or original outcome needs an owner
    /// decision; readiness alone proves neither completion nor native closure.
    pub fn has_ready_native_work(&self) -> bool {
        self.runner.native_ready()
    }

    /// Access the owned journal until confirmed shutdown releases it.
    pub fn store_mut(&mut self) -> Option<&mut Store> {
        self.store.as_mut()
    }

    /// Run an explicit ordinary tick, or admission-free shutdown after stop.
    pub fn tick(&mut self, clock: &mut dyn Clock, now_ms: i64) -> Vec<String> {
        if self.control.requested() {
            return self.poll(clock, now_ms);
        }
        let Some(store) = self.store.as_mut() else {
            return Vec::new();
        };
        super::tick_with(
            &mut self.watcher,
            &mut self.scheduler,
            &mut self.runner,
            &mut self.pipeline,
            store,
            clock,
            now_ms,
        )
    }

    /// Observe original completions and bounded closure work without admission.
    /// No pending effects, retries or machine deadlines are scheduled here.
    pub fn poll(&mut self, clock: &mut dyn Clock, now_ms: i64) -> Vec<String> {
        let Some(store) = self.store.as_mut() else {
            return Vec::new();
        };
        let observed = super::observe_admitted_with(
            &mut self.watcher,
            &mut self.scheduler,
            &mut self.runner,
            &mut self.pipeline,
            store,
            clock,
            now_ms,
        );
        let complete = observed.is_ok();
        let mut lines = observed.unwrap_or_else(|error| vec![super::error_line(&error)]);
        lines.extend(self.runner.take_native_cleanup_diagnostic());
        if self.control.requested() {
            // These are this runner's client helpers, never foreign domains.
            self.runner.cancel_foreign_native_helpers();
        }
        if self.control.closure_requested()
            && complete
            && let Some(store) = self.store.as_ref()
        {
            self.closures.start(&self.runner, store, &mut lines);
        }
        self.closures.poll(
            &mut self.runner,
            &mut self.scheduler,
            self.store.as_mut(),
            clock,
            &mut lines,
        );
        let (run_ids, unclaimed, helpers_retired) = self.runner.native_shutdown_inventory();
        let helpers_retired = helpers_retired && self.closures.is_empty();
        if self.control.requested()
            && complete
            && run_ids.is_empty()
            && unclaimed == 0
            && helpers_retired
        {
            // Store Drop may fsync; independent control remains available throughout.
            drop(self.store.take());
        }
        self.control.publish(
            run_ids,
            self.runner.native_preparation_inventory(),
            helpers_retired,
            complete,
            self.store.is_none(),
        );
        lines
    }
}
