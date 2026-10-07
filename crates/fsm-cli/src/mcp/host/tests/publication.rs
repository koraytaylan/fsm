//! Actual hosted mutation response admission precedes its change-feed output.
use super::{Scratch, seeded, value};
use crate::mcp::{
    host::Owner,
    methods::handle_request_hosted,
    notify::{Notifier, SessionIo, SharedSink},
    serve::Live,
    subscribe::Subscriptions,
    watch::Feed,
};
use crate::{clock::FixedClock, store::Store};
use std::{
    cell::RefCell,
    io::Cursor,
    time::{Duration, Instant},
};

#[test]
fn execution_host_session_channels_response_scope_defers_feed_without_advancing_watermark() {
    let scratch = Scratch::new();
    let store = seeded(&scratch.0);
    let before = store.journal.last_seq;
    let (owner, handle) = Owner::new(store, FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/inst-publication-create");
    let mut feed = Feed::new(&scratch.0, watched, notifier.clone_handle(), before);
    let (reply, deferred, watermark, pending, committed) = {
        let mut input = Cursor::new(Vec::<u8>::new());
        let io = RefCell::new(SessionIo::new(&notifier, &mut input));
        let reply = handle_request_hosted(
            &notifier,
            &session,
            &scratch.0,
            &mut FixedClock::new(2000, 0),
            &mut true,
            &mut Live::default(),
            value("4"),
            "tools/call",
            Some(value(
                r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"publication-create"}}"#,
            )),
            "publication case",
            Some(&io),
            None,
        );
        let committed = Store::open_read_only(&scratch.0).unwrap().journal.last_seq;
        (
            reply,
            feed.poll_once(),
            feed.watermark(),
            notifier.publication_pending(),
            committed,
        )
    };
    let released = !notifier.publication_pending();
    let published = feed.poll_once();
    handle.stop();
    worker.join().unwrap();
    output.close();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !output.drained() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(reply.is_ok() && pending && released);
    assert_eq!(committed, before + 1);
    assert_eq!(deferred, 0);
    assert_eq!(watermark, before);
    assert_eq!(published, 2);
    assert!(output.drained());
    let frames: Vec<_> = sink.text().lines().map(value).collect();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0].get("id"), Some(&value("4")));
    assert_eq!(
        frames[1]
            .get("method")
            .and_then(fsm_core::json::Value::as_str),
        Some("notifications/resources/updated")
    );
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, committed);
}

