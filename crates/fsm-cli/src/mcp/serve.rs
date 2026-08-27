//! Blocking newline-delimited MCP serve loop.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use fsm_core::json::Value;

use crate::args::resolve_data_dir;
use crate::clock::{Clock, SystemClock};
use crate::render::render_human;
use crate::store::{ErrorObj, Store};

use super::jsonrpc::{
    INVALID_PARAMS, INVALID_REQUEST, Incoming, METHOD_NOT_FOUND, NOT_INITIALIZED, PARSE_ERROR,
    RESOURCE_NOT_FOUND, WireError, error_response, parse_line, result_response,
};
use super::notify::{FeedHandle, Notifier};
use super::tools;
use super::{cancel, logging, subscribe, watch};

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
    let wrapped = Value::Obj(std::collections::BTreeMap::from([(
        "error".into(),
        err.to_value(),
    )]));
    let mut item = std::collections::BTreeMap::new();
    item.insert("type".into(), Value::Str("text".into()));
    item.insert("text".into(), Value::Str(render_human(&wrapped)));
    let mut result = std::collections::BTreeMap::new();
    result.insert("content".into(), Value::Arr(vec![Value::Obj(item)]));
    result.insert("structuredContent".into(), wrapped);
    result.insert("isError".into(), Value::Bool(true));
    Value::Obj(result)
}

pub fn tool_ok(name: &str, structured: Value) -> Value {
    let mut item = std::collections::BTreeMap::new();
    item.insert("type".into(), Value::Str("text".into()));
    item.insert("text".into(), Value::Str(render_human(&structured)));
    let mut content = vec![Value::Obj(item)];
    // A model that creates a workflow gets a handle to it rather than a
    // string it has to reassemble into a URI — and the link is what makes
    // the resource it can subscribe to discoverable at the moment it becomes
    // relevant. The id comes from the *result*, so a tool that resolved or
    // defaulted one links to what it actually acted on.
    if tools::LINKED_TOOLS.contains(&name)
        && let Some(instance_id) = structured.get("instance_id").and_then(Value::as_str)
    {
        content.push(Value::Obj(std::collections::BTreeMap::from([
            ("type".into(), Value::Str("resource_link".into())),
            (
                "uri".into(),
                Value::Str(format!("fsm://instance/{instance_id}")),
            ),
            ("name".into(), Value::Str(instance_id.into())),
            ("mimeType".into(), Value::Str("application/json".into())),
        ])));
    }
    let mut result = std::collections::BTreeMap::new();
    result.insert("content".into(), Value::Arr(content));
    // Untouched: this is what the parity suites compare against the CLI's
    // `--json`, and a cosmetic addition to `content` must not move it.
    result.insert("structuredContent".into(), structured);
    Value::Obj(result)
}

pub fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    format!(
        "fsm panic: {info}\n{}",
        std::backtrace::Backtrace::force_capture()
    )
}

fn install_panic_hook() {
    if HOOK
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        std::panic::set_hook(Box::new(|info| {
            let _ = writeln!(std::io::stderr(), "{}", panic_text(info));
            std::process::abort();
        }));
    }
}

/// How this server relates to the writer lock and to the executor.
///
/// The mode is a deployment decision, not a protocol one: it decides who may
/// write to the data directory while the server is up. See `docs/EMBEDDING.md`
/// for the decision rule.
pub enum ServeMode {
    /// This process holds the writer and runs no handlers. The default.
    Writer,
    /// The executor owns the writer; this process watches. Every tool in
    /// `MUTATING_TOOLS` is refused with a sentence naming the mode.
    ReadOnly,
    /// This process holds the writer *and* runs the executor loop inline, one
    /// tick per protocol iteration.
    Embedded(Box<ExecutorLoop>),
}

/// The executor's own components, held by an embedded server.
///
/// Bundled rather than passed separately because `serve` owns the writer and
/// lends it: `service::tick` would open a second `Store` on the same data
/// directory and collide with the lock this process already holds.
pub struct ExecutorLoop {
    watcher: fsm_execute::watch::Watcher,
    scheduler: fsm_execute::sched::Scheduler,
    runner: fsm_execute::run::Runner,
    pipeline: fsm_execute::run::Pipeline,
}

