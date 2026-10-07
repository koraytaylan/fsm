//! Actual byte framing, quiet input and original owned shutdown over Unix streams.

use std::{
    io::{BufReader, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

use fsm_execute::{
    config::HandlerTable,
    service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode},
};

use crate::{
    clock::FixedClock,
    mcp::{notify::SharedSink, serve::hosted},
    store::Store,
};

use super::{Scratch, seeded, value};

struct LogicalClock(Arc<AtomicI64>);
impl crate::clock::Clock for LogicalClock {
    fn now_ms(&mut self) -> i64 {
        self.0.load(Ordering::Acquire)
    }
}

#[test]
fn execution_host_owned_stdio_blocked_output_does_not_keep_the_writer() {
    use std::sync::mpsc;
    struct HeldOutput {
        ready: Option<mpsc::Sender<()>>,
        release: mpsc::Receiver<()>,
    }
    impl Write for HeldOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(ready) = self.ready.take() {
                ready.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let scratch = Scratch::new();
    let driver = OwnedNativeExecutor::new(seeded(&scratch.0), HandlerTable::default()).unwrap();
    let control = driver.control();
    let (mut client, server) = UnixStream::pair().unwrap();
    let (ready, blocked) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let (reported, report) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        reported
            .send(
                hosted::serve(
                    driver,
                    FixedClock::new(1000, 0),
                    move || BufReader::new(server),
                    HeldOutput {
                        ready: Some(ready),
                        release: wait,
                    },
                    std::io::sink(),
                )
                .unwrap(),
            )
            .unwrap();
    });
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
    writeln!(client, "{initialize}").unwrap();
    blocked.recv_timeout(Duration::from_secs(5)).unwrap();
    let original = control.stop(ShutdownMode::Abort, 1000).unwrap();
    let report = report.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(report.shutdown_deadline, original.deadline());
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.shutdown.writer_released && report.shutdown.helpers_retired);
    assert!(
        !report.output_drained,
        "an unreleased output fixture cannot prove delivery"
    );
    assert!(report.operator_output_drained);
    assert!(report.worker.is_none());
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
    release.send(()).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    worker.join().unwrap();
}

#[test]
fn execution_host_owned_stdio_quiet_deadline_and_eof_release_the_writer() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    store.define_machine_on(&mut FixedClock::new(1000, 0), value(r#"{"format":"fsm.machine/1","name":"stdio_deadline","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"done"}]}"#), false, false).unwrap();
    let driver = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
    let time = Arc::new(AtomicI64::new(1000));
    let clock = LogicalClock(Arc::clone(&time));
    let (mut client, server) = UnixStream::pair().unwrap();
    let output = SharedSink::new();
    let writer = output.writer();
    let worker = std::thread::spawn(move || {
        hosted::serve(
            driver,
            clock,
            move || BufReader::new(server),
            writer,
            std::io::sink(),
        )
        .unwrap()
    });
    for frame in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_create","arguments":{"machine":"stdio_deadline","request_id":"stdio-quiet"}}}"#,
    ] {
        writeln!(client, "{frame}").unwrap();
    }
    let watchdog = Instant::now() + Duration::from_secs(5);
    while output.text().lines().count() < 2 {
        assert!(
            Instant::now() < watchdog,
            "creation must reply over the actual byte stream"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    time.store(1001, Ordering::Release);
    loop {
        let observed = Store::open_read_only(&scratch.0).unwrap();
        if observed
            .instance_view("inst-stdio-quiet", None, None)
            .unwrap()
            .get("status")
            .and_then(fsm_core::json::Value::as_str)
            == Some("completed")
        {
            assert_eq!(observed.journal.last_seq, 3);
            break;
        }
        assert!(
            Instant::now() < watchdog,
            "open quiet input must not pause the deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(Store::open(&scratch.0).is_err());
    client.shutdown(Shutdown::Write).unwrap();
    let mut report = worker.join().unwrap();
    assert!(report.failure.is_none());
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(
        report.shutdown.writer_released
            && report.shutdown.inventory_complete
            && report.shutdown.helpers_retired
    );
    assert!(report.output_drained && report.operator_output_drained);
    assert_eq!(report.operator_lines_dropped, Some(0));
    assert!(report.worker.is_none());
    assert!(report.exit.as_mut().unwrap().driver.store_mut().is_none());
    assert_eq!(output.text().lines().count(), 2);
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 3);
}

#[test]
fn execution_host_owned_stdio_original_stop_interrupts_quiet_input() {
    let scratch = Scratch::new();
    let driver = OwnedNativeExecutor::new(seeded(&scratch.0), HandlerTable::default()).unwrap();
    let control = driver.control();
    let (client, server) = UnixStream::pair().unwrap();
    let worker = std::thread::spawn(move || {
        hosted::serve(
            driver,
            FixedClock::new(1000, 0),
            move || BufReader::new(server),
            std::io::sink(),
            std::io::sink(),
        )
        .unwrap()
    });
    let original = control.stop(ShutdownMode::Drain, 5000).unwrap();
    let report = worker.join().unwrap();
    assert_eq!(report.shutdown_deadline, original.deadline());
    assert!(report.failure.is_none());
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(
        report.shutdown.writer_released && report.output_drained && report.operator_output_drained
    );
    assert!(report.worker.is_none());
    // The reader worker may still hold the actual stream; no generic blocking
    // reader interruption is inferred from writer/native shutdown.
    client.shutdown(Shutdown::Write).unwrap();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
}
