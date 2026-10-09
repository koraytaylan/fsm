//! Server-scoped native ownership; HTTP sessions cannot stop the executor.
use super::SharedWriter;
use crate::{
    clock::SystemClock,
    local_control::LocalControlEndpoint,
    mcp::{
        host::{
            Handle,
            native::{NativeExit, NativeOwner},
        },
        notify::diagnostic_output::DiagnosticOutput,
        serve::ExecutorLoop,
    },
    store::Store,
};
use fsm_execute::service::{ExecutorControl, ExecutorPhase, ShutdownMode};
use std::{
    io,
    os::unix::fs::DirBuilderExt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

const SHUTDOWN_TIMEOUT_MS: i64 = 10000;

pub(crate) struct NativeServer {
    control: ExecutorControl,
    handle: Handle,
    endpoint: LocalControlEndpoint,
    worker: Option<JoinHandle<NativeExit>>,
}

struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl NativeServer {
    pub(crate) fn start(
        store: Store,
        executor: ExecutorLoop,
        stop: Arc<AtomicBool>,
    ) -> io::Result<(Arc<SharedWriter>, Self)> {
        Self::start_with_hook(store, executor, stop, || {})
    }

    // Pause before the original owner runs for retirement uncertainty proof;
    // production's hook is inert and creates no second ownership boundary.
    fn start_with_hook(
        store: Store,
        executor: ExecutorLoop,
        stop: Arc<AtomicBool>,
        ready: impl FnOnce() + Send + 'static,
    ) -> io::Result<(Arc<SharedWriter>, Self)> {
        let data_dir = store.data_dir.clone();
        let interval = executor.poll_interval;
        let mut driver = executor.into_native(store).map_err(io::Error::other)?;
        let control = driver.control();
        let root = std::path::PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "native HTTP publication requires HOME",
            )
        })?)
        .join(".cache/fsm/control");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&root)?;
        let endpoint = LocalControlEndpoint::publish(&root, &mut driver)?;
        let diagnostics = DiagnosticOutput::start(io::stderr())?;
        let (owner, handle) = NativeOwner::new(
            driver,
            SystemClock,
            diagnostics,
            interval,
            SHUTDOWN_TIMEOUT_MS,
        )?;
        let finished = Finished(stop);
        let worker = std::thread::Builder::new()
            .name("fsm-http-execution-host".into())
            .spawn(move || {
                let _finished = finished;
                ready();
                owner.run()
            })?;
        let host = Arc::new(SharedWriter {
            handle: handle.clone(),
            data_dir,
            worker: None,
        });
        Ok((
            host,
            Self {
                control,
                handle,
                endpoint,
                worker: Some(worker),
            },
        ))
    }

    /// Observe only the original worker and first monotonic stop deadline.
    pub(crate) fn finish(&mut self) -> io::Result<()> {
        let mode = if self.control.report().phase == ExecutorPhase::Running {
            ShutdownMode::Abort
        } else {
            ShutdownMode::Drain
        };
        let request = self
            .control
            .stop(mode, SHUTDOWN_TIMEOUT_MS)
            .map_err(io::Error::other)?;
        self.handle.reject_queued();
        let shutdown = request.wait();
        let removal = self.endpoint.close_until(request.deadline());
        let removed = removal.as_ref().copied().unwrap_or(false);
        let mut exit = None;
        let mut failure = removal.err();
        loop {
            if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                match self.worker.take().expect("observed original worker").join() {
                    Ok(mut retired) => {
                        if let Some(error) = retired.failure.take() {
                            failure.get_or_insert(error);
                        }
                        exit = Some(retired);
                    }
                    Err(_) => {
                        failure
                            .get_or_insert_with(|| io::Error::other("native HTTP owner panicked"));
                    }
                }
            }
            let diagnostics_retired = exit
                .as_ref()
                .is_some_and(|exit| exit.diagnostics.drained() || exit.diagnostics.is_broken());
            if (self.worker.is_none() && diagnostics_retired)
                || Instant::now() >= request.deadline()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let confirmed = shutdown.phase == ExecutorPhase::Stopped
            && shutdown.admission_closed
            && shutdown.inventory_complete
            && shutdown.helpers_retired
            && shutdown.writer_released
            && shutdown.unresolved_run_ids.is_empty()
            && shutdown.unclaimed_reservations == Some(0)
            && removed
            && self.worker.is_none()
            && exit.as_ref().is_some_and(|exit| {
                exit.shutdown.phase == ExecutorPhase::Stopped
                    && exit.diagnostics.drained()
                    && exit.diagnostics.dropped() == 0
            });
        if confirmed && failure.is_none() {
            return Ok(());
        }
        Err(io::Error::other(
            crate::native_error::NativeSessionFailure {
                error: fsm_execute::error::ExecError::new(
                    "exec/inflight_deferred",
                    failure.map_or_else(
                        || "native HTTP retirement remains uncertain".into(),
                        |error| error.to_string(),
                    ),
                )
                .details(retirement_details(
                    &shutdown,
                    removed,
                    self.worker.is_none(),
                    exit.as_ref(),
                )),
                deadline: request.deadline(),
            },
        ))
    }
}

