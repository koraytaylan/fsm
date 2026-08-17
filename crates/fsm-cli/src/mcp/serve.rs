//! Blocking newline-delimited MCP serve loop.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;

use crate::args::resolve_data_dir;
use crate::clock::{Clock, SystemClock};
use crate::render::render_human;
use crate::store::{ErrorObj, Store};

use super::jsonrpc::{
    INVALID_PARAMS, INVALID_REQUEST, Incoming, METHOD_NOT_FOUND, NOT_INITIALIZED, PARSE_ERROR,
    WireError, error_response, parse_line, result_response,
};
use super::tools;

const LINE_CAP: usize = 16 * 1024 * 1024;
const KNOWN_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const DEFAULT_VERSION: &str = "2025-06-18";

static HOOK: AtomicBool = AtomicBool::new(false);

enum Line {
    Eof,
    Data(Vec<u8>),
    TooLong,
}

pub fn negotiate(client: Option<&str>) -> &'static str {
    match client {
        Some(v) if KNOWN_VERSIONS.contains(&v) => {
            // leak-free: return the matching static
            KNOWN_VERSIONS.iter().copied().find(|k| *k == v).unwrap()
        }
        _ => DEFAULT_VERSION,
    }
}

pub fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    error_response(id, code, message)
}

pub fn tool_error(err: &ErrorObj) -> Value {
    let mut item = std::collections::BTreeMap::new();
    item.insert("type".into(), Value::Str("text".into()));
    item.insert("text".into(), Value::Str(render_human(&err.to_value())));
    let mut sc = std::collections::BTreeMap::new();
    sc.insert("error".into(), err.to_value());
    let mut result = std::collections::BTreeMap::new();
    result.insert("content".into(), Value::Arr(vec![Value::Obj(item)]));
    result.insert("structuredContent".into(), Value::Obj(sc));
    result.insert("isError".into(), Value::Bool(true));
    Value::Obj(result)
}

pub fn tool_ok(structured: Value) -> Value {
    let mut item = std::collections::BTreeMap::new();
    item.insert("type".into(), Value::Str("text".into()));
    item.insert("text".into(), Value::Str(render_human(&structured)));
    let mut result = std::collections::BTreeMap::new();
    result.insert("content".into(), Value::Arr(vec![Value::Obj(item)]));
    result.insert("structuredContent".into(), structured);
    Value::Obj(result)
}

fn install_panic_hook() {
    if HOOK
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::panic::set_hook(Box::new(|info| {
            let _ = writeln!(std::io::stderr(), "fsm panic: {info}");
            std::process::abort();
        }));
    }
}

pub fn run() -> std::io::Result<()> {
    install_panic_hook();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(stdin.lock(), stdout.lock())
}

pub fn serve(input: impl BufRead, output: impl Write) -> std::io::Result<()> {
    let dir = resolve_data_dir(None);
    let mut store = match Store::open(&dir) {
        Ok(s) => Some(s),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "fsm store open failed: {}", e.message);
            return Err(std::io::Error::other(e.message));
        }
    };
    let mut clock = SystemClock;
    serve_session(store.as_mut(), &mut clock, input, output)
}

pub fn serve_session(
    mut store: Option<&mut Store>,
    clock: &mut dyn Clock,
    mut input: impl BufRead,
    mut output: impl Write,
) -> std::io::Result<()> {
    if std::env::var("FSM_MCP_PANIC").ok().as_deref() == Some("1") {
        install_panic_hook();
        panic!("deliberate serve panic");
    }
    let mut initialized = false;
    loop {
        match read_capped_line(&mut input, LINE_CAP)? {
            Line::Eof => {
                output.flush()?;
                return Ok(());
            }
            Line::TooLong => {
                let msg = format!("parse error: line exceeds {LINE_CAP} bytes");
                send_line(&mut output, &rpc_error(Value::Null, PARSE_ERROR, &msg))?;
                continue;
            }
            Line::Data(buf) => {
                let line = match std::str::from_utf8(&buf) {
                    Ok(s) => s.trim_end_matches('\r').to_string(),
                    Err(_) => {
                        send_line(
                            &mut output,
                            &rpc_error(Value::Null, PARSE_ERROR, "parse error"),
                        )?;
                        continue;
                    }
                };
                if line.is_empty() {
                    continue;
                }
                match parse_line(&line) {
                    Err(WireError::Parse(_)) => {
                        send_line(
                            &mut output,
                            &rpc_error(Value::Null, PARSE_ERROR, "parse error"),
                        )?;
                    }
                    Err(WireError::Batch) => {
                        send_line(
                            &mut output,
                            &rpc_error(
                                Value::Null,
                                INVALID_REQUEST,
                                "batch requests are not supported",
                            ),
                        )?;
                    }
                    Err(WireError::Invalid) => {
                        send_line(
                            &mut output,
                            &rpc_error(Value::Null, INVALID_REQUEST, "invalid request"),
                        )?;
                    }
                    Ok(Incoming::Notification { method, .. }) => {
                        if method == "notifications/cancelled" {
                            let _ = writeln!(
                                std::io::stderr(),
                                "fsm info: cancelled notification ignored"
                            );
                        }
                    }
                    Ok(Incoming::Request { id, method, params }) => {
                        handle_request(
                            &mut output,
                            store.as_deref_mut(),
                            clock,
                            &mut initialized,
                            id,
                            &method,
                            params,
                        )?;
                    }
                }
            }
        }
    }
}

