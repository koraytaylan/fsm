//! Real child transport fixtures authenticate no Root closure or durable ownership.

use super::{InlineRequest, NativeRequest, RESPONSE_LIMIT, Reader};
use fsm_core::{
    canon::canon_bytes,
    json::{JsonLimits, Value, parse},
    record::execution::Claim,
};
use std::{
    io::Write,
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn held_transport() -> (InlineRequest, UnixStream, UnixStream) {
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

pub(super) fn frame(response: &mut UnixStream, value: &Value) {
    let body = canon_bytes(value);
    assert!(body.len() + 4 <= RESPONSE_LIMIT);
    response
        .write_all(&(body.len() as u32).to_be_bytes())
        .unwrap();
    response.write_all(&body).unwrap();
}

pub(super) fn response() -> Value {
    parse(
        br#"{"format":"fsm.native-response/1","ok":true,"result":null}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

pub(super) fn receive(request: &mut NativeRequest) -> Result<Value, String> {
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

pub(super) fn original_claim() -> Claim {
    let fixture = parse(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fsm-core/tests/fixtures/execution-handoff.json"
        )),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    Claim::from_value(fixture.get("claim").unwrap()).unwrap()
}

pub(super) fn completed_transport() -> super::startup::FixtureFactory {
    super::startup::FixtureFactory::install(|prepared| {
        let (mut request, mut gate, mut output) = held_transport();
        request.deadline = prepared.deadline;
        frame(&mut output, &response());
        drop(output);
        gate.write_all(b"release\n").unwrap();
        Ok(request)
    })
}
