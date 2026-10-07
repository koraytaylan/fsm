//! The real shared method handler drives owned commands without a writer borrow.

use super::{Scratch, command, seeded, value};
use crate::{
    clock::FixedClock,
    mcp::{
        host::Owner,
        methods::handle_request_hosted,
        notify::{Notifier, SharedSink},
        serve::Live,
    },
    store::Store,
};
use fsm_core::json::Value;

#[test]
fn execution_host_shared_protocol_output_backpressure_does_not_report_admission_busy() {
    use std::{
        io::{self, Write},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    struct RefusingOutput(Arc<AtomicUsize>);
    impl Write for RefusingOutput {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0.fetch_add(1, Ordering::Relaxed) == 0 {
                Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "output queue full",
                ))
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let session = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let attempts = Arc::new(AtomicUsize::new(0));
    let output = Notifier::new(Box::new(RefusingOutput(Arc::clone(&attempts))));
    let error = handle_request_hosted(&output, &session, &scratch.0, &mut FixedClock::new(9999, 0), &mut true, &mut Live::default(), Value::Str("committed-rpc".into()), "tools/call", Some(value(r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"committed-output"}}"#)), "host integration", None, None).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(
        attempts.load(Ordering::Relaxed),
        1,
        "do not write a false busy response after committing"
    );
    handle.stop();
    worker.join().unwrap();
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 2);
    assert!(
        reopened
            .state
            .instances
            .contains_key("inst-committed-output")
    );
}