impl ExecutorLoop {
    /// Build the loop for one data directory and one validated table.
    pub fn new(
        data_dir: &std::path::Path,
        table: fsm_execute::config::HandlerTable,
    ) -> Result<Self, fsm_execute::error::ExecError> {
        Ok(Self {
            watcher: fsm_execute::watch::Watcher::new(
                data_dir.to_path_buf(),
                fsm_execute::service::advancing_effects(&table),
            ),
            scheduler: fsm_execute::sched::Scheduler::new(table),
            runner: fsm_execute::run::Runner::new()?,
            pipeline: fsm_execute::run::Pipeline,
        })
    }

    /// One tick against the writer this server already holds.
    fn tick(&mut self, store: &mut Store, clock: &mut dyn Clock) -> Vec<String> {
        let now_ms = clock.now_ms();
        fsm_execute::service::tick_with(
            &mut self.watcher,
            &mut self.scheduler,
            &mut self.runner,
            &mut self.pipeline,
            store,
            clock,
            now_ms,
        )
    }
}

pub fn run() -> std::io::Result<()> {
    run_with_dir(&resolve_data_dir(None))
}

pub fn run_with_dir(dir: &std::path::Path) -> std::io::Result<()> {
    run_with_mode(dir, ServeMode::Writer)
}

/// Run the server over stdio in one of the three modes.
pub fn run_with_mode(dir: &std::path::Path, mode: ServeMode) -> std::io::Result<()> {
    install_panic_hook();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    // `stdout()` itself is `Send`; a borrowed `StdoutLock` is not, and the
    // change feed writes from another thread.
    serve_dir_with(dir, mode, stdin.lock(), stdout)
}

pub fn serve(input: impl BufRead, output: impl Write + Send + 'static) -> std::io::Result<()> {
    serve_dir(&resolve_data_dir(None), input, output)
}

pub fn serve_dir(
    dir: &std::path::Path,
    input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    serve_dir_with(dir, ServeMode::Writer, input, output)
}

pub fn serve_dir_with(
    dir: &std::path::Path,
    mode: ServeMode,
    input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    let mut degraded: Option<String> = None;
    let opened = match &mode {
        // A read-only open takes no lock and creates nothing, which is what
        // lets this process watch a data directory the executor is writing.
        ServeMode::ReadOnly => Store::open_read_only(dir),
        ServeMode::Writer | ServeMode::Embedded(_) => Store::open(dir),
    };
    // A store that will not open used to kill the server, which is exactly
    // backwards: diagnosis is the one case where the server must not vanish.
    // The session starts, `initialize` succeeds, the tool list is unchanged,
    // and the diagnostic tools answer from the directory itself.
    let mut store = match opened {
        Ok(s) => Some(s),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "fsm store open failed: {}", e.message);
            degraded = Some(format!("{}: {}", e.code, e.message));
            None
        }
    };
    // Reported, never selected: there is no `--degraded` flag, because it is
    // not a way to run a server, it is what happened to one.
    let reported_mode = match &degraded {
        Some(_) => "degraded",
        None => mode_name(&mode),
    };
    // One startup line per process, on stderr: stdout is the protocol stream,
    // and no tick trace may depend on this line.
    let _ = writeln!(
        std::io::stderr(),
        "fsm serve: mode={reported_mode} data_dir={}{}",
        dir.display(),
        if degraded.is_some() {
            " executor=none"
        } else {
            ""
        }
    );
    let mut clock = SystemClock;
    // A read-only handle is one consistent prefix, frozen at the moment it was
    // opened. For a monitoring session that is useless: the whole point of the
    // mode is to watch the executor's acks and transitions arrive, so the
    // session reopens before each request.
    let refresh = match mode {
        ServeMode::ReadOnly => Some(dir.to_path_buf()),
        ServeMode::Writer | ServeMode::Embedded(_) => None,
    };
    let mut executor = match mode {
        // Nothing to write to, so nothing to run: an executor in a degraded
        // session would tick against a store that will not open.
        ServeMode::Embedded(_) if degraded.is_some() => None,
        ServeMode::Embedded(loop_) => Some(loop_),
        ServeMode::Writer | ServeMode::ReadOnly => None,
    };
    serve_session_degraded(
        store.as_mut(),
        &mut clock,
        executor.as_deref_mut(),
        refresh.as_deref(),
        degraded.as_deref(),
        degraded.as_ref().map(|_| dir),
        input,
        output,
    )
}

/// The name that appears in the startup line and in `instructions`.
pub fn mode_name(mode: &ServeMode) -> &'static str {
    match mode {
        ServeMode::Writer => "writer",
        ServeMode::ReadOnly => "read-only",
        ServeMode::Embedded(_) => "embedded",
    }
}