#[cfg(target_os = "linux")]
#[test]
fn execution_host_session_channels_native_pass_holds_feed_until_deadline_commit_returns() {
    use crate::mcp::{host::native::NativeOwner, notify::diagnostic_output::DiagnosticOutput};
    use fsm_execute::{config::HandlerTable, service::OwnedNativeExecutor};
    use std::{collections::BTreeMap, sync::mpsc};
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
            "publication-native",
            "publication-native",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    let driver = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let mut watched = Subscriptions::default();
    watched.subscribe("fsm://instance/publication-native");
    let mut feed = Feed::new(&scratch.0, watched, notifier.clone_handle(), before);
    let (entered, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let (owner, handle) = NativeOwner::new(
        driver,
        FixedClock::new(1001, 0),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(50),
        10000,
    )
    .unwrap();
    let mut owner = owner.with_publication(&notifier);
    let worker = std::thread::spawn(move || {
        let lines = owner.decision_pass_after_commit(move || {
            entered.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        (owner, lines)
    });
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    let guarded = notifier.publication_pending();
    let deferred = feed.poll_once();
    let committed_before_publication =
        Store::open_read_only(&scratch.0).unwrap().journal.last_seq == before + 1;
    release.send(()).unwrap();
    let (owner, lines) = worker.join().unwrap();
    let released = !notifier.publication_pending();
    let published = feed.poll_once();
    handle.stop();
    let exit = owner.run();
    output.close();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !output.drained() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(guarded && released && committed_before_publication);
    assert_eq!(deferred, 0);
    assert_eq!(published, 1);
    assert!(exit.shutdown.writer_released && output.drained());
    assert!(
        !lines.iter().any(|line| line.starts_with("error ")),
        "{lines:?}"
    );
    assert_eq!(
        Store::open(&scratch.0).unwrap().journal.last_seq,
        before + 1
    );
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[cfg(target_os = "linux")]
#[test]
fn execution_host_session_channels_elicitation_feed_runs_before_answer_and_defers_settlement() {
    use crate::mcp::{host::mailbox::Next, owned_input::OwnedInput};
    use fsm_core::{canon::canon_bytes, json::Value};
    use std::{
        collections::BTreeMap,
        io::{BufReader, Write},
        os::unix::net::UnixStream,
        sync::mpsc,
    };
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
            "inst-question",
            "question-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    let (mut owner, handle) = Owner::new(store, FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let path = scratch.0.clone();
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let adapter_output = notifier.clone_handle();
    let mut subscriptions = Subscriptions::default();
    subscriptions.subscribe("fsm://instance/inst-question");
    let mut feed = Feed::new(&scratch.0, subscriptions, notifier.clone_handle(), before);
    let (mut client, server) = UnixStream::pair().unwrap();
    let (response_queued, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let caller = std::thread::spawn(move || {
        let mut input = OwnedInput::start(move || BufReader::new(server), || false).unwrap();
        let mut pending = crate::mcp::notify::pending_input::PendingInput::default();
        {
            let io = RefCell::new(SessionIo::with_owned_wait(
                &adapter_output,
                &mut input,
                &mut pending,
            ));
            let mut live = Live::default();
            live.client_elicitation = true;
            let reply = handle_request_hosted(
                &adapter_output,
                &session,
                &path,
                &mut FixedClock::new(2000, 0),
                &mut true,
                &mut live,
                value("4"),
                "tools/call",
                Some(value(
                    r#"{"name":"instance_elicit","arguments":{"instance_id":"inst-question","event":"decide","request_id":"question-publication"}}"#,
                )),
                "elicitation publication case",
                Some(&io),
                None,
            );
            response_queued.send(()).unwrap();
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            // The original request's I/O scope survives real response admission.
            reply
        }
    });
    let Next::Command(prepared) = owner
        .mailbox
        .next_until(Instant::now() + Duration::from_secs(5))
    else {
        panic!("original preparation missing")
    };
    owner.apply(prepared);
    let deadline = Instant::now() + Duration::from_secs(5);
    let question =
        loop {
            if let Some(question) = sink.text().lines().map(value).find(|frame| {
                frame.get("method").and_then(Value::as_str) == Some("elicitation/create")
            }) {
                break question;
            }
            assert!(Instant::now() < deadline, "original question missing");
            std::thread::sleep(Duration::from_millis(1));
        };
    let waiting_unguarded = !notifier.publication_pending();
    owner
        .store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "other",
            "other-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let waiting_published = feed.poll_once();
    let waiting_watermark = feed.watermark();
    let answer = Value::Obj(BTreeMap::from([
        ("jsonrpc".into(), Value::Str("2.0".into())),
        ("id".into(), question.get("id").unwrap().clone()),
        (
            "result".into(),
            value(r#"{"action":"accept","content":{"score":7}}"#),
        ),
    ]));
    client.write_all(&canon_bytes(&answer)).unwrap();
    client.write_all(b"\n").unwrap();
    let Next::Command(settlement) = owner
        .mailbox
        .next_until(Instant::now() + Duration::from_secs(5))
    else {
        panic!("original settlement missing")
    };
    let settling_guarded = notifier.publication_pending();
    owner.apply(settlement);
    observed.recv_timeout(Duration::from_secs(5)).unwrap();
    let committed = Store::open_read_only(&scratch.0).unwrap().journal.last_seq;
    let deferred = feed.poll_once();
    let held_watermark = feed.watermark();
    release.send(()).unwrap();
    let reply = caller.join().unwrap();
    let released = !notifier.publication_pending();
    let published = feed.poll_once();
    handle.stop();
    owner.run();
    output.close();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !output.drained() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(waiting_unguarded && settling_guarded && released && reply.is_ok());
    assert_eq!(waiting_published, 1);
    assert_eq!(waiting_watermark, before + 1);
    assert_eq!(committed, before + 2);
    assert_eq!(deferred, 0);
    assert_eq!(held_watermark, waiting_watermark);
    // SPEC: EventApplied changes the subscribed instance, not listing membership.
    assert_eq!(published, 1);
    assert!(output.drained());
    let frames: Vec<_> = sink.text().lines().map(value).collect();
    let response = frames
        .iter()
        .position(|frame| frame.get("id") == Some(&value("4")))
        .unwrap();
    let update = frames
        .iter()
        .position(|frame| {
            frame.get("method").and_then(Value::as_str) == Some("notifications/resources/updated")
        })
        .unwrap();
    assert!(response < update);
    assert_ne!(
        frames[response]
            .get("result")
            .and_then(|result| result.get("isError")),
        Some(&Value::Bool(true))
    );
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, committed);
    assert!(reopened.state.dedup.contains_key("question-publication"));
    assert_eq!(
        reopened
            .instance_view("inst-question", None, None)
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("completed")
    );
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}
