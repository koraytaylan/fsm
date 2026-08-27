//! A minimal stdio MCP client: one process, one tool call.
//!
//! The runner already spawns processes under a timeout with bounded capture
//! and kills them. This makes one of those processes a *conversation* instead
//! of an exit code, and reuses everything else — the spawn, the timeout, the
//! kill, the capture. Written against the workspace's own JSON parser and
//! writer, so no dependency is added and none of `fsm-cli`'s server code is
//! reachable from here: this crate must not depend on the binary crate.
//!
//! # One effect is one tool call
//!
//! The exchange is fixed and short: `initialize`, `notifications/initialized`,
//! one `tools/call`, and the response to it. There is no second call, and that
//! is the constraint that keeps this feature small. A handler that needs two
//! calls is **two effects** — which is not a limitation but the design: each
//! effect is independently retryable, independently journaled, and
//! independently visible in the outbox. A client that made two calls would
//! have to decide what a failure of the second means for the first, and that
//! decision belongs to the machine, in a transition, where an operator can
//! read it.
//!
//! # One process per effect
//!
//! No pooling and no long-lived connections, for the reason every subprocess
//! handler gets its own process: an isolated timeout, an isolated kill, and no
//! state shared between effects that could make one effect's failure another's
//! problem. A pooled server would also make the *order* of effects observable
//! to the tool, which nothing in the machine's semantics promises.
//!
//! # The conversation is blocking, and does not block the tick
//!
//! A tool call takes as long as the tool takes, so the exchange runs on a
//! worker thread and the tick polls for its answer — exactly as it polls a
//! subprocess for its exit. The worker owns the pipes; the **runner** owns the
//! child, so a timeout is still enforced by the scheduler's deadline and
//! `Runner::kill`, which closes the pipes and ends the worker. Nothing here
//! implements a second timeout.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};

use fsm_core::json::{JsonLimits, Value, parse, write_canonical};

/// The protocol version this client offers.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// The most one inbound line may be, matching the server half of this
/// workspace.
///
/// A server that sends more than sixteen mebibytes on one line is not sending
/// a tool result; it is either broken or hostile, and reading it would let a
/// subprocess choose how much memory the executor allocates.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// The id of the `initialize` request, and of the one `tools/call`.
const INITIALIZE_ID: i64 = 1;
const CALL_ID: i64 = 2;

/// Why an exchange was not a valid one, from a closed set.
///
/// Closed and `'static` on purpose: this reaches the ack's `result`, which the
/// store fingerprints, so a reason carrying an OS error string or a path would
/// make a re-issued ack a conflict instead of a replay. Each variant is a
/// diagnosis an operator can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolFault {
    /// The stream ended before the awaited response arrived.
    Closed,
    /// A line was not valid JSON, or was not a JSON-RPC object.
    MalformedLine,
    /// A line was longer than [`MAX_LINE_BYTES`].
    OversizedLine,
    /// A response arrived carrying an id nothing had asked for.
    IdMismatch,
    /// The `initialize` response carried no result.
    NoInitializeResult,
    /// The `tools/call` response carried neither a result nor an error.
    NoCallResult,
    /// Writing to the server's standard input failed.
    WriteFailed,
}

impl ProtocolFault {
    /// The identifier this fault is journaled and traced as.
    pub fn as_str(self) -> &'static str {
        match self {
            ProtocolFault::Closed => "closed",
            ProtocolFault::MalformedLine => "malformed_line",
            ProtocolFault::OversizedLine => "oversized_line",
            ProtocolFault::IdMismatch => "id_mismatch",
            ProtocolFault::NoInitializeResult => "no_initialize_result",
            ProtocolFault::NoCallResult => "no_call_result",
            ProtocolFault::WriteFailed => "write_failed",
        }
    }
}

