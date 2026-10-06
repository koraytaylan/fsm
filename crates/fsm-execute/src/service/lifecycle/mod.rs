//! Explicitly driven native lifecycle with an independently waitable control.

mod control;
pub use control::{ExecutorControl, ExecutorPhase, ShutdownMode, ShutdownReport, ShutdownRequest};

use crate::{
    config::HandlerTable,
    error::ExecError,
    run::{Pipeline, Runner, native_client::NativeShutdown},
    sched::Scheduler,
    watch::Watcher,
};
use fsm_core::record::execution::Claim;
use fsm_store::{clock::Clock, store::Store};
use std::{collections::BTreeMap, time::Duration};

const MAX_CLOSURE_HELPERS: usize = 4;
const CLOSURE_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(3);

struct Closing {
    claim: Claim,
    shutdown: NativeShutdown,
}

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
    closing: BTreeMap<u64, Closing>,
    closure_cursor: u64,
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
            closing: BTreeMap::new(),
            closure_cursor: 0,
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
        if self.control.requested() {
            // These are this runner's client helpers, never foreign domains.
            self.runner.cancel_foreign_native_helpers();
        }
        if self.control.closure_requested() && complete {
            self.start_closures(&mut lines);
        }
        self.poll_closures(clock, &mut lines);
        let (run_ids, unclaimed, helpers_retired) = self.runner.native_shutdown_inventory();
        let helpers_retired = helpers_retired && self.closing.is_empty();
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
            unclaimed,
            helpers_retired,
            complete,
            self.store.is_none(),
        );
        lines
    }

    fn start_closures(&mut self, lines: &mut Vec<String>) {
        let candidates: Vec<Claim> = self
            .runner
            .local_native_claims()
            .filter(|claim| {
                !self.closing.contains_key(&claim.run_id())
                    && !self.runner.native_has_completion(claim)
            })
            .cloned()
            .collect();
        let ordered = candidates
            .iter()
            .filter(|claim| claim.run_id() > self.closure_cursor)
            .chain(
                candidates
                    .iter()
                    .filter(|claim| claim.run_id() <= self.closure_cursor),
            );
        // Bound failed starts too; a bad first target must not starve later owners.
        let mut cursor = self.closure_cursor;
        for (attempts, claim) in ordered.enumerate() {
            if attempts == MAX_CLOSURE_HELPERS || self.closing.len() == MAX_CLOSURE_HELPERS {
                break;
            }
            cursor = claim.run_id();
            let Some(store) = self.store.as_ref() else {
                break;
            };
            match NativeShutdown::start(store, claim, CLOSURE_TRANSPORT_TIMEOUT) {
                Ok(shutdown) => {
                    self.closing.insert(
                        claim.run_id(),
                        Closing {
                            claim: claim.clone(),
                            shutdown,
                        },
                    );
                }
                Err(message) => lines.push(super::error_line(&ExecError::new(
                    "exec/inflight_deferred",
                    message,
                ))),
            }
        }
        self.closure_cursor = cursor;
    }

    fn poll_closures(&mut self, clock: &mut dyn Clock, lines: &mut Vec<String>) {
        self.runner.finished_effects();
        let mut retired = Vec::new();
        for (run_id, closing) in &mut self.closing {
            let proof = closing.shutdown.poll();
            let helper_retired = closing.shutdown.reap().unwrap_or(false);
            if !self
                .runner
                .local_native_claims()
                .any(|claim| claim == &closing.claim)
            {
                if helper_retired {
                    retired.push(*run_id);
                }
                continue;
            }
            // Preserve authentic completion even when closure proof arrives first.
            if self.runner.native_has_completion(&closing.claim)
                || !self.runner.native_helper_retired(&closing.claim)
            {
                continue;
            }
            match proof {
                Ok(Some(_)) if helper_retired => {
                    let Some(store) = self.store.as_mut() else {
                        continue;
                    };
                    let result = closing
                        .shutdown
                        .settle_interrupted(store, clock)
                        .and_then(|_| {
                            self.runner.retire_native_interrupted(
                                store,
                                &closing.claim,
                                &mut closing.shutdown,
                                &mut self.scheduler,
                            )
                        });
                    match result {
                        Ok(true) => retired.push(*run_id),
                        Ok(false) => {}
                        Err(error) => lines.push(super::error_line(&error)),
                    }
                }
                Err(message) => {
                    lines.push(super::error_line(&ExecError::new(
                        "exec/inflight_deferred",
                        message,
                    )));
                    if helper_retired {
                        retired.push(*run_id);
                    }
                }
                _ => {}
            }
        }
        for run_id in retired {
            self.closing.remove(&run_id);
        }
    }
}
