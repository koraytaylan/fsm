//! Owned native protocol composition, with bounded independent I/O admission.

use super::{SessionRuntime, SessionStore, serve_session_core};
use crate::{
    clock::Clock,
    mcp::{notify::Notifier, owned_input::OwnedInput},
};
use fsm_execute::service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode, ShutdownReport};
use std::{
    io::{self, BufRead, Write},
    time::{Duration, Instant},
};

/// Native cleanup and separate protocol-delivery facts for an owned session.
pub struct OwnedSessionReport {
    /// Original native ownership, helper retirement and writer release.
    pub shutdown: ShutdownReport,
    /// Actual successful write/flush of every admitted output frame.
    pub output_drained: bool,
    /// Original request deadline, reusable for endpoint retirement.
    pub shutdown_deadline: Instant,
    /// Initiating protocol failure retained alongside actual cleanup facts.
    pub failure: Option<io::Error>,
}

/// Serve an explicitly selected native driver through one journal owner.
///
/// Construct input inside its sole reader worker; borrowed session APIs retain
/// their existing bounds. Quiet input permits only admitted observation, and
/// independent control interrupts both outer and reverse-reply waiting.
/// Output and feed I/O never hold the owner while waiting on a client.
/// Native/journal I/O still runs on this worker: control waits independently
/// return uncertainty at their first deadline if this worker stalls.
/// This entry does not select the production CLI backend or install authority.
pub fn serve_owned_native_session<R: BufRead + 'static>(
    driver: &mut OwnedNativeExecutor,
    clock: &mut dyn Clock,
    input: impl FnOnce() -> R + Send + 'static,
    output: impl Write + Send + 'static,
    shutdown_timeout_ms: i64,
) -> io::Result<OwnedSessionReport> {
    let mut report =
        serve_owned_native_session_reporting(driver, clock, input, output, shutdown_timeout_ms)?;
    match report.failure.take() {
        Some(error) => Err(error),
        None => Ok(report),
    }
}

/// Retain actual cleanup and the initiating I/O failure in the same result.
/// Invalid options still refuse before worker startup or admission changes.
pub fn serve_owned_native_session_reporting<R: BufRead + 'static>(
    driver: &mut OwnedNativeExecutor,
    clock: &mut dyn Clock,
    input: impl FnOnce() -> R + Send + 'static,
    output: impl Write + Send + 'static,
    shutdown_timeout_ms: i64,
) -> io::Result<OwnedSessionReport> {
    if !(1..=fsm_execute::config::MAX_TIMEOUT_MS).contains(&shutdown_timeout_ms) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "owned session shutdown timeout outside finite bounds",
        ));
    }
    let control = driver.control();
    let input_control = control.clone();
    let handlers = super::super::executor::handlers(driver.handler_table());
    let mut output_control = None;
    let result = (|| {
        let (notifier, queued) = Notifier::queued(Box::new(output))?;
        output_control = Some(queued);
        let input = OwnedInput::start(input, move || {
            input_control.report().phase != ExecutorPhase::Running
        })?;
        serve_session_core(
            SessionRuntime {
                store: SessionStore::Native(driver),
                executor: None,
                handlers: Some(handlers),
                bounded_shutdown: true,
            },
            clock,
            None,
            None,
            input,
            notifier,
        )
    })();
    if let Some(output) = &output_control {
        output.close();
    }
    let explicit_stop = control.report().phase != ExecutorPhase::Running;
    let mode = if explicit_stop {
        ShutdownMode::Drain
    } else {
        ShutdownMode::Abort
    };
    // Reusing a request preserves its original mode escalation and deadline.
    let request = control
        .stop(mode, shutdown_timeout_ms)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.message))?;
    let shutdown = loop {
        let now_ms = clock.now_ms();
        driver.poll(clock, now_ms);
        let report = request.poll();
        if matches!(
            report.phase,
            ExecutorPhase::Stopped | ExecutorPhase::Uncertain
        ) {
            break report;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Healthy EOF must not lose queued replies on immediate empty native stop;
    // blocked delivery remains false at the same original deadline.
    while output_control
        .as_ref()
        .is_some_and(|output| !output.drained() && !output.is_broken())
        && Instant::now() < request.deadline()
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    let output_drained = output_control
        .as_ref()
        .is_some_and(|output| output.drained());
    let failure = match result {
        Err(error) if explicit_stop && error.kind() == io::ErrorKind::Interrupted => None,
        other => other.err(),
    };
    Ok(OwnedSessionReport {
        shutdown,
        output_drained,
        shutdown_deadline: request.deadline(),
        failure,
    })
}
