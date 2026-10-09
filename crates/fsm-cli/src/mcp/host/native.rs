//! Native owner integration retaining the original lifecycle driver through exit.
//!
//! Monotonic waits do not supply journal timestamps; client commands and native
//! decisions use the same writer. Transport construction and completion fairness
//! acceptance remain integration obligations before advertising autonomy.

use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};

use fsm_execute::service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode, ShutdownReport};

use crate::{
    clock::{Clock, FixedClock},
    mcp::notify::diagnostic_output::DiagnosticOutput,
};

use super::{
    Handle, apply_command,
    mailbox::{Mailbox, Next, Retirement},
};

const COMMAND_BATCH: usize = 8;
const EXECUTOR_TURNS: usize = 8;
const OBSERVATION_INTERVAL: Duration = Duration::from_millis(50);

/// Wait time wakes the owner but never supplies journal timestamps.
pub(super) trait WaitClock: Send {
    fn now(&self) -> Instant;

    fn wait(&self, mailbox: &Mailbox, deadline: Instant) -> Next {
        mailbox.next_until_with(deadline, || self.now())
    }
}

struct MonotonicWaitClock;

impl WaitClock for MonotonicWaitClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Carries the original driver back even when shutdown cannot prove closure.
pub(in crate::mcp) struct NativeExit {
    pub driver: OwnedNativeExecutor,
    pub shutdown: ShutdownReport,
    pub diagnostics: DiagnosticOutput,
    pub failure: Option<io::Error>,
}

pub(in crate::mcp) struct NativeOwner<C> {
    _retirement: Retirement,
    driver: OwnedNativeExecutor,
    clock: C,
    wait_clock: Box<dyn WaitClock>,
    mailbox: Arc<Mailbox>,
    diagnostics: DiagnosticOutput,
    interval: Duration,
    decision_ready: bool,
    shutdown_timeout_ms: i64,
    publication: Option<crate::mcp::notify::Notifier>,
}

