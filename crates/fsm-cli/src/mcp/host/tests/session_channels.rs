//! A stalled session output never owns the writer or the native wait clock.

use super::{Scratch, value};
use crate::mcp::{
    host::native::NativeOwner,
    methods::handle_request_hosted,
    notify::{Notifier, SharedSink, diagnostic_output::DiagnosticOutput},
    serve::Live,
};
use crate::{
    clock::{Clock, FixedClock},
    store::Store,
};
use fsm_execute::{config::HandlerTable, service::OwnedNativeExecutor};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

struct LogicalClock(Arc<AtomicI64>);
impl Clock for LogicalClock {
    fn now_ms(&mut self) -> i64 {
        self.0.load(Ordering::Acquire)
    }
}

struct HeldOutput {
    entered: Option<mpsc::Sender<()>>,
    resume: mpsc::Receiver<()>,
}
impl Write for HeldOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.resume
                .recv_timeout(Duration::from_secs(10))
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "stalled output release missing")
                })?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn session_channels_stalled_output_allows_quiet_deadline_and_another_session() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(super::interaction::CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "question",
            "create-question",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let prefix = store.journal.last_seq;
    let logical = Arc::new(AtomicI64::new(1000));
    let (owner, handle) = NativeOwner::new(
        OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap(),
        LogicalClock(Arc::clone(&logical)),
        DiagnosticOutput::start(io::sink()).unwrap(),
        Duration::from_millis(10),
        10000,
    )
    .unwrap();
    let stalled = handle.session().unwrap();
    let healthy = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let (entered, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let (stalled_notifier, stalled_output) = Notifier::hosted_queued(Box::new(HeldOutput {
        entered: Some(entered),
        resume,
    }))
    .unwrap();
    let reply = handle_request_hosted(
        &stalled_notifier,
        &stalled,
        &scratch.0,
        &mut clock,
        &mut true,
        &mut Live::default(),
        value("1"),
        "tools/call",
        Some(value(
            r#"{"name":"instance_get","arguments":{"instance_id":"question"}}"#,
        )),
        "stalled session",
        None,
        None,
    );
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(reply.is_ok());
    assert_eq!(
        Store::open_read_only(&scratch.0).unwrap().journal.last_seq,
        prefix
    );
    logical.store(1001, Ordering::Release);
    // No client request drives the deadline; wall time only bounds native I/O.
    let watchdog = Instant::now() + Duration::from_secs(3);
    while Store::open_read_only(&scratch.0).unwrap().journal.last_seq == prefix {
        assert!(
            Instant::now() < watchdog,
            "stalled session parked autonomous deadline"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let sink = SharedSink::new();
    let (healthy_notifier, healthy_output) =
        Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let healthy_reply = handle_request_hosted(
        &healthy_notifier,
        &healthy,
        &scratch.0,
        &mut clock,
        &mut true,
        &mut Live::default(),
        value("2"),
        "tools/call",
        Some(value(
            r#"{"name":"instance_get","arguments":{"instance_id":"question"}}"#,
        )),
        "healthy session",
        None,
        None,
    );
    handle.stop();
    let exit = worker.join().unwrap();
    let still_blocked = !stalled_output.drained() && !stalled_output.is_broken();
    stalled_output.close();
    release.send(()).unwrap();
    healthy_output.close();
    let watchdog = Instant::now() + Duration::from_secs(3);
    while !healthy_output.drained() || !stalled_output.drained() {
        assert!(Instant::now() < watchdog, "session output failed to retire");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(healthy_reply.is_ok() && still_blocked);
    assert!(exit.failure.is_none() && exit.shutdown.writer_released);
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, prefix + 1);
    assert_eq!(
        reopened.records.last().unwrap().kind,
        fsm_core::record::RecordKind::DeadlineApplied
    );
    let frames: Vec<_> = sink.text().lines().map(value).collect();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].get("id"), Some(&value("2")));
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

struct FailedOutput;
impl Write for FailedOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "original session disconnected",
        ))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn session_channels_reconnect_replays_committed_mutation_after_actual_delivery_failure() {
    let scratch = Scratch::new();
    let store = super::seeded(&scratch.0);
    let prefix = store.journal.last_seq;
    let (owner, handle) = super::Owner::new(store, FixedClock::new(2000, 0));
    let worker = std::thread::spawn(move || owner.run());
    let original = handle.session().unwrap();
    let (notifier, output) = Notifier::hosted_queued(Box::new(FailedOutput)).unwrap();
    let parameters = value(
        r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"lost-response"}}"#,
    );
    // Queue acceptance is not delivery; observe the actual adapter failure.
    let _reply = handle_request_hosted(
        &notifier,
        &original,
        &scratch.0,
        &mut FixedClock::new(2000, 0),
        &mut true,
        &mut Live::default(),
        value("1"),
        "tools/call",
        Some(parameters.clone()),
        "original session",
        None,
        None,
    );
    let watchdog = Instant::now() + Duration::from_secs(3);
    while !output.is_broken() {
        assert!(
            Instant::now() < watchdog,
            "original output did not report delivery failure"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    original.close();
    let committed = Store::open_read_only(&scratch.0).unwrap().journal.last_seq;
    let reconnect = handle.session().unwrap();
    let sink = SharedSink::new();
    let (replacement, replacement_output) =
        Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let replay = handle_request_hosted(
        &replacement,
        &reconnect,
        &scratch.0,
        &mut FixedClock::new(9999, 0),
        &mut true,
        &mut Live::default(),
        value("2"),
        "tools/call",
        Some(parameters),
        "reconnected session",
        None,
        None,
    );
    handle.stop();
    worker.join().unwrap();
    replacement_output.close();
    let watchdog = Instant::now() + Duration::from_secs(3);
    while !replacement_output.drained() {
        assert!(
            Instant::now() < watchdog,
            "replacement response was not delivered"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(replay.is_ok());
    assert_eq!(committed, prefix + 1);
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, committed);
    assert_eq!(reopened.state.instances.len(), 1);
    assert!(reopened.state.instances.contains_key("inst-lost-response"));
    let frames: Vec<_> = sink.text().lines().map(value).collect();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].get("id"), Some(&value("2")));
    assert_eq!(
        frames[0]
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
            .get("duplicate"),
        Some(&fsm_core::json::Value::Bool(true))
    );
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

struct HeldDiagnosticClock {
    entered: Option<mpsc::Sender<()>>,
    resume: mpsc::Receiver<()>,
}
impl Clock for HeldDiagnosticClock {
    fn now_ms(&mut self) -> i64 {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        2000
    }
}

#[test]
fn session_channels_long_diagnostic_wait_never_parks_the_writer_owner() {
    let scratch = Scratch::new();
    let store = super::seeded(&scratch.0);
    let prefix = store.journal.last_seq;
    let (owner, handle) = super::Owner::new(store, FixedClock::new(2000, 0));
    let diagnostic = handle.session().unwrap();
    let healthy = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let (entered, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let path = scratch.0.clone();
    let caller = std::thread::spawn(move || {
        handle_request_hosted(
            &notifier,
            &diagnostic,
            &path,
            &mut HeldDiagnosticClock {
                entered: Some(entered),
                resume,
            },
            &mut true,
            &mut Live::default(),
            value(r#""diagnostic""#),
            "tools/call",
            Some(value(
                r#"{"name":"journal_verify","arguments":{},"_meta":{"progressToken":"diagnostic"}}"#,
            )),
            "diagnostic session",
            None,
            None,
        )
    });
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    let reply = healthy
        .submit(super::command("machine_list", "{}"))
        .unwrap()
        .recv_timeout(Duration::from_millis(500));
    release.send(()).unwrap();
    let diagnostic_reply = caller.join().unwrap();
    handle.stop();
    worker.join().unwrap();
    output.close();
    let watchdog = Instant::now() + Duration::from_secs(3);
    while !output.drained() {
        assert!(Instant::now() < watchdog);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        reply
            .expect("diagnostic held the writer owner")
            .result
            .is_ok()
    );
    assert!(diagnostic_reply.is_ok());
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, prefix);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

struct FailedDiagnosticClock;
impl Clock for FailedDiagnosticClock {
    fn now_ms(&mut self) -> i64 {
        panic!("injected adapter clock failure")
    }
}

#[test]
fn session_channels_diagnostic_cancel_at_full_host_capacity_releases_one_slot() {
    let scratch = Scratch::new();
    let store = super::seeded(&scratch.0);
    let prefix = store.journal.last_seq;
    let (owner, handle) = super::Owner::new(store, FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let others = (0..3)
        .map(|_| handle.session().unwrap())
        .collect::<Vec<_>>();
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let (entered, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let path = scratch.0.clone();
    let diagnostic = session.clone();
    let caller = std::thread::spawn(move || {
        handle_request_hosted(
            &notifier,
            &diagnostic,
            &path,
            &mut HeldDiagnosticClock {
                entered: Some(entered),
                resume,
            },
            &mut true,
            &mut Live::default(),
            value(r#""diagnostic""#),
            "tools/call",
            Some(value(
                r#"{"name":"simulate","arguments":{"machine":"owner_case","events":[{"name":"finish","payload":{}},{"name":"finish","payload":{}}],"on_reject":"continue"}}"#,
            )),
            "diagnostic session",
            None,
            None,
        )
    });
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    // Keep the owner idle: all 31 ordinary commands remain admitted while
    // the original diagnostic retains the thirty-second application slot.
    let mut pending = (0..super::SESSION_COMMANDS - 1)
        .map(|_| {
            session
                .submit(super::command("machine_list", "{}"))
                .unwrap()
        })
        .collect::<Vec<_>>();
    for other in &others {
        for _ in 0..super::SESSION_COMMANDS {
            pending.push(other.submit(super::command("machine_list", "{}")).unwrap());
        }
    }
    assert_eq!(pending.len() + 1, super::HOST_COMMANDS);
    let extra = handle.session().unwrap();
    assert_eq!(
        extra.submit(super::command("machine_list", "{}")).err(),
        Some(super::AdmissionError::Busy)
    );
    assert_eq!(extra.cancel(&value(r#""diagnostic""#)), 0);
    assert_eq!(session.cancel(&value(r#""diagnostic""#)), 1);
    assert_eq!(
        extra.submit(super::command("machine_list", "{}")).err(),
        Some(super::AdmissionError::Busy),
        "cancellation cannot release a worker that has not returned"
    );
    release.send(()).unwrap();
    assert!(caller.join().unwrap().is_ok());
    assert_eq!(session.cancel(&value(r#""diagnostic""#)), 0);
    pending.push(extra.submit(super::command("machine_list", "{}")).unwrap());
    assert_eq!(
        extra.submit(super::command("machine_list", "{}")).err(),
        Some(super::AdmissionError::Busy),
        "worker retirement must release exactly one host slot"
    );
    handle.stop();
    owner.run();
    output.close();
    let watchdog = Instant::now() + Duration::from_secs(3);
    while !output.drained() {
        assert!(Instant::now() < watchdog);
        std::thread::sleep(Duration::from_millis(1));
    }
    let frames = sink.text().lines().map(value).collect::<Vec<_>>();
    assert_eq!(frames.len(), 1);
    let result = frames[0].get("result").unwrap();
    assert_eq!(
        result.get("isError"),
        Some(&fsm_core::json::Value::Bool(true))
    );
    assert_eq!(
        result
            .get("structuredContent")
            .unwrap()
            .get("error")
            .unwrap()
            .get("code")
            .and_then(fsm_core::json::Value::as_str),
        Some("req/cancelled")
    );
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, prefix);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn session_channels_diagnostic_adapter_unwind_cancels_original_controls() {
    use crate::mcp::host::operation::HostedToolContext;
    let scratch = Scratch::new();
    let store = super::seeded(&scratch.0);
    let prefix = store.journal.last_seq;
    let (owner, handle) = super::Owner::new(store, FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let other = handle.session().unwrap();
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let rpc_id = value(r#""diagnostic""#);
    // Original controls for the same session/request share cancellation;
    // retaining one makes adapter unwinding observable without a worker race.
    let reserve = |session: &crate::mcp::host::Session| {
        session
            .reserve_diagnostic(
                crate::mcp::host::Command {
                    rpc_id: rpc_id.clone(),
                    tool: "journal_verify".into(),
                    arguments: value("{}"),
                },
                HostedToolContext::new(None, &rpc_id, &notifier).unwrap(),
            )
            .unwrap()
    };
    let original = reserve(&session);
    let unrelated = reserve(&other);
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handle_request_hosted(
            &notifier,
            &session,
            &scratch.0,
            &mut FailedDiagnosticClock,
            &mut true,
            &mut Live::default(),
            rpc_id.clone(),
            "tools/call",
            Some(value(r#"{"name":"journal_verify","arguments":{}}"#)),
            "diagnostic session",
            None,
            None,
        )
    }));
    assert!(unwind.is_err());
    assert!(original.cancel.cancelled());
    assert!(!unrelated.cancel.cancelled());
    // The detached worker must release its own reservation, while this
    // original control still occupies exactly one ordinary application slot.
    let mut pending = Vec::new();
    let watchdog = Instant::now() + Duration::from_secs(3);
    while pending.len() < super::SESSION_COMMANDS - 1 {
        match session.submit(super::command("machine_list", "{}")) {
            Ok(reply) => pending.push(reply),
            Err(super::AdmissionError::Busy) => {
                assert!(
                    Instant::now() < watchdog,
                    "diagnostic worker did not retire"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("unexpected admission failure: {error:?}"),
        }
    }
    assert_eq!(
        session.submit(super::command("machine_list", "{}")).err(),
        Some(super::AdmissionError::Busy)
    );
    drop(original);
    pending.push(
        session
            .submit(super::command("machine_list", "{}"))
            .unwrap(),
    );
    drop(unrelated);
    handle.stop();
    owner.run();
    output.close();
    let watchdog = Instant::now() + Duration::from_secs(3);
    while !output.drained() {
        assert!(Instant::now() < watchdog);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, prefix);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}
