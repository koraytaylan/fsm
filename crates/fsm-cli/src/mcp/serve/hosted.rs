//! Owned stdio composition over the shared framing and method implementation.
//!
//! The writer lives on its native owner worker; an uncertain shutdown retains
//! that original worker in the report. Production selection awaits interactive
//! continuations, progress/egress integration and versioned discovery.

// This composition is staged until process-entry selection and protocol completion.
#![allow(dead_code)]

use std::{
    io::{self, BufRead, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use fsm_execute::service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode};

use crate::{
    clock::{Clock, SystemClock},
    mcp::{
        host::native::{NativeExit, NativeOwner},
        notify::{Notifier, diagnostic_output::DiagnosticOutput},
        owned_input::OwnedInput,
    },
};

use super::{SessionRuntime, SessionStore, serve_session_core};

const SHUTDOWN_TIMEOUT_MS: i64 = 10000;
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(in crate::mcp) struct HostedReport {
    pub shutdown: fsm_execute::service::ShutdownReport,
    pub output_drained: bool,
    pub operator_output_drained: bool,
    pub operator_lines_dropped: Option<u64>,
    pub shutdown_deadline: Instant,
    pub failure: Option<io::Error>,
    pub exit: Option<NativeExit>,
    /// Never join a live/stalled worker merely because its deadline expired.
    pub worker: Option<JoinHandle<NativeExit>>,
}

struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Compose actual bounded byte input/output with the owned command host.
#[allow(dead_code)] // Process-entry selection follows the remaining protocol integration.
pub(in crate::mcp) fn serve<C: Clock + Send + 'static, R: BufRead + 'static>(
    mut driver: OwnedNativeExecutor,
    clock: C,
    input: impl FnOnce() -> R + Send + 'static,
    output: impl Write + Send + 'static,
    operator_output: impl Write + Send + 'static,
) -> io::Result<HostedReport> {
    let data_dir = driver
        .store_mut()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "hosted session requires its original writer",
            )
        })?
        .data_dir
        .clone();
    let handlers = crate::mcp::executor::handlers(driver.handler_table());
    let control = driver.control();
    let diagnostics = DiagnosticOutput::start(operator_output)?;
    let mut adapter_diagnostics = diagnostics.fork();
    let (notifier, queued) = Notifier::queued(Box::new(output))?;
    let (owner, handle) = NativeOwner::new(
        driver,
        clock,
        diagnostics,
        POLL_INTERVAL,
        SHUTDOWN_TIMEOUT_MS,
    )?;
    let session = handle
        .session()
        .map_err(|error| io::Error::other(format!("host session admission failed: {error:?}")))?;
    let finished = Arc::new(AtomicBool::new(false));
    let worker_finished = Finished(Arc::clone(&finished));
    let worker = std::thread::Builder::new()
        .name("fsm-execution-host".into())
        .spawn(move || {
            let _finished = worker_finished;
            owner.run()
        })?;
    let input_control = control.clone();
    let input_output = queued.clone();
    let result = (|| {
        let input = OwnedInput::start(input, move || {
            input_control.report().phase != ExecutorPhase::Running
                || input_output.is_broken()
                || finished.load(Ordering::Acquire)
        })?;
        serve_session_core(
            SessionRuntime {
                store: SessionStore::Hosted {
                    session: &session,
                    data_dir: &data_dir,
                },
                executor: None,
                handlers: Some(handlers),
                bounded_shutdown: true,
                diagnostics: Some(&mut adapter_diagnostics),
            },
            &mut SystemClock,
            None,
            None,
            input,
            notifier,
        )
    })();
    session.close();
    let explicit_stop = control.report().phase != ExecutorPhase::Running;
    let request = control
        .stop(
            if explicit_stop {
                ShutdownMode::Drain
            } else {
                ShutdownMode::Abort
            },
            SHUTDOWN_TIMEOUT_MS,
        )
        .map_err(|error| io::Error::other(error.message))?;
    handle.reject_queued();
    queued.close();
    let shutdown = request.wait();
    let mut worker = Some(worker);
    let mut exit = None;
    let mut failure = match result {
        Err(error) if explicit_stop && error.kind() == io::ErrorKind::Interrupted => None,
        result => result.err(),
    };
    loop {
        if worker.as_ref().is_some_and(JoinHandle::is_finished) {
            match worker.take().expect("observed finished worker").join() {
                Ok(mut retired) => {
                    if let Some(error) = retired.failure.take() {
                        failure.get_or_insert(error);
                    }
                    exit = Some(retired);
                }
                Err(_) => {
                    failure.get_or_insert_with(|| {
                        io::Error::other("execution host panicked; closure remains unproven")
                    });
                }
            }
        }
        let operator_drained = exit.as_ref().is_some_and(|exit| exit.diagnostics.drained());
        if (worker.is_none() && queued.drained() && operator_drained)
            || Instant::now() >= request.deadline()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(HostedReport {
        shutdown,
        output_drained: queued.drained(),
        operator_output_drained: exit.is_some() && adapter_diagnostics.drained(),
        operator_lines_dropped: exit.as_ref().map(|exit| {
            exit.diagnostics
                .dropped()
                .saturating_add(adapter_diagnostics.dropped())
        }),
        shutdown_deadline: request.deadline(),
        failure,
        exit,
        worker,
    })
}
