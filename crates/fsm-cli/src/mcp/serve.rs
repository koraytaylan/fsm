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
    INVALID_REQUEST, Incoming, PARSE_ERROR, WireError, error_response, parse_line,
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
    let mut contended = false;
    let opened = match &mode {
        // A read-only open takes no lock and creates nothing, which is what
        // lets this process watch a data directory the executor is writing.
        ServeMode::ReadOnly => Store::open_read_only(dir),
        ServeMode::Writer | ServeMode::Embedded(_) => match open_writer(dir) {
            Ok(store) => Ok(store),
            Err(WriterUnavailable::Contended(store)) => {
                // Healthy and busy, which is not the same thing as broken.
                contended = true;
                Ok(*store)
            }
            Err(WriterUnavailable::Unhealthy(error)) => Err(*error),
        },
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
    let reported_mode = match (&degraded, contended) {
        (Some(_), _) => "degraded",
        (None, true) => "read-only (writer held elsewhere)",
        (None, false) => mode_name(&mode),
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
    // A contended store is reported the way a degraded one is — through the
    // same slot, because "unavailable" has two reasons and one of them is
    // not a fault. The words differ because the remedies do: one is "run
    // repair after a human looks", the other is "stop the other writer, or
    // use the paired deployment".
    let unavailable = match (&degraded, contended) {
        (Some(detail), _) => Some(Unavailable {
            contended: false,
            detail,
            data_dir: Some(dir),
        }),
        (None, true) => Some(Unavailable {
            contended: true,
            detail: CONTENDED_DETAIL,
            data_dir: None,
        }),
        (None, false) => None,
    };
    serve_session_degraded(
        store.as_mut(),
        &mut clock,
        executor.as_deref_mut(),
        refresh.as_deref(),
        unavailable,
        input,
        output,
    )
}

/// Why this session has less than a writer, and what to tell a client.
///
/// Two reasons, one slot. A **degraded** store is unhealthy: the remedy is a
/// diagnosis and, after a person looks, a repair. A **contended** one is
/// healthy and busy: the remedy is to stop the other writer, or to use the
/// paired deployment where the executor writes and this server watches. One
/// enum with two reasons is easier to hold in your head than two enums, and
/// the words differ because the remedies do.
pub struct Unavailable<'a> {
    /// True when somebody else holds the writer; false when the store is
    /// unhealthy.
    pub contended: bool,
    pub detail: &'a str,
    /// The directory to diagnose, for the unhealthy case.
    pub data_dir: Option<&'a std::path::Path>,
}

/// What a contended session tells its client.
pub const CONTENDED_DETAIL: &str = "another process holds the writer; this session is read-only. Stop that writer, or use the paired deployment: the executor writes and this server watches.";

/// Why a writable open did not produce a writer.
enum WriterUnavailable {
    /// Somebody else holds the lock: the store is **healthy and busy**.
    /// A read-only handle comes back instead, boxed because a `Store` is
    /// large and an enum is as big as its largest variant.
    Contended(Box<Store>),
    /// The store itself will not open, which is plan 0014's degraded mode.
    Unhealthy(Box<ErrorObj>),
}

/// How many times a contended writer is retried, and how long between.
///
/// The executor takes and releases the writer once a tick, so a collision at
/// startup is expected rather than fatal — but a server that retried forever
/// would hang instead of telling anybody, so the window is small and then it
/// is over.
const WRITER_ATTEMPTS: u32 = 5;
const WRITER_BACKOFF_MS: u64 = 400;

