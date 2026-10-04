//! Falling back to inspection must stop execution and keep the view fresh.

use std::collections::BTreeMap;
use std::io::{self, BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::{ExecutorLoop, ServeMode, serve_dir_with, serve_session_with};
use fsm_cli::store::Store;
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::config::HandlerTable;
use fsm_store::clock::FixedClock;

const MACHINE: &str = r#"{
  "format":"fsm.machine/1","name":"readonly_effect",
  "states":[
    {"name":"waiting","entry":{"emit":[{"effect":"notify","args":{}}]}},
    {"name":"advanced","terminal":true}
  ],
  "initial":"waiting","context":[],
  "effects":[{"name":"notify","fields":[]}],
  "events":[{"name":"finish","fields":[]}],
  "transitions":[{"from":"waiting","on":"finish","to":"advanced"}]
}"#;

const HELLO: &str = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n";
const GET: &str = "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"instance_get\",\"arguments\":{\"instance_id\":\"instance\"}}}\n";

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "fsm-embedded-readonly-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create scratch: {error}"),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn seeded(path: &Path) -> Store {
    let mut store = Store::open(path).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "readonly_effect",
            "instance",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    assert_eq!(store.state.instances["instance"].pending.len(), 1);
    store
}

fn executor(path: &Path) -> ExecutorLoop {
    // A real portable subprocess, with no shell or platform-specific command.
    let mut table = value(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":[],"timeout_ms":30000}]}"#,
    );
    if let Value::Obj(root) = &mut table
        && let Some(Value::Arr(handlers)) = root.get_mut("handlers")
        && let Value::Obj(handler) = &mut handlers[0]
    {
        handler.insert(
            "argv".into(),
            Value::Arr(vec![
                Value::Str(env!("CARGO_BIN_EXE_fsm").into()),
                Value::Str("--version".into()),
            ]),
        );
    }
    let source = String::from_utf8(canon_bytes(&table)).unwrap();
    ExecutorLoop::new(path, HandlerTable::parse(&source).unwrap()).unwrap()
}

fn assert_no_handler_started(sink: &SharedSink) {
    let stream = sink.text();
    assert!(
        !stream.contains("observed pending") && !stream.contains("spawned handler"),
        "read-only sessions must not start an external command: {stream}"
    );
}

#[test]
fn contended_embedded_server_never_starts_handlers() {
    let directory = Scratch::new();
    let holder = seeded(&directory.0);
    let sink = SharedSink::new();
    serve_dir_with(
        &directory.0,
        ServeMode::Embedded(Box::new(executor(&directory.0))),
        Cursor::new(format!("{HELLO}{GET}")),
        sink.writer(),
    )
    .unwrap();
    assert!(sink.text().contains("contended"), "{}", sink.text());
    assert_no_handler_started(&sink);
    assert_eq!(holder.state.instances["instance"].pending.len(), 1);
}

#[test]
fn a_direct_read_only_session_cannot_run_an_executor() {
    let directory = Scratch::new();
    let holder = seeded(&directory.0);
    let mut reader = Store::open_read_only(&directory.0).unwrap();
    let mut executor = executor(&directory.0);
    let sink = SharedSink::new();
    serve_session_with(
        Some(&mut reader),
        &mut FixedClock::new(1_000, 1),
        Some(&mut executor),
        None,
        Cursor::new(format!("{HELLO}{GET}")),
        sink.writer(),
    )
    .unwrap();
    assert_no_handler_started(&sink);
    assert_eq!(holder.state.instances["instance"].pending.len(), 1);
}

#[test]
fn the_same_handler_starts_when_the_session_owns_a_writer() {
    let directory = Scratch::new();
    let mut store = seeded(&directory.0);
    let mut executor = executor(&directory.0);
    let sink = SharedSink::new();
    serve_session_with(
        Some(&mut store),
        &mut FixedClock::new(1_000, 1),
        Some(&mut executor),
        None,
        Cursor::new(HELLO),
        sink.writer(),
    )
    .unwrap();
    assert!(sink.text().contains("spawned handler"), "{}", sink.text());
}

/// Perform an external write only after initialize has been answered, before
/// the next client request. This proves refresh without a timing race.
struct BetweenRequests<F> {
    first: Cursor<Vec<u8>>,
    second: Cursor<Vec<u8>>,
    change: Option<F>,
}

impl<F: FnOnce()> Read for BetweenRequests<F> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let count = self.first.read(buffer)?;
        if count != 0 {
            return Ok(count);
        }
        if let Some(change) = self.change.take() {
            change();
        }
        self.second.read(buffer)
    }
}

#[test]
fn contended_writer_and_embedded_sessions_refresh_the_journal() {
    for embedded in [false, true] {
        let directory = Scratch::new();
        let mut holder = seeded(&directory.0);
        let mode = if embedded {
            ServeMode::Embedded(Box::new(executor(&directory.0)))
        } else {
            ServeMode::Writer
        };
        let sink = SharedSink::new();
        let input = BetweenRequests {
            first: Cursor::new(HELLO.as_bytes().to_vec()),
            second: Cursor::new(GET.as_bytes().to_vec()),
            change: Some(|| {
                assert!(sink.text().contains("protocolVersion"));
                holder
                    .send_event(
                        "instance",
                        "finish",
                        Value::Obj(BTreeMap::new()),
                        "external-finish",
                        None,
                    )
                    .unwrap();
            }),
        };
        serve_dir_with(&directory.0, mode, BufReader::new(input), sink.writer()).unwrap();
        let reply = sink
            .text()
            .lines()
            .map(value)
            .find(|message| message.get("id").and_then(Value::as_num) == Some("2"))
            .unwrap();
        let configuration = reply
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .and_then(|result| result.get("configuration"))
            .unwrap();
        assert_eq!(
            configuration.get("leaf").and_then(Value::as_str),
            Some("advanced"),
            "the fallback view must include the external write (embedded={embedded})"
        );
        assert_no_handler_started(&sink);
    }
}