/// What one tool call did, at the protocol level.
///
/// Deliberately *not* an ack: this says what happened on the wire, and the
/// mapping onto a journaled `result` is a separate decision made in one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpOutcome {
    /// The tool answered. `is_error` is the tool's own `isError` flag, which
    /// is a statement about the work, not about the protocol.
    Answered {
        /// `structuredContent` when the server sent it, `content` otherwise.
        structured: Value,
        /// Whether the tool reported its own failure.
        is_error: bool,
    },
    /// The server answered the call with a JSON-RPC error.
    RpcError {
        /// The JSON-RPC error code, verbatim.
        code: i64,
        /// Its message, verbatim.
        message: String,
    },
    /// The exchange was not a valid one.
    Protocol(ProtocolFault),
}

impl McpOutcome {
    /// Whether this call is acked `ok`. Only a tool that answered without
    /// raising its own error flag is.
    pub fn succeeded(&self) -> bool {
        matches!(
            self,
            McpOutcome::Answered {
                is_error: false,
                ..
            }
        )
    }
}

/// Run the whole exchange over one server's pipes, blocking until it ends.
///
/// Never panics and never returns an `io::Error`: a protocol violation by a
/// subprocess is a failure of *that effect*, and the executor's job is to
/// journal it rather than to fall over. Every failure path here produces an
/// [`McpOutcome`] the ack can carry.
///
/// There is no timeout in here. The scheduler's deadline and `Runner::kill`
/// enforce it by closing these pipes, which ends the read this function is
/// sitting in — one timeout for both handler kinds, in one place.
pub fn converse(stdin: impl Write, stdout: impl Read, tool: &str, arguments: &Value) -> McpOutcome {
    let mut writer = stdin;
    let mut reader = BufReader::new(stdout);

    if write_line(&mut writer, &initialize_request()).is_err() {
        return McpOutcome::Protocol(ProtocolFault::WriteFailed);
    }
    match await_response(&mut reader, INITIALIZE_ID) {
        Ok(response) => {
            // A server that answers `initialize` with an error is not a server
            // this client can talk to, and saying "no result" is the honest
            // description of what came back.
            if response.get("result").is_none() {
                return McpOutcome::Protocol(ProtocolFault::NoInitializeResult);
            }
        }
        Err(fault) => return McpOutcome::Protocol(fault),
    }
    if write_line(&mut writer, &initialized_notification()).is_err() {
        return McpOutcome::Protocol(ProtocolFault::WriteFailed);
    }
    if write_line(&mut writer, &call_request(tool, arguments)).is_err() {
        return McpOutcome::Protocol(ProtocolFault::WriteFailed);
    }
    match await_response(&mut reader, CALL_ID) {
        Ok(response) => interpret(&response),
        Err(fault) => McpOutcome::Protocol(fault),
    }
}

/// Turn the `tools/call` response into what happened.
fn interpret(response: &Value) -> McpOutcome {
    if let Some(error) = response.get("error") {
        return McpOutcome::RpcError {
            code: error
                .get("code")
                .and_then(Value::as_num)
                .and_then(|code| code.parse::<i64>().ok())
                .unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        };
    }
    let Some(result) = response.get("result") else {
        return McpOutcome::Protocol(ProtocolFault::NoCallResult);
    };
    // `structuredContent` first: a tool that returns typed data should not
    // have its result flattened into rendered text on the way to the journal.
    let structured = result
        .get("structuredContent")
        .or_else(|| result.get("content"))
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    McpOutcome::Answered {
        structured,
        // Absent means false, which is what the protocol says and what a
        // server that only ever succeeds will send.
        is_error: matches!(result.get("isError"), Some(Value::Bool(true))),
    }
}

