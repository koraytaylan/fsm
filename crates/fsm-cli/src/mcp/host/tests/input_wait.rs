//! Actual owned byte input services controls before a held owner can dispatch.

use super::super::{Owner, mailbox::Next};
use super::{Scratch, seeded, value};
use crate::mcp::{
    methods::handle_request_hosted,
    notify::{Notifier, SessionIo, SharedSink, pending_input::PendingInput},
    owned_input::OwnedInput,
    serve::Live,
};
use crate::{clock::FixedClock, store::Store};
use std::{
    cell::RefCell,
    io::{self, BufReader, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const REQUEST: &str = r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"wait-input-create"}}"#;

type ConversationResult = (io::Result<()>, Option<String>);
type Conversation = (
    UnixStream,
    SharedSink,
    mpsc::Receiver<ConversationResult>,
    thread::JoinHandle<()>,
);

fn send(client: &mut UnixStream, line: &str) {
    client.write_all(line.as_bytes()).unwrap();
    client.write_all(b"\n").unwrap();
}

fn ping_observed(output: &SharedSink) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if output
            .text()
            .lines()
            .map(value)
            .any(|frame| frame.get("id") == Some(&value("5")))
        {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    false
}

fn conversation(session: super::super::Session, path: std::path::PathBuf) -> Conversation {
    let (client, server) = UnixStream::pair().unwrap();
    let output = SharedSink::new();
    let sink = output.clone();
    let (finished, result) = mpsc::channel();
    let caller = thread::spawn(move || {
        let mut input = OwnedInput::start(move || BufReader::new(server), || false).unwrap();
        let (notifier, control) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
        let mut pending = PendingInput::default();
        let reply = {
            let io = RefCell::new(SessionIo::with_owned_wait(
                &notifier,
                &mut input,
                &mut pending,
            ));
            handle_request_hosted(
                &notifier,
                &session,
                &path,
                &mut FixedClock::new(9999, 0),
                &mut true,
                &mut Live::default(),
                value("4"),
                "tools/call",
                Some(value(REQUEST)),
                "held owner input",
                Some(&io),
                None,
            )
        };
        control.close();
        finished.send((reply, pending.take())).unwrap();
    });
    (client, output, result, caller)
}

#[test]
fn execution_host_owned_response_wait_eof_retires_before_owner_turn() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let (client, _, result, caller) = conversation(session, scratch.0.clone());
    let Next::Command(admitted) = owner
        .mailbox
        .next_until(Instant::now() + Duration::from_secs(5))
    else {
        panic!("request admission missing")
    };
    client.shutdown(Shutdown::Write).unwrap();
    let retired = result.recv_timeout(Duration::from_millis(500));
    let writer_held = Store::open(&scratch.0).is_err();
    // Cleanup also releases a response wait whose input guard was neutralized.
    drop(admitted);
    handle.stop();
    owner.run();
    caller.join().unwrap();
    assert!(retired.unwrap().0.is_ok());
    assert!(writer_held);
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 1);
    assert!(!reopened.state.dedup.contains_key("wait-input-create"));
}

#[test]
fn execution_host_owned_response_wait_client_cancel_and_ping_precede_owner_dispatch() {
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let (mut client, output, result, caller) = conversation(session, scratch.0.clone());
    let Next::Command(admitted) = owner
        .mailbox
        .next_until(Instant::now() + Duration::from_secs(5))
    else {
        panic!("request admission missing")
    };
    send(
        &mut client,
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":4}}"#,
    );
    send(&mut client, r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#);
    let ping = ping_observed(&output);
    let cancelled = admitted.cancel.cancelled();
    owner.apply(admitted);
    client.shutdown(Shutdown::Write).unwrap();
    let finished = result.recv_timeout(Duration::from_secs(5));
    handle.stop();
    owner.run();
    caller.join().unwrap();
    assert!(ping && cancelled);
    assert!(finished.unwrap().0.is_ok());
    assert!(
        !output
            .text()
            .lines()
            .map(value)
            .any(|frame| frame.get("id") == Some(&value("4")))
    );
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 1);
    assert!(!reopened.state.dedup.contains_key("wait-input-create"));
}

#[test]
fn execution_host_owned_response_wait_suppresses_known_deferred_cancel_without_future_control() {
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let (mut client, output, result, caller) = conversation(session.clone(), scratch.0.clone());
    let Next::Command(admitted) = owner
        .mailbox
        .next_until(Instant::now() + Duration::from_secs(5))
    else {
        panic!("request admission missing")
    };
    send(
        &mut client,
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"machine_list","arguments":{}}}"#,
    );
    send(
        &mut client,
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":6}}"#,
    );
    send(&mut client, r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#);
    let ping = ping_observed(&output);
    let no_future_control = session.cancel(&value("6")) == 0;
    owner.apply(admitted);
    let finished = result.recv_timeout(Duration::from_secs(5));
    client.shutdown(Shutdown::Write).unwrap();
    handle.stop();
    owner.run();
    caller.join().unwrap();
    let (reply, deferred) = finished.unwrap();
    assert!(ping && no_future_control && reply.is_ok());
    assert!(deferred.is_none());
    assert!(
        !output
            .text()
            .lines()
            .map(value)
            .any(|frame| frame.get("id") == Some(&value("6")))
    );
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}

#[test]
fn execution_host_owned_response_wait_refuses_ninth_deferred_frame_and_preserves_first() {
    let scratch = Scratch::new();
    let (mut owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let (mut client, output, result, caller) = conversation(session, scratch.0.clone());
    let Next::Command(admitted) = owner
        .mailbox
        .next_until(Instant::now() + Duration::from_secs(5))
    else {
        panic!("request admission missing")
    };
    for id in 10..=18 {
        send(
            &mut client,
            &format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/list"}}"#),
        );
    }
    send(&mut client, r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#);
    let ping = ping_observed(&output);
    let frames: Vec<_> = output.text().lines().map(value).collect();
    owner.apply(admitted);
    let finished = result.recv_timeout(Duration::from_secs(5));
    client.shutdown(Shutdown::Write).unwrap();
    handle.stop();
    owner.run();
    caller.join().unwrap();
    let (reply, deferred) = finished.unwrap();
    assert!(ping && reply.is_ok());
    assert_eq!(value(&deferred.unwrap()).get("id"), Some(&value("10")));
    let refusals: Vec<_> = frames
        .iter()
        .filter(|frame| frame.get("error").is_some())
        .collect();
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].get("id"), Some(&value("18")));
    assert_eq!(
        refusals[0].get("error").unwrap().get("code"),
        Some(&value("-32004"))
    );
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}
