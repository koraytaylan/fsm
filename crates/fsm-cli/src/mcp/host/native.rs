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

use crate::{clock::Clock, mcp::notify::diagnostic_output::DiagnosticOutput};

use super::{
    Handle, apply_command,
    mailbox::{Mailbox, Next, Retirement},
};

const COMMAND_BATCH: usize = 8;

/// Carries the original driver back even when shutdown cannot prove closure.
pub(super) struct NativeExit {
    pub driver: OwnedNativeExecutor,
    pub shutdown: ShutdownReport,
    pub diagnostics: DiagnosticOutput,
    pub failure: Option<io::Error>,
}

pub(super) struct NativeOwner<C> {
    _retirement: Retirement,
    driver: OwnedNativeExecutor,
    clock: C,
    mailbox: Arc<Mailbox>,
    diagnostics: DiagnosticOutput,
    interval: Duration,
    shutdown_timeout_ms: i64,
}

impl<C: Clock> NativeOwner<C> {
    pub(super) fn new(
        driver: OwnedNativeExecutor,
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
        let mailbox = Arc::new(Mailbox::default());
        let handle = Handle {
            mailbox: Arc::clone(&mailbox),
            native_stop: Some((driver.control(), shutdown_timeout_ms)),
        };
        Ok((
            Self {
                _retirement: Retirement(Arc::clone(&mailbox)),
                driver,
                clock,
                mailbox,
                diagnostics,
                interval,
                shutdown_timeout_ms,
            },
            handle,
        ))
    }

    /// Service the original native driver independently of application input.
    pub(super) fn run(mut self) -> NativeExit {
        let control = self.driver.control();
        let mut next_pass = Instant::now();
        let mut commands = 0;
        let mut failure = None;
        loop {
            if control.report().phase != ExecutorPhase::Running || self.diagnostics.is_broken() {
                if self.diagnostics.is_broken() {
                    failure = Some(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "native operator output failed",
                    ));
                }
                break;
            }
            if commands == COMMAND_BATCH || Instant::now() >= next_pass {
                let now_ms = self.clock.now_ms();
                let lines = self.driver.tick(&mut self.clock, now_ms);
                if let Err(error) = publish(&mut self.diagnostics, lines) {
                    failure = Some(error);
                    break;
                }
                commands = 0;
                next_pass = Instant::now() + self.interval;
            }
            // Independent lifecycle control does not need an application
            // command to wake a long configured scheduler interval.
            let control_check = Instant::now() + Duration::from_millis(50);
            match self.mailbox.next_until(next_pass.min(control_check)) {
                Next::Command(admitted) => {
                    if let Some(store) = self.driver.store_mut() {
                        apply_command(store, &mut self.clock, admitted);
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
            let now_ms = self.clock.now_ms();
            let lines = self.driver.poll(&mut self.clock, now_ms);
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
}

fn publish(diagnostics: &mut DiagnosticOutput, lines: Vec<String>) -> io::Result<()> {
    for line in lines {
        diagnostics.enqueue(&format!("fsm execute: {line}"))?;
    }
    Ok(())
}
