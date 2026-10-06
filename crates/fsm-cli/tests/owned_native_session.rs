//! Actual writer and protocol-worker facts; no fabricated native domain proof.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use fsm_cli::mcp::serve::serve_owned_native_session;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::{
    config::HandlerTable,
    service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode},
};
use fsm_store::{
    clock::{Clock, FixedClock},
    store::Store,
};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Cursor, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const INITIALIZE: &[u8] = b"{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"initialize\",\"params\":{\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"0\"},\"protocolVersion\":\"2025-06-18\"}}\n";
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache required"));
        assert!(!root.starts_with("/tmp"));
        let path = root.join(format!(
            "fsm-owned-session-{}-{}",
            std::process::id(),
            DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

struct HeldReader {
    prefix: Cursor<Vec<u8>>,
    ready: Option<Sender<()>>,
    release: Receiver<()>,
}
impl Read for HeldReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(buffer.len());
        buffer[..count].copy_from_slice(&available[..count]);
        self.consume(count);
        Ok(count)
    }
}
impl BufRead for HeldReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.prefix.position() < self.prefix.get_ref().len() as u64 {
            return self.prefix.fill_buf();
        }
        if let Some(ready) = self.ready.take() {
            ready.send(()).unwrap();
            self.release.recv().unwrap();
        }
        Ok(&[])
    }
    fn consume(&mut self, count: usize) {
        self.prefix.consume(count);
    }
}
struct HeldWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    ready: Option<Sender<()>>,
    release: Receiver<()>,
}
impl Write for HeldWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(ready) = self.ready.take() {
            ready.send(()).unwrap();
            self.release.recv().unwrap();
        }
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
// Release held workers even when an assertion unwinds; the holds never model
// native closure, and each test must let its actual protocol I/O finish.
struct ReleaseOnDrop(Option<Sender<()>>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

#[test]
fn held_input_and_output_cannot_hold_empty_native_stop_or_its_writer() {
    let directory = Directory::new();
    let mut driver =
        OwnedNativeExecutor::new(Store::open(&directory.0).unwrap(), HandlerTable::default())
            .unwrap();
    let control = driver.control();
    let (input_ready, input_observed) = mpsc::channel();
    let (input_release, input_wait) = mpsc::channel();
    let (output_ready, output_observed) = mpsc::channel();
    let (output_release, output_wait) = mpsc::channel();
    let release_input = ReleaseOnDrop(Some(input_release));
    let release_output = ReleaseOnDrop(Some(output_release));
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let written = bytes.clone();
    let (done, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let report = serve_owned_native_session(
            &mut driver,
            &mut FixedClock::new(0, 1),
            move || HeldReader {
                prefix: Cursor::new(INITIALIZE.to_vec()),
                ready: Some(input_ready),
                release: input_wait,
            },
            HeldWriter {
                bytes: written,
                ready: Some(output_ready),
                release: output_wait,
            },
            1000,
        )
        .unwrap();
        done.send(report).unwrap();
    });
    input_observed.recv_timeout(Duration::from_secs(5)).unwrap();
    output_observed
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let request = control.stop(ShutdownMode::Abort, 250).unwrap();
    let report = request.wait();
    assert_eq!(report.phase, ExecutorPhase::Stopped);
    assert!(report.writer_released && report.helpers_retired && report.inventory_complete);
    assert!(report.unresolved_run_ids.is_empty());
    let session = result.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(session.shutdown.phase, ExecutorPhase::Stopped);
    assert!(!session.output_drained);
    assert!(bytes.lock().unwrap().is_empty());
    drop(Store::open(&directory.0).unwrap());
    worker.join().unwrap();
    drop(release_input);
    drop(release_output);
}

struct ObservationClock {
    calls: usize,
    observed: Option<Sender<()>>,
}
impl Clock for ObservationClock {
    fn now_ms(&mut self) -> i64 {
        self.calls += 1;
        if self.calls == 2 {
            self.observed.take().unwrap().send(()).unwrap();
        }
        10000
    }
}

#[test]
fn quiet_owned_session_observes_without_starting_pending_or_due_deadlines() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    let definition = parse(br#"{
        "format":"fsm.machine/1","name":"quiet","context":[],"events":[],
        "effects":[{"name":"work","fields":[]}],
        "states":[{"name":"waiting","entry":{"emit":[{"effect":"work","args":{}}]}},{"name":"done","terminal":true}],
        "initial":"waiting","transitions":[],
        "deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"done"}]
    }"#, &JsonLimits::DEFAULT).unwrap();
    let mut clock = FixedClock::new(1000, 1);
    store
        .define_machine_on(&mut clock, definition, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "quiet",
            "instance",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    assert_eq!(store.state.instances["instance"].pending.len(), 1);
    assert!(store.state.instances["instance"].deadlines["due"] < 10000);
    let records = store.records.clone();
    let state = store.state.clone();
    let table = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","argv":["/bin/false"],"timeout_ms":1000}]}"#).unwrap();
    let mut driver = OwnedNativeExecutor::new(store, table).unwrap();
    let control = driver.control();
    let (ready, input_ready) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let release = ReleaseOnDrop(Some(release));
    let (observed, observation) = mpsc::channel();
    let (done, result) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let report = serve_owned_native_session(
            &mut driver,
            &mut ObservationClock {
                calls: 0,
                observed: Some(observed),
            },
            move || HeldReader {
                prefix: Cursor::new(Vec::new()),
                ready: Some(ready),
                release: held,
            },
            Cursor::new(Vec::<u8>::new()),
            1000,
        )
        .unwrap();
        done.send(report).unwrap();
    });
    input_ready.recv_timeout(Duration::from_secs(5)).unwrap();
    observation.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        control
            .stop(ShutdownMode::Abort, 1000)
            .unwrap()
            .wait()
            .phase,
        ExecutorPhase::Stopped
    );
    assert!(
        result
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .output_drained
    );
    worker.join().unwrap();
    drop(release);
    let cold = Store::open(&directory.0).unwrap();
    assert_eq!(cold.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&cold.state, &state));
}

