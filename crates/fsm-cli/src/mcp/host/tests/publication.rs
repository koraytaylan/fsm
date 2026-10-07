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