fn retirement_details(
    shutdown: &fsm_execute::service::ShutdownReport,
    endpoint_removed: bool,
    worker_joined: bool,
    exit: Option<&NativeExit>,
) -> fsm_core::json::Value {
    use fsm_core::json::Value;
    let phase = match shutdown.phase {
        ExecutorPhase::Running => "running",
        ExecutorPhase::Draining => "draining",
        ExecutorPhase::Stopping => "stopping",
        ExecutorPhase::Stopped => "stopped",
        ExecutorPhase::Uncertain => "uncertain",
    };
    Value::Obj(std::collections::BTreeMap::from([
        ("phase".into(), Value::Str(phase.into())),
        (
            "admission_closed".into(),
            Value::Bool(shutdown.admission_closed),
        ),
        ("timed_out".into(), Value::Bool(shutdown.timed_out)),
        (
            "inventory_complete".into(),
            Value::Bool(shutdown.inventory_complete),
        ),
        (
            "helpers_retired".into(),
            Value::Bool(shutdown.helpers_retired),
        ),
        (
            "writer_released".into(),
            Value::Bool(shutdown.writer_released),
        ),
        (
            "unresolved_run_ids".into(),
            Value::Arr(
                shutdown
                    .unresolved_run_ids
                    .iter()
                    .map(|id| Value::Str(id.to_string()))
                    .collect(),
            ),
        ),
        (
            "unclaimed_reservations".into(),
            shutdown
                .unclaimed_reservations
                .map_or(Value::Null, |count| Value::Str(count.to_string())),
        ),
        ("endpoint_removed".into(), Value::Bool(endpoint_removed)),
        ("worker_joined".into(), Value::Bool(worker_joined)),
        ("worker_returned".into(), Value::Bool(exit.is_some())),
        (
            "operator_output_drained".into(),
            Value::Bool(exit.is_some_and(|exit| exit.diagnostics.drained())),
        ),
        (
            "operator_lines_dropped".into(),
            exit.map_or(Value::Null, |exit| {
                Value::Str(exit.diagnostics.dropped().to_string())
            }),
        ),
    ]))
}

impl Drop for NativeServer {
    fn drop(&mut self) {
        // Closing admission is not cleanup proof and never joins a live worker.
        let mode = if self.control.report().phase == ExecutorPhase::Running {
            ShutdownMode::Abort
        } else {
            ShutdownMode::Drain
        };
        let _ = self.control.stop(mode, SHUTDOWN_TIMEOUT_MS);
        self.handle.reject_queued();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn native_http_retirement_preserves_first_deadline_and_unobserved_worker_facts() {
        let directory =
            std::env::temp_dir().join(format!("http-held-owner-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let store = Store::open(&directory).unwrap();
        let executor =
            ExecutorLoop::new(&directory, fsm_execute::config::HandlerTable::default()).unwrap();
        let (ready, entered) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let (host, mut native) = NativeServer::start_with_hook(
            store,
            executor,
            Arc::new(AtomicBool::new(false)),
            move || {
                ready.send(()).unwrap();
                held.recv_timeout(Duration::from_secs(3)).unwrap();
            },
        )
        .unwrap();
        entered.recv_timeout(Duration::from_secs(1)).unwrap();
        let first = native.control.stop(ShutdownMode::Drain, 25).unwrap();
        let result = native.finish();
        let deadline = native
            .control
            .stop(ShutdownMode::Drain, 10000)
            .unwrap()
            .deadline();
        let writer_held = Store::open(&directory).is_err();
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(1);
        while !native.worker.as_ref().unwrap().is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(1));
        }
        // The original finite hook also releases on timeout; cleanup precedes
        // assertions so a regression leaves no parked owner or fixture files.
        let exit = native.worker.take().unwrap().join().unwrap();
        assert!(native.endpoint.close(1000).unwrap());
        drop(exit);
        drop(host);
        drop(native);
        std::fs::remove_dir_all(&directory).unwrap();
        let error = result.unwrap_err();
        let failure = error
            .get_ref()
            .unwrap()
            .downcast_ref::<crate::native_error::NativeSessionFailure>()
            .unwrap();
        assert_eq!(first.deadline(), deadline);
        assert_eq!(failure.deadline, first.deadline());
        assert!(writer_held);
        assert_eq!(failure.error.code, "exec/inflight_deferred");
        let details = failure.error.details.as_ref().unwrap();
        use fsm_core::json::Value;
        assert_eq!(
            details.get("phase").and_then(Value::as_str),
            Some("uncertain")
        );
        assert_eq!(details.get("timed_out"), Some(&Value::Bool(true)));
        for field in [
            "inventory_complete",
            "helpers_retired",
            "writer_released",
            "worker_joined",
            "worker_returned",
            "operator_output_drained",
        ] {
            assert_eq!(details.get(field), Some(&Value::Bool(false)), "{field}");
        }
        assert_eq!(details.get("unclaimed_reservations"), Some(&Value::Null));
        assert_eq!(details.get("operator_lines_dropped"), Some(&Value::Null));
    }
}
