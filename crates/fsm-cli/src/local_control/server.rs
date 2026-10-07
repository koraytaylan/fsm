//! One bounded nonblocking loop; response waiting cannot occupy parser workers.
use super::{
    invalid,
    protocol::{self, ControlIdentity, REQUEST_CAP, RESPONSE_CAP},
};
use fsm_core::json::{JsonLimits, Value, parse, write_canonical};
use fsm_execute::service::{ExecutorControl, ExecutorPhase, ShutdownReport, ShutdownRequest};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const CONNECTION_CAP: usize = 64;
const WIRE_TIME: Duration = Duration::from_millis(250);

struct Connection {
    stream: UnixStream,
    deadline: Instant,
    input: Vec<u8>,
    request: Option<ShutdownRequest>,
    output: Option<Vec<u8>>,
    written: usize,
}
impl Connection {
    fn new(stream: UnixStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            deadline: Instant::now() + WIRE_TIME,
            input: Vec::with_capacity(REQUEST_CAP),
            request: None,
            output: None,
            written: 0,
        })
    }

    fn poll(
        &mut self,
        control: &ExecutorControl,
        identity: &ControlIdentity,
        report: Option<&ShutdownReport>,
    ) -> io::Result<bool> {
        if self.output.is_none() && self.request.is_none() {
            if Instant::now() >= self.deadline {
                return Ok(false);
            }
            let mut bytes = [0u8; REQUEST_CAP + 1];
            match self.stream.read(&mut bytes) {
                Ok(0) => return Ok(false),
                Ok(count) => {
                    let bytes = &bytes[..count];
                    let end = bytes.iter().position(|byte| *byte == b'\n');
                    let body = &bytes[..end.unwrap_or(count)];
                    if body.len() > REQUEST_CAP - self.input.len() {
                        return Err(invalid("control request exceeds byte limit"));
                    }
                    self.input.extend_from_slice(body);
                    if end.is_some() {
                        let observation_version_two = parse(&self.input, &JsonLimits::DEFAULT)
                            .ok()
                            .is_some_and(|value| {
                                value.get("format").and_then(Value::as_str)
                                    == Some("fsm.executor-observe/2")
                            });
                        let request = parse(&self.input, &JsonLimits::DEFAULT)
                            .map_err(|_| invalid("invalid control JSON"))
                            .and_then(|value| {
                                if value.get("format").and_then(Value::as_str).is_some_and(
                                    |format| {
                                        matches!(
                                            format,
                                            "fsm.executor-observe/1" | "fsm.executor-observe/2"
                                        )
                                    },
                                ) {
                                    protocol::validate_observation(identity, &value)
                                        .map(|()| None)
                                        .map_err(|error| invalid(&error.message))
                                } else {
                                    protocol::apply_request(control, identity, &value)
                                        .map(Some)
                                        .map_err(|error| invalid(&error.message))
                                }
                            });
                        self.input.clear();
                        match request {
                            Ok(Some(request)) => self.request = Some(request),
                            Ok(None) => {
                                let (report, phases) = control.observation();
                                let value = if observation_version_two {
                                    protocol::observation_report_value(identity, &report, phases)
                                } else {
                                    protocol::report_value(identity, &report)
                                };
                                self.reply(value)?;
                            }
                            Err(error) => self.reply(Value::Obj(BTreeMap::from([(
                                "error".into(),
                                Value::Str(error.to_string()),
                            )])))?,
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
        }
        if self.output.is_none() {
            if self.request.is_some() {
                if let Some(report) = report {
                    if matches!(
                        report.phase,
                        ExecutorPhase::Stopped | ExecutorPhase::Uncertain
                    ) {
                        self.reply(protocol::report_value(identity, report))?;
                    }
                }
            }
        }
        if let Some(output) = &self.output {
            if Instant::now() >= self.deadline {
                return Ok(false);
            }
            match self.stream.write(&output[self.written..]) {
                Ok(0) => return Ok(false),
                Ok(count) => self.written += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            if self.written == output.len() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn reply(&mut self, value: Value) -> io::Result<()> {
        let mut bytes = Vec::with_capacity(RESPONSE_CAP + 1);
        write_canonical(&value, &mut bytes);
        if bytes.len() > RESPONSE_CAP {
            return Err(invalid("control report exceeds byte limit"));
        }
        bytes.push(b'\n');
        self.output = Some(bytes);
        self.deadline = Instant::now() + WIRE_TIME;
        Ok(())
    }
}

pub(super) fn run(
    listener: UnixListener,
    control: ExecutorControl,
    identity: ControlIdentity,
    stop: &AtomicBool,
) {
    let mut connections = VecDeque::with_capacity(CONNECTION_CAP);
    while !stop.load(Ordering::Acquire) {
        // Bound accept work per iteration so flooding cannot starve existing I/O.
        for _ in 0..8 {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Ok(connection) = Connection::new(stream) {
                        // Eviction loses only a response, never the accepted stop.
                        // Thus old long drains cannot exclude a later abort.
                        if connections.len() == CONNECTION_CAP {
                            connections.pop_front();
                        }
                        connections.push_back(connection);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => return,
            }
        }
        // Share one bounded inventory snapshot among waiting responses.
        let report = connections
            .iter()
            .any(|connection| connection.request.is_some() && connection.output.is_none())
            .then(|| control.report());
        connections.retain_mut(|connection| {
            connection
                .poll(&control, &identity, report.as_ref())
                .unwrap_or(false)
        });
        std::thread::sleep(Duration::from_millis(1));
    }
    // Stop accepting immediately, but retire already accepted responses within
    // their original request budgets; owner close must not race their first write.
    drop(listener);
    while !connections.is_empty() {
        let report = control.report();
        connections.retain_mut(|connection| {
            (connection.output.is_some()
                || matches!(
                    report.phase,
                    ExecutorPhase::Stopped | ExecutorPhase::Uncertain
                ))
                && connection
                    .request
                    .as_ref()
                    .is_some_and(|request| Instant::now() < request.deadline())
                && connection
                    .poll(&control, &identity, Some(&report))
                    .unwrap_or(false)
        });
        if !connections.is_empty() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    // Bounded connections retire before endpoint cleanup.
}