pub fn serve_session(
    store: Option<&mut Store>,
    clock: &mut dyn Clock,
    input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    serve_session_with(store, clock, None, None, input, output)
}

/// The change feed's cadence, matched to the executor's own tick so a
/// subscriber learns of a change about as fast as the executor caused it.
const FEED_INTERVAL_MS: u64 = watch::DEFAULT_INTERVAL_MS;

/// What one session knows that outlives a single request: what it watches,
/// how loudly it wants to be told, and which requests the client withdrew.
#[derive(Default)]
struct Live {
    subscriptions: subscribe::Subscriptions,
    level: Option<logging::Level>,
    cancellations: cancel::Cancellations,
    /// Whether the client advertised `elicitation` at `initialize`. Captured
    /// here because `initialize` is the only place it appears; consulted by
    /// the tool that would otherwise ask a client that cannot answer.
    client_elicitation: bool,
    /// Why the store would not open, when it would not. A client hears this
    /// once, at `error` level, as soon as it is allowed to hear anything.
    degraded: Option<String>,
    /// The directory to diagnose while degraded. Every answer a degraded
    /// session gives is a function of this path, because there is no store.
    degraded_dir: Option<std::path::PathBuf>,
    /// The change feed, spawned on the first successful subscribe and
    /// stopped when the session ends. A server nobody subscribes to spawns
    /// nothing and does no I/O between requests — which is what keeps every
    /// non-subscribing transcript byte-identical and this plan inert for the
    /// callers that do not use it.
    feed: Option<FeedHandle>,
}

impl Live {
    /// Start the change feed if this session does not have one yet.
    ///
    /// The body is `5902`'s; until then a session's subscription is recorded
    /// and nothing polls. The lifecycle is decided here regardless, because
    /// deciding it after something is spawned is how a thread outlives its
    /// session.
    fn ensure_feed(&mut self, data_dir: Option<std::path::PathBuf>, output: &Notifier) {
        if self.feed.is_some() {
            return;
        }
        let Some(data_dir) = data_dir else {
            return;
        };
        let writer = output.clone_handle();
        let watched = self.subscriptions.clone_handle();
        // The feed starts from wherever the journal is now: a subscriber
        // asked to be told what happens next, not what already had.
        let from_seq = crate::store::Store::open_read_only(&data_dir)
            .map(|store| store.journal.last_seq)
            .unwrap_or(0);
        // A test driving the feed by hand takes it here; everyone else gets
        // the timer. The session's own bookkeeping is the same either way,
        // so a hand-driven session is the same session.
        if watch::park(watch::Feed::new(&data_dir, watched, writer, from_seq)) {
            self.feed = Some(FeedHandle::parked());
            return;
        }
        let writer = output.clone_handle();
        let watched = self.subscriptions.clone_handle();
        self.feed = Some(FeedHandle::spawn(move |stop| {
            let mut feed = watch::Feed::new(&data_dir, watched, writer, from_seq);
            feed.run(stop, FEED_INTERVAL_MS);
        }));
    }

    /// Stop the feed and wait for it. Idempotent, and called on every exit
    /// path including a drop.
    fn shutdown(&mut self) {
        if let Some(mut feed) = self.feed.take() {
            feed.stop_and_join();
        }
        watch::release_parked();
    }
}

/// The protocol loop, optionally driving the executor between client lines.
///
/// Two limits of embedded mode live here because this is where they bite: a
/// long-running handler blocks the protocol, and — because this loop blocks in
/// `read_capped_line` until the client sends a line — **a tick happens only
/// when the client speaks**. Embedded mode advances a workflow during a
/// conversation, never overnight; that is why it is not the default and why
/// the unattended claim belongs to a separate executor process.
pub fn serve_session_with(
    store: Option<&mut Store>,
    clock: &mut dyn Clock,
    executor: Option<&mut ExecutorLoop>,
    refresh: Option<&std::path::Path>,
    input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    serve_session_degraded(store, clock, executor, refresh, None, None, input, output)
}