/// Take the writer, waiting briefly for a holder to finish, and fall back to
/// a read-only handle rather than to nothing.
///
/// Exiting here was the old behaviour and it was the wrong one: a client
/// saw a server that never appeared. A contended store is *healthy and
/// busy*, so the session starts, says so, and refuses writes with a message
/// naming the holder — a different problem from plan 0014's unhealthy store,
/// with a completely different remedy.
fn open_writer(dir: &std::path::Path) -> Result<Store, WriterUnavailable> {
    let mut last = None;
    for attempt in 0..WRITER_ATTEMPTS {
        match Store::open(dir) {
            Ok(store) => return Ok(store),
            Err(error) if error.code == "store/lock" => {
                last = Some(error);
                if attempt + 1 < WRITER_ATTEMPTS {
                    std::thread::sleep(std::time::Duration::from_millis(WRITER_BACKOFF_MS));
                }
            }
            Err(error) => return Err(WriterUnavailable::Unhealthy(Box::new(error))),
        }
    }
    // The window is over. Read-only is a working server; no server is not.
    // And this does not upgrade later: a session that silently became the
    // writer halfway through would surprise both writers, so a client that
    // wants the writer restarts.
    match Store::open_read_only(dir) {
        Ok(store) => Err(WriterUnavailable::Contended(Box::new(store))),
        Err(error) => Err(WriterUnavailable::Unhealthy(Box::new(
            last.unwrap_or(error),
        ))),
    }
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
///
/// Public because a second transport needs somewhere to keep it. Over stdio
/// the session is the process and this lives in the loop; over HTTP it lives
/// beside a session id. The protocol above does not know the difference,
/// which is the whole reason plan 0015 adds a transport rather than a
/// server.
#[derive(Default)]
pub struct Live {
    pub subscriptions: subscribe::Subscriptions,
    pub level: Option<logging::Level>,
    pub cancellations: cancel::Cancellations,
    /// Whether the client advertised `elicitation` at `initialize`. Captured
    /// here because `initialize` is the only place it appears; consulted by
    /// the tool that would otherwise ask a client that cannot answer.
    pub client_elicitation: bool,
    /// Why the store would not open, when it would not. A client hears this
    /// once, at `error` level, as soon as it is allowed to hear anything.
    pub degraded: Option<String>,
    /// The directory to diagnose while degraded. Every answer a degraded
    /// session gives is a function of this path, because there is no store.
    pub degraded_dir: Option<std::path::PathBuf>,
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
    pub(crate) fn ensure_feed(&mut self, data_dir: Option<std::path::PathBuf>, output: &Notifier) {
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
    serve_session_degraded(store, clock, executor, refresh, None, input, output)
}

/// The protocol loop, with one more thing it may have to say: that the store
/// it was pointed at would not open.
#[allow(clippy::too_many_arguments)]
pub fn serve_session_degraded(
    mut store: Option<&mut Store>,
    clock: &mut dyn Clock,
    mut executor: Option<&mut ExecutorLoop>,
    refresh: Option<&std::path::Path>,
    unavailable: Option<Unavailable<'_>>,
    mut input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    let degraded = unavailable
        .as_ref()
        .filter(|state| !state.contended)
        .map(|state| state.detail);
    let degraded_dir = unavailable.as_ref().and_then(|state| state.data_dir);
    let contended = unavailable.as_ref().is_some_and(|state| state.contended);
    let detail = unavailable.as_ref().map(|state| state.detail.to_string());
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
    let mode_note = mode_note(
        store.as_deref(),
        executor.is_some(),
        degraded.is_some(),
        contended,
    );
    let mut initialized = false;
    let mut initialized_notified = false;
    let mut live = Live {
        // Both reasons a store can be unavailable travel here, because a
        // client needs to hear either one; only the *words* differ, and they
        // differ because the remedies do.
        degraded: detail,
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
                        crate::mcp::methods::handle_request(
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
                            // stdout is both the answer and the stream, so
                            // the feed writes where everything else does.
                            None,
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

/// The sentence appended to `instructions` when this server is not the plain
/// writer, so a model can tell what it is allowed to do here.
///
/// The default mode adds nothing at all: the instructions are part of a
/// byte-compared transcript, and a mode that changes them would move that
/// golden for every existing deployment.
fn mode_note(
    store: Option<&Store>,
    embedded: bool,
    degraded: bool,
    contended: bool,
) -> &'static str {
    if contended {
        "\n\nThis server could not take the writer because another process holds it (mode=read-only, contended): the store is healthy and busy, not broken. Read tools work normally and writes are refused. Stop the other writer, or use the paired deployment where the executor writes and this server watches."
    } else if degraded {
        "\n\nThis server could not open its store (mode=degraded): every tool that reads or writes instances is refused, and each refusal carries the health, the blast radius, and the remedy. Call store_doctor for the diagnosis; journal_verify and journal_replay also answer, a machine_create with dry_run still validates, and the documentation resources still read."
    } else if store.is_some_and(|store| store.journal.is_read_only()) {
        "\n\nThis server is running read-only (mode=read-only): the effect executor owns the writer, so machine_create, instance_create, instance_send, deadline_poll, effect_ack, and instance_cancel are refused here. Read tools work normally, and a machine_create with dry_run still validates."
    } else if embedded {
        "\n\nThis server runs the effect executor inline (mode=embedded): handlers run on this thread, one tick per request you send, so a workflow advances while you are talking to it and pauses when you stop."
    } else {
        ""
    }
}

pub(crate) fn initialize_result(version: &str, mode_note: &'static str) -> Value {
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

pub(crate) fn fsm_ping_result() -> Value {
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
pub(crate) fn send_line(out: &Notifier, v: &Value) -> std::io::Result<()> {
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