#[test]
fn execution_host_shared_protocol_closed_admission_ends_the_original_session() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let session = handle.session().unwrap();
    handle.stop();
    let buffer = SharedSink::new();
    let output = Notifier::new(Box::new(buffer.writer()));
    let error = handle_request_hosted(
        &output,
        &session,
        &scratch.0,
        &mut FixedClock::new(9999, 0),
        &mut true,
        &mut Live::default(),
        Value::Null,
        "tools/call",
        Some(value(r#"{"name":"machine_list","arguments":{}}"#)),
        "host integration",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    assert!(buffer.bytes().is_empty());
    owner.run();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 1);
}

#[test]
fn execution_host_shared_protocol_suppresses_cancelled_request_and_reuses_key() {
    use std::time::{Duration, Instant};
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let session = handle.session().unwrap();
    let original = session.clone();
    let data_dir = scratch.0.clone();
    let buffer = SharedSink::new();
    let output = Notifier::new(Box::new(buffer.writer()));
    let waiting_output = output.clone_handle();
    let caller = std::thread::spawn(move || {
        handle_request_hosted(
            &waiting_output,
            &original,
            &data_dir,
            &mut FixedClock::new(9999, 0),
            &mut true,
            &mut Live::default(),
            Value::Str("cancelled-rpc".into()),
            "tools/call",
            Some(value(
                r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"cancelled-protocol"}}"#,
            )),
            "host integration",
            None,
            None,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.cancel(&Value::Str("cancelled-rpc".into())) == 0 {
        assert!(
            Instant::now() < deadline,
            "the actual protocol command must reach admission"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let worker = std::thread::spawn(move || owner.run());
    caller.join().unwrap().unwrap();
    assert!(buffer.bytes().is_empty());
    assert_eq!(
        Store::open_read_only(&scratch.0).unwrap().journal.last_seq,
        1
    );
    handle_request_hosted(&output, &session, &scratch.0, &mut FixedClock::new(9999, 0), &mut true, &mut Live::default(), Value::Str("cancelled-rpc".into()), "tools/call", Some(value(r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"cancelled-protocol"}}"#)), "host integration", None, None).unwrap();
    let response = value(buffer.text().trim());
    assert_eq!(
        response
            .get("result")
            .unwrap()
            .get("structuredContent")
            .unwrap()
            .get("instance_id")
            .and_then(Value::as_str),
        Some("inst-cancelled-protocol")
    );
    handle.stop();
    worker.join().unwrap();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}

#[cfg(target_os = "linux")]
#[test]
fn execution_host_shared_protocol_creation_advances_while_the_client_is_quiet() {
    use crate::mcp::notify::diagnostic_output::DiagnosticOutput;
    use fsm_execute::{
        config::HandlerTable,
        service::{ExecutorPhase, OwnedNativeExecutor},
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicI64, Ordering},
        },
        time::{Duration, Instant},
    };

    struct LogicalClock(Arc<AtomicI64>);
    impl crate::clock::Clock for LogicalClock {
        fn now_ms(&mut self) -> i64 {
            self.0.load(Ordering::Acquire)
        }
    }

    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    store.define_machine_on(&mut FixedClock::new(1000, 0), value(r#"{"format":"fsm.machine/1","name":"protocol_deadline","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"done"}]}"#), false, false).unwrap();
    let logical_time = Arc::new(AtomicI64::new(1000));
    let driver = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
    let (owner, handle) = super::super::native::NativeOwner::new(
        driver,
        LogicalClock(Arc::clone(&logical_time)),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(10),
        10000,
    )
    .unwrap();
    let session = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let buffer = SharedSink::new();
    let output = Notifier::new(Box::new(buffer.writer()));
    let mut initialized = false;
    let mut live = Live::default();
    let mut adapter_clock = FixedClock::new(9999, 0);
    handle_request_hosted(
        &output,
        &session,
        &scratch.0,
        &mut adapter_clock,
        &mut initialized,
        &mut live,
        Value::Num("1".into()),
        "initialize",
        None,
        "host integration",
        None,
        None,
    )
    .unwrap();
    handle_request_hosted(&output, &session, &scratch.0, &mut adapter_clock, &mut initialized, &mut live, Value::Num("2".into()), "tools/call", Some(value(r#"{"name":"instance_create","arguments":{"machine":"protocol_deadline","request_id":"quiet-protocol"}}"#)), "host integration", None, None).unwrap();
    let frames: Vec<Value> = buffer.text().lines().map(value).collect();
    assert_eq!(frames.len(), 2);
    assert_eq!(
        frames[1]
            .get("result")
            .unwrap()
            .get("structuredContent")
            .and_then(|result| result.get("instance_id"))
            .and_then(Value::as_str),
        Some("inst-quiet-protocol")
    );
    logical_time.store(1001, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let observed = Store::open_read_only(&scratch.0).unwrap();
        if observed
            .instance_view("inst-quiet-protocol", None, None)
            .unwrap()
            .get("status")
            .and_then(Value::as_str)
            == Some("completed")
        {
            assert_eq!(observed.journal.last_seq, 3);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the quiet client must not pause execution"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        buffer.text().lines().count(),
        2,
        "no polling request or extra reply drives progress"
    );
    handle.stop();
    let exit = worker.join().unwrap();
    assert_eq!(exit.shutdown.phase, ExecutorPhase::Stopped);
    assert!(exit.shutdown.writer_released && exit.shutdown.inventory_complete);
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 3);
}

#[test]
fn execution_host_shared_protocol_handler_creates_and_resolves_a_workflow() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let session = handle.session().unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let buffer = SharedSink::new();
    let output = Notifier::new(Box::new(buffer.writer()));
    let mut initialized = false;
    let mut live = Live::default();
    let mut clock = FixedClock::new(9999, 0);
    for (id, method, parameters) in [
        (1, "initialize", r#"{"protocolVersion":"2025-06-18"}"#),
        (
            2,
            "tools/call",
            r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"protocol-workflow"}}"#,
        ),
        (
            3,
            "resources/read",
            r#"{"uri":"fsm://instance/inst-protocol-workflow"}"#,
        ),
        (
            4,
            "completion/complete",
            r#"{"ref":{"type":"ref/resource","uri":"fsm://instance/{id}"},"argument":{"name":"id","value":"inst-protocol"}}"#,
        ),
    ] {
        handle_request_hosted(
            &output,
            &session,
            &scratch.0,
            &mut clock,
            &mut initialized,
            &mut live,
            Value::Num(id.to_string()),
            method,
            Some(value(parameters)),
            "host integration",
            None,
            None,
        )
        .unwrap();
    }
    let frames: Vec<Value> = buffer.text().lines().map(value).collect();
    assert_eq!(frames.len(), 4);
    assert!(initialized);
    assert_eq!(
        frames[1]
            .get("result")
            .unwrap()
            .get("structuredContent")
            .and_then(|result| result.get("instance_id"))
            .and_then(Value::as_str),
        Some("inst-protocol-workflow")
    );
    let resource = frames[2]
        .get("result")
        .unwrap()
        .get("contents")
        .and_then(Value::as_arr)
        .unwrap();
    let report = value(resource[0].get("text").and_then(Value::as_str).unwrap());
    assert_eq!(
        report.get("instance_id").and_then(Value::as_str),
        Some("inst-protocol-workflow")
    );
    assert_eq!(
        frames[3]
            .get("result")
            .unwrap()
            .get("completion")
            .unwrap()
            .get("values")
            .and_then(Value::as_arr)
            .unwrap(),
        &[Value::Str("inst-protocol-workflow".into())]
    );
    assert!(Store::open(&scratch.0).is_err());
    handle.stop();
    worker.join().unwrap();
    assert_eq!(Store::open(&scratch.0).unwrap().journal.last_seq, 2);
}

#[test]
fn execution_host_shared_protocol_busy_response_does_not_dispatch_a_mutation() {
    let scratch = Scratch::new();
    let (owner, handle) = Owner::new(seeded(&scratch.0), FixedClock::new(2000, 1));
    let session = handle.session().unwrap();
    let queued: Vec<_> = (0..super::SESSION_COMMANDS)
        .map(|_| session.submit(command("machine_list", "{}")).unwrap())
        .collect();
    let buffer = SharedSink::new();
    let output = Notifier::new(Box::new(buffer.writer()));
    let mut initialized = true;
    handle_request_hosted(&output, &session, &scratch.0, &mut FixedClock::new(9999, 0), &mut initialized, &mut Live::default(), Value::Str("busy-rpc".into()), "tools/call", Some(value(r#"{"name":"instance_create","arguments":{"machine":"owner_case","request_id":"must-not-create"}}"#)), "host integration", None, None).unwrap();
    let reply = value(buffer.text().trim());
    assert_eq!(reply.get("id"), Some(&Value::Str("busy-rpc".into())));
    assert_eq!(
        reply.get("error").unwrap().get("code"),
        Some(&Value::Num("-32004".into()))
    );
    handle.stop();
    owner.run();
    assert!(queued.into_iter().all(|receiver| receiver.recv().is_err()));
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 1);
    assert!(reopened.state.instances.is_empty());
}