/// The protocol loop, with one more thing it may have to say: that the store
/// it was pointed at would not open.
#[allow(clippy::too_many_arguments)]
pub fn serve_session_degraded(
    mut store: Option<&mut Store>,
    clock: &mut dyn Clock,
    mut executor: Option<&mut ExecutorLoop>,
    refresh: Option<&std::path::Path>,
    degraded: Option<&str>,
    degraded_dir: Option<&std::path::Path>,
    mut input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    // One writer for the whole session: the request path and, from `5901`,
    // a background change feed share it through cloned handles.
    let output = Notifier::new(Box::new(output));
    if std::env::var("FSM_MCP_PANIC").ok().as_deref() == Some("1") {
        install_panic_hook();
        panic!("deliberate serve panic");
    }
    // Derived once: the mode is a property of how this session was started,
    // and an operator reading a transcript should be able to tell which one
    // ran without reading the launch command.
    let mode_note = mode_note(store.as_deref(), executor.is_some(), degraded.is_some());
    let mut initialized = false;
    let mut initialized_notified = false;
    let mut live = Live {
        degraded: degraded.map(str::to_string),
        degraded_dir: degraded_dir.map(std::path::Path::to_path_buf),
        ..Live::default()
    };
    loop {
        // Bound rather than matched in place: the borrow of `input` ends at
        // the semicolon, which is what lets a request arm lend the same
        // reader to a `SessionIo`.
        let line = read_capped_line(&mut input, LINE_CAP)?;
        match line {
            Line::Eof => {
                // Every send already flushed under the lock, so there is
                // nothing left buffered to push — and nothing to say: a
                // goodbye notification after the client closed stdout is a
                // write to a closed pipe.
                live.shutdown();
                return Ok(());
            }
            Line::TooLong => {
                let msg = format!("parse error: line exceeds {LINE_CAP} bytes");
                send_line(&output, &rpc_error(Value::Null, PARSE_ERROR, &msg))?;
                continue;
            }
            Line::Data(buf) => {
                let line = match std::str::from_utf8(&buf) {
                    Ok(s) => s.trim_end_matches('\r').to_string(),
                    Err(_) => {
                        send_line(&output, &rpc_error(Value::Null, PARSE_ERROR, "parse error"))?;
                        continue;
                    }
                };
                if line.is_empty() {
                    continue;
                }
                match parse_line(&line) {
                    // An answer to a request this server made, arriving when
                    // nothing is waiting for it. A client bug, and dropping
                    // it is strictly better than ending a working session
                    // over it — said at `debug`, where somebody looking for
                    // it will find it.
                    Ok(Incoming::Response { id, .. }) => {
                        logging::message(
                            &output,
                            live.level,
                            initialized,
                            logging::Level::Debug,
                            "fsm.serve",
                            || {
                                Value::Obj(BTreeMap::from([(
                                    "unmatched_response".to_string(),
                                    id.clone(),
                                )]))
                            },
                        );
                    }
                    Err(WireError::Parse(_)) => {
                        send_line(&output, &rpc_error(Value::Null, PARSE_ERROR, "parse error"))?;
                    }
                    Err(WireError::Batch) => {
                        send_line(
                            &output,
                            &rpc_error(
                                Value::Null,
                                INVALID_REQUEST,
                                "batch requests are not supported",
                            ),
                        )?;
                    }
                    Err(WireError::Invalid) => {
                        send_line(
                            &output,
                            &rpc_error(Value::Null, INVALID_REQUEST, "invalid request"),
                        )?;
                    }
                    Ok(Incoming::Notification { method, params }) => {
                        if method == "notifications/initialized" {
                            initialized_notified = true;
                        } else if method == "notifications/cancelled" {
                            // Recorded rather than discarded. What this
                            // server can actually interrupt is `6003`'s
                            // subject; losing the notification entirely was
                            // never the honest answer.
                            if let Some(requested) =
                                params.as_ref().and_then(|p| p.get("requestId"))
                            {
                                live.cancellations.cancel(requested);
                                // An id nobody is running is accepted in
                                // silence — the client may be racing a reply
                                // it has not read yet, which is not an error.
                                // It is still worth saying at `debug`, where
                                // an operator asking "did my cancel arrive?"
                                // can see that it did.
                                let reason = params
                                    .as_ref()
                                    .and_then(|p| p.get("reason"))
                                    .cloned()
                                    .unwrap_or(Value::Null);
                                let requested = requested.clone();
                                logging::message(
                                    &output,
                                    live.level,
                                    initialized,
                                    logging::Level::Debug,
                                    "fsm.serve",
                                    || {
                                        Value::Obj(BTreeMap::from([
                                            ("cancel_requested".to_string(), requested.clone()),
                                            ("reason".to_string(), reason.clone()),
                                        ]))
                                    },
                                );
                            }
                        }
                    }
                    Ok(Incoming::Request { id, method, params }) => {
                        if initialized && !initialized_notified && method != "initialize" {
                            let _ = writeln!(
                                std::io::stderr(),
                                "fsm warn: {method} before notifications/initialized"
                            );
                        }
                        refresh_read_only(store.as_deref_mut(), refresh);
                        // A client can cancel request 7 while the server is
                        // still working on request 6. Per the specification a
                        // request that was never executed gets **no**
                        // response — not an error, not a courtesy reply — and
                        // the id is cleared so a client reusing it later is
                        // not silently cancelled by a stale entry.
                        if live.cancellations.cancelled(&id) {
                            live.cancellations.finish(&id);
                            logging::message(
                                &output,
                                live.level,
                                initialized,
                                logging::Level::Debug,
                                "fsm.serve",
                                || {
                                    Value::Obj(BTreeMap::from([
                                        ("cancelled".to_string(), id.clone()),
                                        ("method".to_string(), Value::Str(method.clone())),
                                    ]))
                                },
                            );
                            continue;
                        }
                        // The session's two halves, borrowed for this one
                        // request: a server-to-client request writes through
                        // the notifier and reads its answer from this same
                        // input.
                        let io = std::cell::RefCell::new(crate::mcp::notify::SessionIo::new(
                            &output, &mut input,
                        ));
                        handle_request(
                            &output,
                            store.as_deref_mut(),
                            clock,
                            &mut initialized,
                            &mut live,
                            id,
                            &method,
                            params,
                            mode_note,
                            Some(&io),
                        )?;
                        drive_executor(
                            executor.as_deref_mut(),
                            store.as_deref_mut(),
                            clock,
                            &output,
                            &live,
                            initialized,
                        );
                    }
                }
            }
        }
    }
}