struct RecordingWriter(Arc<Mutex<Vec<u8>>>);
impl Write for RecordingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn healthy_eof_delivers_queued_reply_before_returning() {
    let directory = Directory::new();
    let mut driver =
        OwnedNativeExecutor::new(Store::open(&directory.0).unwrap(), HandlerTable::default())
            .unwrap();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let report = serve_owned_native_session(
        &mut driver,
        &mut FixedClock::new(0, 1),
        || Cursor::new(INITIALIZE.to_vec()),
        RecordingWriter(bytes.clone()),
        1000,
    )
    .unwrap();
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.output_drained);
    let bytes = bytes.lock().unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    let reply = parse(&bytes[..bytes.len() - 1], &JsonLimits::DEFAULT).unwrap();
    assert_eq!(reply.get("id"), Some(&Value::Num("1".into())));
    let instructions = reply
        .get("result")
        .unwrap()
        .get("instructions")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(instructions.contains("already admitted effects can finish while you are quiet"));
}

#[test]
fn invalid_owned_session_bounds_preserve_the_driver_and_maximum_is_finite() {
    let directory = Directory::new();
    let mut driver =
        OwnedNativeExecutor::new(Store::open(&directory.0).unwrap(), HandlerTable::default())
            .unwrap();
    let control = driver.control();
    for timeout in [-1, 0, fsm_execute::config::MAX_TIMEOUT_MS + 1, i64::MAX] {
        let Err(error) = serve_owned_native_session(
            &mut driver,
            &mut FixedClock::new(0, 1),
            || -> Cursor<Vec<u8>> { panic!("invalid bounds must not construct input") },
            io::sink(),
            timeout,
        ) else {
            panic!("invalid bounds must refuse")
        };
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(control.report().phase, ExecutorPhase::Running);
        assert!(!control.report().admission_closed);
        assert!(driver.store_mut().is_some());
    }
    let report = serve_owned_native_session(
        &mut driver,
        &mut FixedClock::new(0, 1),
        || Cursor::new(Vec::<u8>::new()),
        io::sink(),
        fsm_execute::config::MAX_TIMEOUT_MS,
    )
    .unwrap();
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.output_drained);
}

#[test]
fn reporting_session_retains_actual_input_failure_and_releases_original_writer() {
    struct FailedInput;
    impl io::Read for FailedInput {
        fn read(&mut self, _bytes: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("actual owned input failure"))
        }
    }
    impl io::BufRead for FailedInput {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("actual owned input failure"))
        }
        fn consume(&mut self, _amount: usize) {}
    }
    let directory = Directory::new();
    let mut driver =
        OwnedNativeExecutor::new(Store::open(&directory.0).unwrap(), HandlerTable::default())
            .unwrap();
    let report = fsm_cli::mcp::serve::serve_owned_native_session_reporting(
        &mut driver,
        &mut FixedClock::new(0, 1),
        || FailedInput,
        io::sink(),
        500,
    )
    .unwrap();
    assert_eq!(
        report.failure.as_ref().unwrap().kind(),
        io::ErrorKind::Other
    );
    assert_eq!(
        report.failure.as_ref().unwrap().to_string(),
        "actual owned input failure"
    );
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.shutdown.admission_closed);
    assert!(report.shutdown.writer_released);
    assert!(report.shutdown.helpers_retired);
    assert!(report.shutdown.inventory_complete);
    assert!(report.shutdown.unresolved_run_ids.is_empty());
    assert!(driver.store_mut().is_none());
    drop(Store::open(&directory.0).unwrap());
}

#[test]
fn reporting_session_reuses_the_actual_first_control_deadline() {
    let directory = Directory::new();
    let mut driver =
        OwnedNativeExecutor::new(Store::open(&directory.0).unwrap(), HandlerTable::default())
            .unwrap();
    let request = driver.control().stop(ShutdownMode::Drain, 500).unwrap();
    let report = fsm_cli::mcp::serve::serve_owned_native_session_reporting(
        &mut driver,
        &mut FixedClock::new(0, 1),
        || Cursor::new(Vec::<u8>::new()),
        io::sink(),
        10000,
    )
    .unwrap();
    assert_eq!(report.shutdown_deadline, request.deadline());
    assert!(report.failure.is_none());
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.output_drained);
    drop(Store::open(&directory.0).unwrap());
}
