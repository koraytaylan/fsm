//! Original diagnostic workers service actual owned-input lifetime controls.

use super::{Scratch, command, seeded, value};
use crate::clock::FixedClock;
use crate::mcp::{
    cancel::CancelFlag,
    host::{AdmissionError, Owner, Session},
    methods::handle_request_hosted,
    notify::{Notifier, SessionIo, SharedSink, pending_input::PendingInput},
    owned_input::OwnedInput,
    serve::Live,
};
use crate::store::Store;
use std::{
    cell::RefCell,
    io::{self, BufReader, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

struct Conversation {
    client: UnixStream,
    output: SharedSink,
    entered: mpsc::Receiver<CancelFlag>,
    release: mpsc::Sender<()>,
    finished: mpsc::Receiver<io::Result<()>>,
    caller: thread::JoinHandle<()>,
}

fn conversation(session: Session, path: std::path::PathBuf) -> Conversation {
    let (entered, observed) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    *session.original.diagnostic_hold.lock().unwrap() = Some((entered, resume));
    let (client, server) = UnixStream::pair().unwrap();
    let output = SharedSink::new();
    let sink = output.clone();
    let (finished, result) = mpsc::channel();
    let caller = thread::spawn(move || {
        let mut input = OwnedInput::start(move || BufReader::new(server), || false).unwrap();
        let (notifier, control) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
        let mut pending = PendingInput::default();
        let reply = {
            let input = RefCell::new(SessionIo::with_owned_wait(
                &notifier,
                &mut input,
                &mut pending,
            ));
            handle_request_hosted(
                &notifier,
                &session,
                &path,
                &mut FixedClock::new(2000, 0),
                &mut true,
                &mut Live::default(),
                value("4"),
                "tools/call",
                Some(value(r#"{"name":"journal_verify","arguments":{}}"#)),
                "owned diagnostic input",
                Some(&input),
                None,
            )
        };
        control.close();
        finished.send(reply).unwrap();
    });
    Conversation {
        client,
        output,
        entered: observed,
        release,
        finished: result,
        caller,
    }
}

fn send(client: &mut UnixStream, line: &str) {
    client.write_all(line.as_bytes()).unwrap();
    client.write_all(b"\n").unwrap();
}

fn observed(output: &SharedSink, id: &str) -> bool {
    let watchdog = Instant::now() + Duration::from_secs(2);
    while Instant::now() < watchdog {
        if output
            .text()
            .lines()
            .map(value)
            .any(|frame| frame.get("id") == Some(&value(id)))
        {
            return true;
        }
        thread::sleep(Duration::from_millis(1));
    }
    false
}

#[test]
fn session_channels_owned_diagnostic_cancel_and_ping_bypass_full_application_capacity() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let mut conversation = conversation(session.clone(), scratch.0.clone());
    let cancellation = conversation
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let mut pending = (0..super::SESSION_COMMANDS - 1)
        .map(|_| session.submit(command("machine_list", "{}")).unwrap())
        .collect::<Vec<_>>();
    let others = (0..3)
        .map(|_| handle.session().unwrap())
        .collect::<Vec<_>>();
    for other in &others {
        for _ in 0..super::SESSION_COMMANDS {
            pending.push(other.submit(command("machine_list", "{}")).unwrap());
        }
    }
    assert_eq!(pending.len() + 1, super::HOST_COMMANDS);
    send(
        &mut conversation.client,
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":4}}"#,
    );
    send(
        &mut conversation.client,
        r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#,
    );
    let ping = observed(&conversation.output, "5");
    let cancelled = cancellation.cancelled();
    let capacity = session.submit(command("machine_list", "{}")).err();
    conversation.release.send(()).unwrap();
    let finished = conversation.finished.recv_timeout(Duration::from_secs(3));
    conversation.client.shutdown(Shutdown::Write).unwrap();
    conversation.caller.join().unwrap();
    handle.stop();
    owner.run();
    assert!(
        ping && cancelled,
        "owned controls did not reach the held diagnostic"
    );
    assert_eq!(capacity, Some(AdmissionError::Busy));
    assert!(finished.unwrap().is_ok());
    let watchdog = Instant::now() + Duration::from_secs(2);
    let response = loop {
        if let Some(frame) = conversation
            .output
            .text()
            .lines()
            .map(value)
            .find(|frame| frame.get("id") == Some(&value("4")))
        {
            break frame;
        }
        assert!(
            Instant::now() < watchdog,
            "cancelled tool outcome was not delivered"
        );
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(
        response
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
            .get("error")
            .unwrap()
            .get("code")
            .and_then(fsm_core::json::Value::as_str),
        Some("req/cancelled")
    );
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
}

#[test]
fn session_channels_owned_diagnostic_eof_retires_before_worker_returns() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 0));
    let session = handle.session().unwrap();
    let conversation = conversation(session.clone(), scratch.0.clone());
    let cancellation = conversation
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    conversation.client.shutdown(Shutdown::Write).unwrap();
    let retired = conversation
        .finished
        .recv_timeout(Duration::from_millis(500));
    let cancelled = cancellation.cancelled();
    let capacity = session.submit(command("machine_list", "{}")).err();
    // Cleanup also releases the actual worker if input retirement is disabled.
    conversation.release.send(()).unwrap();
    conversation.caller.join().unwrap();
    let watchdog = Instant::now() + Duration::from_secs(2);
    while session.cancel(&value("4")) != 0 {
        assert!(
            Instant::now() < watchdog,
            "original worker control did not retire"
        );
        thread::sleep(Duration::from_millis(1));
    }
    handle.stop();
    owner.run();
    assert!(retired.unwrap().is_ok());
    assert!(cancelled);
    assert_eq!(capacity, Some(AdmissionError::Closed));
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}
