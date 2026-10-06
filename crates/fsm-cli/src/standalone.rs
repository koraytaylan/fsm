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
            let report = run_paired(
                &mut driver,
                &mut ReadyClock {
                    ready: Some(ready),
                    calls: 0,
                },
                std::io::sink(),
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

    struct UnusedClock;
    impl Clock for UnusedClock {
        fn now_ms(&mut self) -> i64 {
            panic!("invalid options must not drive ownership")
        }
    }
    struct StartupWitness(mpsc::Sender<std::thread::ThreadId>);
    impl std::io::Write for StartupWitness {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            panic!("invalid options must not write")
        }
        fn flush(&mut self) -> std::io::Result<()> {
            panic!("invalid options must not flush")
        }
    }
    impl Drop for StartupWitness {
        fn drop(&mut self) {
            let _ = self.0.send(std::thread::current().id());
        }
    }
    #[test]
    fn invalid_public_options_refuse_before_worker_start_or_admission_closure() {
        let root = std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("task cache"));
        assert!(!root.starts_with("/tmp"));
        let directory = root.join(format!(
            "standalone-invalid-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let writer = Store::open(&directory).unwrap();
        let mut driver = PairedNativeExecutor::new(&directory, HandlerTable::default()).unwrap();
        for (interval, timeout) in [(0, 1), (1, 0), (1, fsm_execute::config::MAX_TIMEOUT_MS + 1)] {
            let (sent, observed) = mpsc::channel();
            let result = run_paired(
                &mut driver,
                &mut UnusedClock,
                StartupWitness(sent),
                interval,
                false,
                timeout,
            );
            let Err(error) = result else {
                panic!("invalid options accepted")
            };
            assert_eq!(error.code, "exec/config");
            assert_eq!(
                observed.recv_timeout(Duration::from_millis(100)).unwrap(),
                std::thread::current().id()
            );
            assert!(!driver.control().report().admission_closed);
            assert_eq!(driver.control().report().phase, ExecutorPhase::Running);
        }
        drop(driver);
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn exclusive_writer_contention_preserves_failure_after_actual_stop() {
        let root = std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("task cache"));
        assert!(!root.starts_with("/tmp"));
        let directory = root.join(format!(
            "standalone-exclusive-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let mut writer = Store::open(&directory).unwrap();
        let definition = fsm_core::json::parse(br#"{"format":"fsm.machine/1","name":"quiet","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"done"}]}"#, &fsm_core::json::JsonLimits::DEFAULT).unwrap();
        let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
        writer
            .define_machine_on(&mut clock, definition, false, false)
            .unwrap();
        writer
            .create_instance_ctx_on(
                &mut clock,
                "quiet",
                "instance",
                "create",
                None,
                &std::collections::BTreeMap::new(),
                &[],
            )
            .unwrap();
        let records = writer.records.clone();
        let mut driver = PairedNativeExecutor::new(&directory, HandlerTable::default()).unwrap();
        // Bound a broken exclusive predicate without allowing an infinite test.
        let control = driver.control();
        let fallback = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            control.stop(ShutdownMode::Abort, 500).unwrap();
        });
        let result = run_paired(
            &mut driver,
            &mut fsm_store::clock::FixedClock::new(10000, 1),
            std::io::sink(),
            1,
            true,
            500,
        );
        fallback.join().unwrap();
        let unchanged = Store::open_read_only(&directory).unwrap().records == records;
        let still_held = Store::open(&directory).is_err();
        drop(driver);
        drop(writer);
        std::fs::remove_dir_all(&directory).unwrap();
        let report = result.unwrap();
        assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
        assert_eq!(report.failure.unwrap().code, "exec/mode");
        assert!(report.shutdown.writer_released && report.output_drained);
        assert!(unchanged && still_held);
    }
}
