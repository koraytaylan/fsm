//! Real held child/stream observations are not native domain closure evidence.

use super::*;
use crate::run::native_client::{NativeRequest, RESPONSE_LIMIT, Reader};
use fsm_core::{
    canon::canon_bytes,
    json::{JsonLimits, parse},
};
use std::{
    collections::BTreeMap,
    io::Write,
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Command, Stdio},
    sync::mpsc,
    time::Instant,
};

fn held_transport() -> (InlineRequest, UnixStream, UnixStream) {
    let (gate, input) = UnixStream::pair().unwrap();
    let (stream, response) = UnixStream::pair().unwrap();
    stream.set_nonblocking(true).unwrap();
    let (stderr, diagnostics) = Reader::open(4096).unwrap();
    let mut command = Command::new("sh");
    command
        .args(["-c", "read signal"])
        .stdin(Stdio::from(OwnedFd::from(input)))
        .stdout(Stdio::from(OwnedFd::from(response.try_clone().unwrap())))
        .stderr(diagnostics);
    let child = command.spawn().unwrap();
    drop(command);
    (
        InlineRequest {
            child,
            input: None,
            pending: Vec::new(),
            written: 0,
            stdout: Reader {
                stream,
                bytes: Vec::with_capacity(RESPONSE_LIMIT + 1),
                limit: RESPONSE_LIMIT + 1,
                eof: false,
            },
            stderr,
            status: None,
            deadline: Instant::now() + Duration::from_secs(10),
            error: None,
            collected: false,
        },
        gate,
        response,
    )
}

fn frame(response: &mut UnixStream, value: &Value) {
    let body = canon_bytes(value);
    assert!(body.len() + 4 <= RESPONSE_LIMIT);
    response
        .write_all(&(body.len() as u32).to_be_bytes())
        .unwrap();
    response.write_all(&body).unwrap();
}

fn response() -> Value {
    parse(
        br#"{"format":"fsm.native-response/1","ok":true,"result":null}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn receive(request: &mut NativeRequest) -> Result<Value, String> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match request.poll() {
            Ok(Some(value)) => return Ok(value),
            Err(error) => return Err(error),
            Ok(None) => assert!(Instant::now() < until, "original worker response missing"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_worker_polling_and_cancellation_return_before_held_child_release() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let (inline, _gate, response) = held_transport();
    drop(response);
    let mut request = NativeRequest {
        inline: Some(inline),
        worker: None,
        ticket: None,
    };
    assert!(request.poll().unwrap().is_none());
    assert!(request.worker.is_some());
    assert!(!request.reap().unwrap());
    assert!(!request.progress().reaped);
    request.cancel().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !request.reap().unwrap() {
        assert!(Instant::now() < until, "original helper not retired");
        std::thread::sleep(Duration::from_millis(1));
    }
    let progress = request.progress();
    assert!(progress.reaped && progress.stdout_eof && progress.stderr_eof);
    assert!(receive(&mut request).is_err());
    assert_eq!(budget.0.load(Ordering::Acquire), 1);
    drop(request);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_actual_reap_and_eofs_do_not_retire_an_unjoined_worker() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let (inline, mut gate, mut output) = held_transport();
    frame(&mut output, &response());
    drop(output);
    let (arrived, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let worker = Worker::start_after_retirement(inline, Arc::clone(&ticket), move || {
        arrived.send(()).unwrap();
        held.recv().unwrap();
    })
    .unwrap_or_else(|_| panic!("worker startup failed"));
    let mut request = NativeRequest {
        inline: None,
        worker: Some(worker),
        ticket: Some(ticket),
    };
    gate.write_all(b"release\n").unwrap();
    let reached = observed.recv_timeout(Duration::from_secs(5)).is_ok();
    let before = request.progress();
    let retired = request.reap().unwrap();
    let early = request.poll().unwrap();
    // Release the actual worker before assertions, including sensitivity failures.
    release.send(()).unwrap();
    let delivered = receive(&mut request).unwrap();
    assert!(reached);
    assert!(before.stdout_eof && before.stderr_eof && !before.reaped);
    assert!(!retired && early.is_none());
    assert_eq!(delivered, response());
    assert!(request.reap().unwrap());
    assert_eq!(budget.0.load(Ordering::Acquire), 1);
    drop(request);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

fn storage_boundary(extra: usize) -> Value {
    // Parsed short keys allocate eight bytes, and the format string allocates
    // 32; numeric token storage is exact, allowing a one-byte boundary change.
    let value_bytes = std::mem::size_of::<Value>();
    let entry_bytes = 4096 + 8 + value_bytes;
    let count = (RESPONSE_STORAGE - value_bytes - 32) / entry_bytes - 4;
    let digits = RESPONSE_STORAGE - (value_bytes + 32 + (count + 4) * entry_bytes) + extra;
    let mut result = BTreeMap::new();
    for index in 0..count {
        result.insert(format!("key{index:04}"), Value::Null);
    }
    result.insert("padding".into(), Value::Num("1".repeat(digits)));
    Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.native-response/1".into())),
        ("ok".into(), Value::Bool(true)),
        ("result".into(), Value::Obj(result)),
    ]))
}

#[test]
fn native_worker_public_poll_pins_exact_response_storage_and_one_byte_over() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    for extra in [0, 1] {
        let expected = storage_boundary(extra);
        let (inline, mut gate, mut output) = held_transport();
        frame(&mut output, &expected);
        drop(output);
        let mut request = NativeRequest {
            inline: Some(inline),
            worker: None,
            ticket: None,
        };
        assert!(request.poll().unwrap().is_none());
        gate.write_all(b"release\n").unwrap();
        let result = receive(&mut request);
        assert!(request.reap().unwrap());
        match extra {
            0 => assert_eq!(result.unwrap(), expected),
            _ => assert_eq!(
                result.unwrap_err(),
                "native client response retained storage exceeds bound"
            ),
        }
        drop(request);
    }
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_public_start_refuses_the_next_slot_before_helper_startup() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let tickets = (0..WORKER_SLOTS)
        .map(|_| reserve_current().unwrap().unwrap())
        .collect::<Vec<_>>();
    let request = parse(
        br#"{"format":"fsm.native-request/1","action":"prepare","payload":null}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let refusal = NativeRequest::start(
        "00000000000000000000000000000000",
        1,
        &request,
        Duration::from_secs(1),
    );
    assert!(matches!(refusal, Err(error) if error == "native transport worker capacity exhausted"));
    assert_eq!(budget.0.load(Ordering::Acquire), WORKER_SLOTS);
    drop(tickets);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}