/// Re-open the read-only prefix so this request answers from the journal as
/// it is now, not as it was when the server started.
///
/// A failed reopen keeps the handle it already has: answering from a slightly
/// old prefix beats refusing to answer at all, and the next request tries
/// again.
fn refresh_read_only(store: Option<&mut Store>, refresh: Option<&std::path::Path>) {
    let (Some(store), Some(dir)) = (store, refresh) else {
        return;
    };
    if let Ok(reopened) = Store::open_read_only(dir) {
        *store = reopened;
    }
}

/// Run one executor tick against the writer this session holds.
///
/// Tick lines go to stderr: stdout carries the JSON-RPC stream, and one stray
/// line there is a protocol error rather than a log entry.
fn drive_executor(
    executor: Option<&mut ExecutorLoop>,
    store: Option<&mut Store>,
    clock: &mut dyn Clock,
    output: &Notifier,
    live: &Live,
    initialized: bool,
) {
    let (Some(executor), Some(store)) = (executor, store) else {
        return;
    };
    for line in executor.tick(store, clock) {
        // Both audiences, deliberately. An operator reading a terminal must
        // not lose output because a client attached, and a later reader who
        // "cleans up the duplication" would take that away from them.
        let _ = writeln!(std::io::stderr(), "fsm execute: {line}");
        logging::message(
            output,
            live.level,
            initialized,
            logging::Level::Info,
            "fsm.execute",
            // Structured, not a rendered sentence: `{"line": ...}` is what a
            // client can act on, and the line already carries identifiers
            // only — no path, pid, or duration, by plan 0008's rule.
            || {
                Value::Obj(std::collections::BTreeMap::from([(
                    "line".to_string(),
                    Value::Str(line.clone()),
                )]))
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_request<'a>(
    output: &'a Notifier,
    store: Option<&mut Store>,
    clock: &mut dyn Clock,
    initialized: &mut bool,
    live: &mut Live,
    id: Value,
    method: &str,
    params: Option<Value>,
    mode_note: &'static str,
    io: Option<&'a std::cell::RefCell<crate::mcp::notify::SessionIo<'a>>>,
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
            // The only message that carries what the client can do. A tool
            // that would ask the client a question needs to know whether
            // there is anybody able to answer, and it cannot see `initialize`
            // from where it runs.
            live.client_elicitation = super::elicit::client_supports(params.as_ref());
            send_line(
                output,
                &result_response(id, initialize_result(version, mode_note)),
            )?;
            // A client that only reads stdout would otherwise never learn
            // why every store-backed call is failing. Said once, at `error`,
            // and only now: nothing may be sent before `initialize`.
            if let Some(detail) = live.degraded.clone() {
                logging::message(
                    output,
                    live.level,
                    *initialized,
                    logging::Level::Error,
                    "fsm.store",
                    || {
                        Value::Obj(BTreeMap::from([
                            ("degraded".to_string(), Value::Bool(true)),
                            ("detail".to_string(), Value::Str(detail.clone())),
                            (
                                "next".to_string(),
                                Value::Str("call store_doctor for the diagnosis".into()),
                            ),
                        ]))
                    },
                );
            }
            Ok(())
        }
        _ if !*initialized => send_line(
            output,
            &rpc_error(id, NOT_INITIALIZED, "Server not initialized"),
        ),
        "tools/list" => send_line(output, &result_response(id, tools::tools_list_result())),
        "completion/complete" => match super::complete::complete(params.as_ref(), store.as_deref())
        {
            Ok(result) => send_line(output, &result_response(id, result)),
            Err(invalid) => send_line(output, &rpc_error(id, INVALID_PARAMS, &invalid.0)),
        },
        "resources/list" => send_line(
            output,
            &result_response(id, super::resources::list(store.as_deref())),
        ),
        "resources/templates/list" => {
            send_line(output, &result_response(id, super::resources::templates()))
        }
        // Every arm this plan adds is routed here and only here: five tasks
        // each adding one would serialise the plan behind one file for no
        // benefit. The registries below are real; what the later tasks add
        // is the notification each one produces.
        "resources/subscribe" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if uri.is_empty() {
                return send_line(output, &rpc_error(id, INVALID_PARAMS, "uri is required"));
            }
            // Validated against the resolver rather than a prefix match, so a
            // subscription can never name something unreadable — and refused
            // with the code a read of the same URI would give.
            if super::resources::read(uri, store.as_deref()).is_err() {
                return send_line(
                    output,
                    &rpc_error(id, RESOURCE_NOT_FOUND, "Resource not found"),
                );
            }
            // An unbounded set is an unbounded per-poll cost, and this cap is
            // the only backpressure the design has.
            if !live.subscriptions.watches(uri)
                && live.subscriptions.len() >= subscribe::MAX_SUBSCRIPTIONS
            {
                return send_line(
                    output,
                    &rpc_error(
                        id,
                        INVALID_PARAMS,
                        &format!(
                            "a session may watch at most {} resources; unsubscribe one first",
                            subscribe::MAX_SUBSCRIPTIONS
                        ),
                    ),
                );
            }
            live.subscriptions.subscribe(uri);
            // The feed starts on the first successful subscription and not
            // before. It is never stopped when the last one goes: a session
            // that unsubscribes and resubscribes is common, and a parked feed
            // costs one integer comparison per interval.
            live.ensure_feed(store.as_ref().map(|st| st.data_dir.clone()), output);
            send_line(output, &result_response(id, Value::Obj(Default::default())))
        }
        "resources/unsubscribe" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if uri.is_empty() {
                return send_line(output, &rpc_error(id, INVALID_PARAMS, "uri is required"));
            }
            live.subscriptions.unsubscribe(uri);
            send_line(output, &result_response(id, Value::Obj(Default::default())))
        }
        "logging/setLevel" => {
            let named = params
                .as_ref()
                .and_then(|p| p.get("level"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match logging::Level::parse(named) {
                Some(level) => {
                    live.level = Some(level);
                    send_line(output, &result_response(id, Value::Obj(Default::default())))
                }
                None => send_line(
                    output,
                    &rpc_error(
                        id,
                        INVALID_PARAMS,
                        &format!("level must be one of {}", logging::Level::names()),
                    ),
                ),
            }
        }
        "resources/read" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match super::resources::read(uri, store.as_deref()) {
                Ok(v) => send_line(output, &result_response(id, v)),
                Err(_) => send_line(
                    output,
                    &rpc_error(id, RESOURCE_NOT_FOUND, "Resource not found"),
                ),
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
            let raw_args = params.as_ref().and_then(|p| p.get("arguments"));
            if raw_args.is_some() && raw_args.and_then(Value::as_obj).is_none() {
                return send_line(
                    output,
                    &rpc_error(id, INVALID_PARAMS, "arguments must be an object"),
                );
            }
            let args = raw_args.cloned().unwrap_or(Value::Obj(Default::default()));
            // What the call knows about its own request: the writer, the id
            // a cancellation would name, and the `_meta` a progress token
            // lives in. Threaded now; `6002` and `6003` are the consumers.
            let ctx = tools::ToolCtx {
                notifier: Some(output),
                request_id: Some(id.clone()),
                meta: params.as_ref().and_then(|p| p.get("_meta")).cloned(),
                cancel: live.cancellations.flag(&id),
                // Both halves of the session, for the one tool that will ask
                // the client a question and wait for the answer. Unused until
                // `6401`; provided here so that task never touches this loop.
                io,
                client_elicitation: live.client_elicitation,
            };
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
                let called = match (store, live.degraded_dir.clone()) {
                    (Some(st), _) => tools::dispatch_with(st, clock, name, &args, &ctx),
                    // Degraded: the diagnostic tools answer from the
                    // directory itself, and everything else is refused with
                    // the diagnosis rather than with "unavailable".
                    (None, Some(data_dir)) => {
                        tools::dispatch_degraded(&data_dir, clock, name, &args, &ctx)
                    }
                    (None, None) => Err(ErrorObj::new("io/read", "no store")),
                };
                match called {
                    Ok(v) => send_line(output, &result_response(id, tool_ok(name, v))),
                    Err(e) => send_line(output, &result_response(id, tool_error(&e))),
                }
            }
        }
        other => send_line(
            output,
            &rpc_error(id, METHOD_NOT_FOUND, &format!("method not found: {other}")),
        ),
    }
}