impl<C: Clock> NativeOwner<C> {
    pub(in crate::mcp) fn new(
        mut driver: OwnedNativeExecutor,
        clock: C,
        diagnostics: DiagnosticOutput,
        interval: Duration,
        shutdown_timeout_ms: i64,
    ) -> io::Result<(Self, Handle)> {
        if interval.is_zero()
            || interval > Duration::from_millis(fsm_execute::config::MAX_TIMEOUT_MS as u64)
            || !(1..=fsm_execute::config::MAX_TIMEOUT_MS).contains(&shutdown_timeout_ms)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "native host interval or shutdown timeout outside finite bounds",
            ));
        }
        driver.enable_worker_polling();
        let mailbox = Arc::new(Mailbox::with_committed_prefix(
            driver
                .store_mut()
                .and_then(|store| store.journal.committed_prefix()),
        ));
        let handle = Handle {
            mailbox: Arc::clone(&mailbox),
            operator_handlers: Some(Arc::new(driver.handler_table().clone())),
            native_stop: Some((driver.control(), shutdown_timeout_ms)),
        };
        Ok((
            Self {
                _retirement: Retirement(Arc::clone(&mailbox)),
                driver,
                clock,
                wait_clock: Box::new(MonotonicWaitClock),
                mailbox,
                diagnostics,
                interval,
                decision_ready: false,
                shutdown_timeout_ms,
                publication: None,
            },
            handle,
        ))
    }

    pub(in crate::mcp) fn with_publication(
        mut self,
        output: &crate::mcp::notify::Notifier,
    ) -> Self {
        output.bind_committed_prefix(self.mailbox.committed.clone());
        self.publication = output.hosted_handle();
        self
    }

    #[cfg(test)]
    pub(super) fn with_wait_clock(mut self, clock: impl WaitClock + 'static) -> Self {
        self.wait_clock = Box::new(clock);
        self
    }

    /// Service the original native driver independently of application input.
    pub(in crate::mcp) fn run(mut self) -> NativeExit {
        let control = self.driver.control();
        let handlers = crate::mcp::executor::handlers(self.driver.handler_table());
        let mut next_pass = self.wait_clock.now();
        let mut next_observation = next_pass;
        let mut inventory_unpublished = true;
        let mut commands = 0;
        let mut failure = None;
        loop {
            let report = control.report();
            if report.phase != ExecutorPhase::Running || self.diagnostics.is_broken() {
                if self.diagnostics.is_broken() {
                    failure = Some(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "native operator output failed",
                    ));
                }
                break;
            }
            if commands == COMMAND_BATCH || self.wait_clock.now() >= next_pass {
                let lines = self.decision_pass();
                if let Err(error) = publish(&mut self.diagnostics, lines) {
                    failure = Some(error);
                    break;
                }
                commands = 0;
                next_pass = if self.decision_ready {
                    self.wait_clock.now()
                } else {
                    self.wait_clock.now() + self.interval
                };
                next_observation = self.wait_clock.now() + OBSERVATION_INTERVAL;
                inventory_unpublished = true;
            } else if self.wait_clock.now() >= next_observation {
                // An ordinary tick may change retained work without publishing
                // lifecycle inventory; publish once before trusting quiescence.
                if inventory_unpublished
                    || !report.inventory_complete
                    || !report.helpers_retired
                    || !report.unresolved_run_ids.is_empty()
                    || report.unclaimed_reservations != Some(0)
                {
                    let lines = self.observation_pass();
                    if let Err(error) = publish(&mut self.diagnostics, lines) {
                        failure = Some(error);
                        break;
                    }
                    inventory_unpublished = false;
                    // Observation cannot authorize binding or entry; wake the
                    // original writer decision rather than waiting on its timer.
                    if self.driver.has_ready_native_work() {
                        next_pass = self.wait_clock.now();
                    }
                }
                next_observation = self.wait_clock.now() + OBSERVATION_INTERVAL;
            }
            // Independent lifecycle control does not need an application
            // command to wake a long configured scheduler interval.
            let control_check = self.wait_clock.now() + OBSERVATION_INTERVAL;
            match self.wait_clock.wait(
                &self.mailbox,
                next_pass.min(next_observation).min(control_check),
            ) {
                Next::Command(admitted) => {
                    if let Some(store) = self.driver.store_mut() {
                        apply_command(store, &mut self.clock, admitted, Some(&handlers));
                    }
                    commands += 1;
                }
                Next::Due => {}
                Next::Stopped => break,
            }
        }
        self.mailbox.stop();
        // The lifecycle controller preserves any earlier request's deadline and
        // escalation; queued application work cannot postpone this request.
        let mode = if control.report().phase == ExecutorPhase::Running {
            ShutdownMode::Abort
        } else {
            ShutdownMode::Drain
        };
        let request = control
            .stop(mode, self.shutdown_timeout_ms)
            .expect("validated native host shutdown timeout");
        let shutdown = loop {
            let _publication = self
                .publication
                .as_ref()
                .and_then(crate::mcp::notify::Notifier::publication_guard);
            let now_ms = self.clock.now_ms();
            let lines = self.driver.poll(&mut FixedClock::new(now_ms, 0), now_ms);
            if let Err(error) = publish(&mut self.diagnostics, lines) {
                failure.get_or_insert(error);
            }
            let report = request.poll();
            if matches!(
                report.phase,
                ExecutorPhase::Stopped | ExecutorPhase::Uncertain
            ) {
                break report;
            }
            // No application dispatch during shutdown, and no new logical time
            // derived from this sleep; original claims remain driver-owned.
            std::thread::sleep(Duration::from_millis(10));
        };
        self.diagnostics.close();
        NativeExit {
            driver: self.driver,
            shutdown,
            diagnostics: self.diagnostics,
            failure,
        }
    }

    /// SPEC: one logical sample supplies every operation in this decision pass.
    pub(super) fn decision_pass(&mut self) -> Vec<String> {
        self.decision_pass_after_commit(|| {})
    }

    /// Observe retained original work without scheduling admission or deadlines.
    pub(super) fn observation_pass(&mut self) -> Vec<String> {
        let _publication = self
            .publication
            .as_ref()
            .and_then(crate::mcp::notify::Notifier::publication_guard);
        let now_ms = self.clock.now_ms();
        self.driver.poll(&mut FixedClock::new(now_ms, 0), now_ms)
    }

    // Tests pause only after the original driver returns, preserving the real
    // journal operation and publication scope used by production's no-op hook.
    pub(super) fn decision_pass_after_commit(
        &mut self,
        after_commit: impl FnOnce(),
    ) -> Vec<String> {
        let _publication = self
            .publication
            .as_ref()
            .and_then(crate::mcp::notify::Notifier::publication_guard);
        let now_ms = self.clock.now_ms();
        let mut clock = FixedClock::new(now_ms, 0);
        let control = self.driver.control();
        let mut lines = Vec::new();
        self.decision_ready = false;
        for _ in 0..EXECUTOR_TURNS {
            if control.report().phase != ExecutorPhase::Running {
                self.decision_ready = false;
                break;
            }
            let Some(before) = self.driver.store_mut().map(|store| store.journal.last_seq) else {
                break;
            };
            let tick = self.driver.tick(&mut clock, now_ms);
            let refused = tick.iter().any(|line| line.starts_with("error "));
            lines.extend(tick);
            let progressed = self
                .driver
                .store_mut()
                .is_some_and(|store| store.journal.last_seq != before);
            // Rescan only after durable progress or retained local readiness;
            // an unchanged fixed clock cannot turn this batch into an idle spin.
            self.decision_ready = !refused && (progressed || self.driver.has_ready_native_work());
            if !self.decision_ready {
                break;
            }
        }
        after_commit();
        lines
    }
}

fn publish(diagnostics: &mut DiagnosticOutput, lines: Vec<String>) -> io::Result<()> {
    for line in lines {
        diagnostics.enqueue(&format!("fsm execute: {line}"))?;
    }
    Ok(())
}