/// Read lines until the response with this id, ignoring everything else.
///
/// A server that logs is not a server that failed: notifications, log
/// messages, and requests of its own all pass by. What does not pass is a
/// *response* carrying an id nobody asked for, because that means the two
/// sides disagree about which call is being answered.
fn await_response(reader: &mut impl BufRead, awaited: i64) -> Result<Value, ProtocolFault> {
    loop {
        let line = read_capped_line(reader, MAX_LINE_BYTES)?;
        let Some(message) = line else {
            return Err(ProtocolFault::Closed);
        };
        let Ok(value) = parse(&message, &JsonLimits::DEFAULT) else {
            return Err(ProtocolFault::MalformedLine);
        };
        if value.as_obj().is_none() {
            return Err(ProtocolFault::MalformedLine);
        }
        // Anything naming a method is the server talking, not answering.
        if value.get("method").is_some() {
            continue;
        }
        let Some(id) = value.get("id") else {
            return Err(ProtocolFault::MalformedLine);
        };
        let matches = id
            .as_num()
            .and_then(|id| id.parse::<i64>().ok())
            .is_some_and(|id| id == awaited);
        if !matches {
            return Err(ProtocolFault::IdMismatch);
        }
        return Ok(value);
    }
}

/// One line, or `None` at a clean end of stream.
fn read_capped_line(
    reader: &mut impl BufRead,
    cap: usize,
) -> Result<Option<Vec<u8>>, ProtocolFault> {
    let mut line = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ProtocolFault::Closed),
        };
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }
        if let Some(end) = available.iter().position(|byte| *byte == b'\n') {
            if line.len() + end > cap {
                return Err(ProtocolFault::OversizedLine);
            }
            line.extend_from_slice(&available[..end]);
            reader.consume(end + 1);
            return Ok(Some(line));
        }
        // No newline in what is buffered. Refusing here rather than after
        // accumulating means a server that never sends one cannot make the
        // executor allocate without bound.
        if line.len() + available.len() > cap {
            return Err(ProtocolFault::OversizedLine);
        }
        line.extend_from_slice(available);
        let taken = available.len();
        reader.consume(taken);
    }
}

fn write_line(writer: &mut impl Write, value: &Value) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    write_canonical(value, &mut bytes);
    bytes.push(b'\n');
    // One write, then one flush: a server reading line-by-line must see the
    // whole message or none of it.
    writer.write_all(&bytes)?;
    writer.flush()
}

fn request(id: i64, method: &str, params: Value) -> Value {
    Value::Obj(BTreeMap::from([
        ("jsonrpc".to_string(), Value::Str("2.0".into())),
        ("id".to_string(), Value::Num(id.to_string())),
        ("method".to_string(), Value::Str(method.into())),
        ("params".to_string(), params),
    ]))
}

fn initialize_request() -> Value {
    request(
        INITIALIZE_ID,
        "initialize",
        Value::Obj(BTreeMap::from([
            (
                "protocolVersion".to_string(),
                Value::Str(PROTOCOL_VERSION.into()),
            ),
            // No capabilities are declared, and that is the point: this client
            // cannot be asked to sample, to elicit, or to serve a root. A
            // server that asks anyway is ignored.
            ("capabilities".to_string(), Value::Obj(BTreeMap::new())),
            (
                "clientInfo".to_string(),
                Value::Obj(BTreeMap::from([
                    ("name".to_string(), Value::Str("fsm-execute".into())),
                    // The crate version, not a build stamp: this reaches no
                    // journaled value, but a varying string here would still
                    // make one exchange differ from the next.
                    (
                        "version".to_string(),
                        Value::Str(env!("CARGO_PKG_VERSION").into()),
                    ),
                ])),
            ),
        ])),
    )
}

fn initialized_notification() -> Value {
    Value::Obj(BTreeMap::from([
        ("jsonrpc".to_string(), Value::Str("2.0".into())),
        (
            "method".to_string(),
            Value::Str("notifications/initialized".into()),
        ),
    ]))
}

fn call_request(tool: &str, arguments: &Value) -> Value {
    request(
        CALL_ID,
        "tools/call",
        Value::Obj(BTreeMap::from([
            ("name".to_string(), Value::Str(tool.into())),
            ("arguments".to_string(), arguments.clone()),
        ])),
    )
}