/// The sentence appended to `instructions` when this server is not the plain
/// writer, so a model can tell what it is allowed to do here.
///
/// The default mode adds nothing at all: the instructions are part of a
/// byte-compared transcript, and a mode that changes them would move that
/// golden for every existing deployment.
fn mode_note(store: Option<&Store>, embedded: bool, degraded: bool) -> &'static str {
    if degraded {
        "\n\nThis server could not open its store (mode=degraded): every tool that reads or writes instances is refused, and each refusal carries the health, the blast radius, and the remedy. Call store_doctor for the diagnosis; journal_verify and journal_replay also answer, a machine_create with dry_run still validates, and the documentation resources still read."
    } else if store.is_some_and(|store| store.journal.is_read_only()) {
        "\n\nThis server is running read-only (mode=read-only): the effect executor owns the writer, so machine_create, instance_create, instance_send, deadline_poll, effect_ack, and instance_cancel are refused here. Read tools work normally, and a machine_create with dry_run still validates."
    } else if embedded {
        "\n\nThis server runs the effect executor inline (mode=embedded): handlers run on this thread, one tick per request you send, so a workflow advances while you are talking to it and pauses when you stop."
    } else {
        ""
    }
}

fn initialize_result(version: &str, mode_note: &'static str) -> Value {
    let mut tools = std::collections::BTreeMap::new();
    tools.insert("listChanged".into(), Value::Bool(false));
    let mut resources = std::collections::BTreeMap::new();
    resources.insert("subscribe".into(), Value::Bool(true));
    resources.insert("listChanged".into(), Value::Bool(true));
    let mut prompts = std::collections::BTreeMap::new();
    prompts.insert("listChanged".into(), Value::Bool(false));
    let mut caps = std::collections::BTreeMap::new();
    // `tools` and `prompts` stay false because both sets are static: a
    // per-machine tool surface would make `tools/list` depend on store
    // contents, and no client is obliged to re-read a list that cannot
    // change. `resources` is the opposite — an instance is a live object.
    caps.insert("tools".into(), Value::Obj(tools));
    caps.insert("resources".into(), Value::Obj(resources));
    caps.insert("prompts".into(), Value::Obj(prompts));
    caps.insert(
        "logging".into(),
        Value::Obj(std::collections::BTreeMap::new()),
    );
    // Completion has no options to negotiate: either a server can spell its
    // own identifiers or it cannot.
    caps.insert(
        "completions".into(),
        Value::Obj(std::collections::BTreeMap::new()),
    );
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
        Value::Str(format!("{}{mode_note}", super::prompts::INSTRUCTIONS)),
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

/// Every write in this file goes through the session's one notifier, which
/// holds a lock across the whole line. Nothing else in the process writes to
/// the protocol stream.
fn send_line(out: &Notifier, v: &Value) -> std::io::Result<()> {
    out.send(v)
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
