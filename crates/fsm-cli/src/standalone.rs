//! Explicit standalone native ownership pump; endpoint publication belongs to the host.
use fsm_execute::{
    error::ExecError,
    service::{ExecutorPhase, PairedNativeExecutor, ShutdownMode, ShutdownReport, ShutdownRequest},
};
use fsm_store::clock::Clock;
use std::time::{Duration, Instant};
mod diagnostic_output;
use diagnostic_output::DiagnosticOutput;

pub struct StandaloneReport {
    pub shutdown: ShutdownReport,
    pub output_drained: bool,
    pub dropped_lines: u64,
    pub failure: Option<ExecError>,
}

/// Drive an actual paired owner with independently queued diagnostic output.
/// The host publishes control before entry; cleanup facts and output delivery
/// remain separate, and Drop does not promise either operation.
pub fn run_paired(
    driver: &mut PairedNativeExecutor,
    clock: &mut dyn Clock,
    writer: impl std::io::Write + Send + 'static,
    interval_ms: u64,
    exclusive: bool,
    shutdown_timeout_ms: i64,
) -> Result<StandaloneReport, ExecError> {
    if interval_ms == 0 || !(1..=fsm_execute::config::MAX_TIMEOUT_MS).contains(&shutdown_timeout_ms)
    {
        return Err(ExecError::new(
            "exec/config",
            "standalone interval and shutdown timeout must be valid before output startup",
        ));
    }
    let mut output = DiagnosticOutput::start(writer).map_err(|error| {
        ExecError::new(
            "exec/inflight_deferred",
            format!("standalone output startup failed: {error}"),
        )
    })?;
    let result = drive(
        driver,
        clock,
        &mut output,
        interval_ms,
        exclusive,
        shutdown_timeout_ms,
    );
    // Metadata errors also close queue admission without joining its worker.
    output.close();
    result
}

pub(crate) fn drive(
    driver: &mut PairedNativeExecutor,
    clock: &mut dyn Clock,
    output: &mut DiagnosticOutput,
    interval_ms: u64,
    exclusive: bool,
    shutdown_timeout_ms: i64,
) -> Result<StandaloneReport, ExecError> {
    if interval_ms == 0 || !(1..=fsm_execute::config::MAX_TIMEOUT_MS).contains(&shutdown_timeout_ms)
    {
        return Err(ExecError::new(
            "exec/config",
            "standalone interval and shutdown timeout must be valid before driving ownership",
        ));
    }
    let control = driver.control();
    let interval = Duration::from_millis(interval_ms);
    let mut last_tick = None::<Instant>;
    let mut blocked = 0u32;
    let mut failure = None;
    loop {
        let report = control.report();
        let stopping = report.phase != ExecutorPhase::Running;
        let admission_closed = report.admission_closed;
        let due = last_tick.is_none_or(|last| last.elapsed() >= interval);
        let now_ms = clock.now_ms();
        let lines = if stopping || admission_closed || !due {
            driver.poll(clock, now_ms)
        } else {
            last_tick = Some(Instant::now());
            let outcome = driver.tick_reporting(clock, now_ms);
            blocked = if outcome.writer_unavailable {
                blocked.saturating_add(1)
            } else {
                0
            };
            if exclusive && blocked >= fsm_execute::service::BLOCKED_TICKS_BEFORE_FAIL {
                // Do not drop admitted ownership on a writer error: request abort,
                // retain the first deadline, and finish the bounded owner loop.
                failure.get_or_insert_with(|| {
                    ExecError::new(
                        "exec/mode",
                        "exclusive writer unavailable for three consecutive ticks",
                    )
                });
                control.stop(ShutdownMode::Abort, shutdown_timeout_ms)?;
            }
            outcome.lines
        };
        for line in lines {
            if let Err(error) = output.enqueue(&line) {
                failure.get_or_insert_with(|| {
                    ExecError::new(
                        "exec/inflight_deferred",
                        format!("standalone log delivery failed: {error}"),
                    )
                });
                control.stop(ShutdownMode::Abort, shutdown_timeout_ms)?;
            }
        }
        if output.is_broken() {
            failure.get_or_insert_with(|| {
                ExecError::new("exec/inflight_deferred", "standalone log worker failed")
            });
            control.stop(ShutdownMode::Abort, shutdown_timeout_ms)?;
        }
        let report = control.report();
        if matches!(
            report.phase,
            ExecutorPhase::Stopped | ExecutorPhase::Uncertain
        ) {
            // Recover the original request deadline without de-escalating abort.
            let request = control.stop(ShutdownMode::Drain, shutdown_timeout_ms)?;
            return Ok(finish(report, &request, output, failure));
        }
        // Explicit control wakes immediately; ordinary interval never prevents
        // admitted observation at this short bound. This is not plan 20 admission.
        if stopping {
            std::thread::sleep(Duration::from_millis(20));
        } else {
            control.wait_for_request(20)?;
        }
    }
}

fn finish(
    shutdown: ShutdownReport,
    request: &ShutdownRequest,
    output: &DiagnosticOutput,
    failure: Option<ExecError>,
) -> StandaloneReport {
    output.close();
    while !output.drained() && !output.is_broken() && Instant::now() < request.deadline() {
        std::thread::sleep(Duration::from_millis(1));
    }
    StandaloneReport {
        shutdown,
        output_drained: output.drained(),
        dropped_lines: output.dropped(),
        failure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_execute::config::HandlerTable;
    use fsm_store::store::Store;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct ReadyClock {
        ready: Option<mpsc::Sender<()>>,
        calls: usize,
    }
    impl Clock for ReadyClock {
        fn now_ms(&mut self) -> i64 {
            self.calls += 1;
            // Second loop observation proves the first scheduling wait returned.
            if self.calls == 2 {
                if let Some(ready) = self.ready.take() {
                    ready.send(()).unwrap();
                }
            }
            0
        }
    }
    #[test]
    fn actual_stop_interrupts_long_interval_while_another_writer_remains_held() {
        let root = std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("task cache"));
        assert!(!root.starts_with("/tmp"));
        let directory = root.join(format!(
            "standalone-loop-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let writer = Store::open(&directory).unwrap();
        let mut driver = PairedNativeExecutor::new(&directory, HandlerTable::default()).unwrap();
        let control = driver.control();
        let (ready, observed) = mpsc::channel();
        let (completed, result) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut output = DiagnosticOutput::start(std::io::sink()).unwrap();
            let report = drive(
                &mut driver,
                &mut ReadyClock {
                    ready: Some(ready),
                    calls: 0,
                },
                &mut output,
                2000,
                false,
                500,
            );
            completed.send(report).unwrap();
        });
        let ready_observed = observed.recv_timeout(Duration::from_secs(1));
        let request = control.stop(ShutdownMode::Abort, 500).unwrap();
        let delivered = result.recv_timeout(Duration::from_millis(1000));
        // A scheduling-sleep regression is finite at the chosen two-second
        // interval; retire worker before assertions and directory cleanup.
        worker.join().unwrap();
        let still_held = Store::open(&directory).is_err();
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
        ready_observed.unwrap();
        let report = delivered.unwrap().unwrap();
        assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
        assert_eq!(request.poll().phase, ExecutorPhase::Stopped);
        assert!(report.output_drained && still_held);
        assert_eq!(report.dropped_lines, 0);
        assert!(report.failure.is_none());
    }
}
