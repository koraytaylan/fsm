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
