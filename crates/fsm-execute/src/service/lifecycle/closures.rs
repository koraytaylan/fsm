//! Original claim-bound closure transport and optional writer settlement.
use crate::{
    error::ExecError,
    run::{Runner, native_client::NativeShutdown},
    sched::Scheduler,
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
#[derive(Default)]
pub(super) struct Closures {
    entries: BTreeMap<u64, Closing>,
    cursor: u64,
}
impl Closures {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub(super) fn start(&mut self, runner: &Runner, store: &Store, lines: &mut Vec<String>) {
        let candidates: Vec<Claim> = runner
            .local_native_claims()
            .filter(|claim| {
                !self.entries.contains_key(&claim.run_id()) && !runner.native_has_completion(claim)
            })
            .cloned()
            .collect();
        let ordered = candidates
            .iter()
            .filter(|claim| claim.run_id() > self.cursor)
            .chain(
                candidates
                    .iter()
                    .filter(|claim| claim.run_id() <= self.cursor),
            );
        // Bound failed starts too; a bad first target must not starve later owners.
        let mut cursor = self.cursor;
        for (attempts, claim) in ordered.enumerate() {
            if attempts == MAX_CLOSURE_HELPERS || self.entries.len() == MAX_CLOSURE_HELPERS {
                break;
            }
            cursor = claim.run_id();
            match NativeShutdown::start(store, claim, CLOSURE_TRANSPORT_TIMEOUT) {
                Ok(shutdown) => {
                    self.entries.insert(
                        claim.run_id(),
                        Closing {
                            claim: claim.clone(),
                            shutdown,
                        },
                    );
                }
                Err(message) => lines.push(super::super::error_line(&ExecError::new(
                    "exec/inflight_deferred",
                    message,
                ))),
            }
        }
        self.cursor = cursor;
    }

    pub(super) fn poll(
        &mut self,
        runner: &mut Runner,
        scheduler: &mut Scheduler,
        mut writer: Option<&mut Store>,
        clock: &mut dyn Clock,
        lines: &mut Vec<String>,
    ) {
        runner.finished_effects();
        let mut retired = Vec::new();
        for (run_id, closing) in &mut self.entries {
            let proof = closing.shutdown.poll();
            let helper_retired = closing.shutdown.reap().unwrap_or(false);
            if !runner
                .local_native_claims()
                .any(|claim| claim == &closing.claim)
            {
                if helper_retired {
                    retired.push(*run_id);
                }
                continue;
            }
            // Preserve authentic completion even when closure proof arrives first.
            if runner.native_has_completion(&closing.claim)
                || !runner.native_helper_retired(&closing.claim)
            {
                continue;
            }
            match proof {
                Ok(Some(_)) if helper_retired => {
                    let Some(store) = writer.as_deref_mut() else {
                        continue;
                    };
                    let result = closing
                        .shutdown
                        .settle_interrupted(store, clock)
                        .and_then(|_| {
                            runner.retire_native_interrupted(
                                store,
                                &closing.claim,
                                &mut closing.shutdown,
                                scheduler,
                            )
                        });
                    match result {
                        Ok(true) => retired.push(*run_id),
                        Ok(false) => {}
                        Err(error) => lines.push(super::super::error_line(&error)),
                    }
                }
                Err(message) => {
                    lines.push(super::super::error_line(&ExecError::new(
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
            self.entries.remove(&run_id);
        }
    }
}