fn handle_request(
    output: &mut impl Write,
    store: Option<&mut Store>,
    clock: &mut dyn Clock,
    initialized: &mut bool,
    id: Value,
    method: &str,
    params: Option<Value>,
) -> std::io::Result<()> {
    match method {
        "ping" => send_line(output, &result_response(id, Value::Obj(Default::default()))),
        "initialize" => {
            let offered = params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str);
            let version = negotiate(offered);
            *initialized = true;
            send_line(output, &result_response(id, initialize_result(version)))
        }
        _ if !*initialized => send_line(
            output,
            &rpc_error(id, NOT_INITIALIZED, "Server not initialized"),
        ),
        "tools/list" => send_line(output, &result_response(id, tools::tools_list_result())),
        "resources/list" => send_line(
            output,
            &result_response(id, super::resources::list(store.as_deref())),
        ),
        "resources/templates/list" => {
            send_line(output, &result_response(id, super::resources::templates()))
        }
        "resources/read" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match super::resources::read(uri, store.as_deref()) {
                Ok(v) => send_line(output, &result_response(id, v)),
                Err(_) => send_line(output, &rpc_error(id, -32002, "Resource not found")),
            }
        }
        "prompts/list" => send_line(output, &result_response(id, super::prompts::list())),
        "prompts/get" => {
            let name = params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = params.as_ref().and_then(|p| p.get("arguments"));
            match super::prompts::get(name, args) {
                Ok(v) => send_line(output, &result_response(id, v)),
                Err(e) => send_line(output, &rpc_error(id, INVALID_PARAMS, &e.message)),
            }
        }
        "tools/call" => {
            let name = params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = params
                .as_ref()
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(Value::Obj(Default::default()));
            if name == "fsm_ping" {
                send_line(output, &result_response(id, fsm_ping_result()))
            } else if !tools::names().contains(&name) {
                send_line(
                    output,
                    &rpc_error(
                        id,
                        INVALID_PARAMS,
                        &format!("unknown tool; valid: {}", tools::names().join(" ")),
                    ),
                )
            } else {
                match store {
                    Some(st) => match tools::dispatch(st, clock, name, &args) {
                        Ok(v) => send_line(output, &result_response(id, tool_ok(v))),
                        Err(e) => send_line(output, &result_response(id, tool_error(&e))),
                    },
                    None => send_line(
                        output,
                        &result_response(id, tool_error(&ErrorObj::new("io/read", "no store"))),
                    ),
                }
            }
        }
        other => send_line(
            output,
            &rpc_error(id, METHOD_NOT_FOUND, &format!("method not found: {other}")),
        ),
    }
}

fn initialize_result(version: &str) -> Value {
    let mut tools = std::collections::BTreeMap::new();
    tools.insert("listChanged".into(), Value::Bool(false));
    let mut resources = std::collections::BTreeMap::new();
    resources.insert("subscribe".into(), Value::Bool(false));
    resources.insert("listChanged".into(), Value::Bool(false));
    let mut prompts = std::collections::BTreeMap::new();
    prompts.insert("listChanged".into(), Value::Bool(false));
    let mut caps = std::collections::BTreeMap::new();
    caps.insert("tools".into(), Value::Obj(tools));
    caps.insert("resources".into(), Value::Obj(resources));
    caps.insert("prompts".into(), Value::Obj(prompts));
    let mut info = std::collections::BTreeMap::new();
    info.insert("name".into(), Value::Str("fsm".into()));
    info.insert(
        "title".into(),
        Value::Str("fsm — deterministic state machines for LLM workflows".into()),
    );
    info.insert(
        "version".into(),
        Value::Str(env!("CARGO_PKG_VERSION").into()),
    );
    let mut result = std::collections::BTreeMap::new();
    result.insert("protocolVersion".into(), Value::Str(version.into()));
    result.insert("capabilities".into(), Value::Obj(caps));
    result.insert("serverInfo".into(), Value::Obj(info));
    result.insert(
        "instructions".into(),
        Value::Str(super::prompts::INSTRUCTIONS.into()),
    );
    Value::Obj(result)
}

fn fsm_ping_result() -> Value {
    let mut item = std::collections::BTreeMap::new();
    item.insert("type".into(), Value::Str("text".into()));
    item.insert("text".into(), Value::Str("pong".into()));
    let mut result = std::collections::BTreeMap::new();
    result.insert("content".into(), Value::Arr(vec![Value::Obj(item)]));
    Value::Obj(result)
}

fn send_line(out: &mut impl Write, v: &Value) -> std::io::Result<()> {
    let bytes = canon_bytes(v);
    debug_assert!(!bytes.contains(&b'\n'));
    out.write_all(&bytes)?;
    out.write_all(b"\n")?;
    out.flush()
}

fn read_capped_line(input: &mut impl BufRead, cap: usize) -> std::io::Result<Line> {
    let mut buf = Vec::new();
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(Line::Eof)
            } else {
                Ok(Line::Data(buf))
            };
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            if buf.len() + pos > cap {
                input.consume(pos + 1);
                return Ok(Line::TooLong);
            }
            buf.extend_from_slice(&available[..pos]);
            input.consume(pos + 1);
            return Ok(Line::Data(buf));
        }
        if buf.len() + available.len() > cap {
            let n = available.len();
            input.consume(n);
            let mut rest = Vec::new();
            let _ = input.read_until(b'\n', &mut rest);
            return Ok(Line::TooLong);
        }
        buf.extend_from_slice(available);
        let n = available.len();
        input.consume(n);
    }
}
