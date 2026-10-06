//! Discovery uses the active executor table, not a guessed deployment mode.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::{ExecutorLoop, serve_session_with};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::config::HandlerTable;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fsm-mcp-executor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn discover(store: Option<&mut Store>, executor: Option<&mut ExecutorLoop>) -> Value {
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"resources/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"resources/read\",\"params\":{\"uri\":\"fsm://executor\"}}\n"
    );
    let sink = SharedSink::new();
    serve_session_with(
        store,
        &mut FixedClock::new(1_000, 0),
        executor,
        None,
        Cursor::new(input),
        sink.writer(),
    )
    .unwrap();
    let replies: Vec<Value> = sink
        .text()
        .lines()
        .map(value)
        .filter(|reply| reply.get("id").is_some())
        .collect();
    let instructions = replies[0]
        .get("result")
        .unwrap()
        .get("instructions")
        .unwrap()
        .as_str()
        .unwrap();
    assert!(instructions.contains("fsm://executor"));
    let resources = replies[1]
        .get("result")
        .unwrap()
        .get("resources")
        .unwrap()
        .as_arr()
        .unwrap();
    assert!(
        resources
            .iter()
            .any(|resource| resource.get("uri").and_then(Value::as_str) == Some("fsm://executor"))
    );
    let content = &replies[2]
        .get("result")
        .unwrap()
        .get("contents")
        .unwrap()
        .as_arr()
        .unwrap()[0];
    assert_eq!(
        content.get("mimeType").and_then(Value::as_str),
        Some("application/json")
    );
    value(content.get("text").unwrap().as_str().unwrap())
}

#[test]
fn embedded_discloses_the_loaded_contract_without_command_secrets() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    // No effects are queued: the command is deliberately never executed.
    let binary = Value::Str(
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    let table = format!(
        r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"process_item","kind":"mcp","argv":[{},"--secret=private-token","{{workspace}}"],"tool":"process","arguments":{{"nested":["{{item}}","{{workspace}}"]}},"timeout_ms":9000,"on_ok":{{"event":"processed","payload":{{"verified":"true"}},"stamps":["at"]}},"on_failed":{{"event":"processing_failed"}},"retry":{{"attempts":2}}}}]}}"#,
        String::from_utf8(fsm_core::canon::canon_bytes(&binary)).unwrap()
    );
    let mut executor =
        ExecutorLoop::new(&directory.0, HandlerTable::parse(&table).unwrap()).unwrap();
    let report = discover(Some(&mut store), Some(&mut executor));
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("embedded"));
    assert_eq!(report.get("executes_effects"), Some(&Value::Bool(true)));
    assert_eq!(
        report.get("progress").and_then(Value::as_str),
        Some("client_requests")
    );
    let handler = &report.get("handlers").unwrap().as_arr().unwrap()[0];
    assert_eq!(
        handler.get("effect").and_then(Value::as_str),
        Some("process_item")
    );
    assert_eq!(handler.get("kind").and_then(Value::as_str), Some("mcp"));
    assert_eq!(
        handler.get("required_args"),
        Some(&value(r#"["item","workspace"]"#))
    );
    assert_eq!(
        handler.get("on_ok"),
        Some(&value(
            r#"{"event":"processed","payload":{"verified":"true"},"stamps":["at"]}"#
        ))
    );
    assert_eq!(
        handler
            .get("on_failed")
            .unwrap()
            .get("event")
            .and_then(Value::as_str),
        Some("processing_failed")
    );
    assert_eq!(
        handler
            .get("retry")
            .unwrap()
            .get("attempts")
            .and_then(Value::as_num),
        Some("2")
    );
    let serialized = String::from_utf8(fsm_core::canon::canon_bytes(&report)).unwrap();
    assert!(!serialized.contains("private-token"));
    assert!(!serialized.contains("argv"));
    assert!(serialized.contains("Subscribing alone does not advance"));
}

#[test]
fn writer_read_only_and_degraded_do_not_invent_an_executor() {
    let directory = Directory::new();
    let mut writer = Store::open(&directory.0).unwrap();
    let report = discover(Some(&mut writer), None);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("writer"));
    assert_eq!(report.get("executes_effects"), Some(&Value::Bool(false)));
    assert_eq!(report.get("handlers"), Some(&value("[]")));
    assert_eq!(
        report.get("external_executor").and_then(Value::as_str),
        Some("unknown")
    );
    let mut reader = Store::open_read_only(&directory.0).unwrap();
    let report = discover(Some(&mut reader), None);
    assert_eq!(
        report.get("mode").and_then(Value::as_str),
        Some("read-only")
    );
    assert_eq!(report.get("handlers"), Some(&Value::Null));
    assert_eq!(
        report.get("external_executor").and_then(Value::as_str),
        Some("unknown")
    );
    let report = discover(None, None);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("degraded"));
    assert_eq!(report.get("handlers"), Some(&Value::Null));
}

#[test]
fn durable_ownership_counts_are_read_only_and_do_not_invent_live_health() {
    use fsm_core::record::execution::{NativeDomain, RetryPolicy};
    use fsm_store::store::ExecutionClaimRequest;
    let directory = Directory::new();
    let mut writer = Store::open(&directory.0).unwrap();
    assert_eq!(
        discover(Some(&mut writer), None).get("execution_ownership"),
        Some(&value(
            r#"{"enabled":true,"unresolved_runs":0,"stopped_runs":0,"outstanding_handoffs":0}"#
        ))
    );
    writer
        .define_machine(
            parse(
                include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
            false,
            false,
        )
        .unwrap();
    writer
        .create_instance("case_review", "instance", "create", None)
        .unwrap();
    writer
        .send_event("instance", "docs_ok", value("{}"), "send", None)
        .unwrap();
    let effect = writer.state.instances["instance"].pending[0].clone();
    let domain = NativeDomain::from_value(&value(r#"{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}"#)).unwrap();
    let retry = RetryPolicy::from_value(&value(
        r#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]}"#,
    ))
    .unwrap();
    writer.claim_execution_on(&mut FixedClock::new(100, 1), ExecutionClaimRequest {
        instance_id: "instance", effect_id: &effect,
        handler_fingerprint: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        retry: &retry, domain: &domain, request_id: "claim", expected_seq: None,
    }).unwrap();
    fn files(path: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        let mut result = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                result.extend(files(&entry.path()));
            } else {
                result.insert(entry.path(), std::fs::read(entry.path()).unwrap());
            }
        }
        result
    }
    let before = files(&directory.0);
    let mut reader = Store::open_read_only(&directory.0).unwrap();
    let report = discover(Some(&mut reader), None);
    assert_eq!(
        report.get("execution_ownership"),
        Some(&value(
            r#"{"enabled":true,"unresolved_runs":1,"stopped_runs":0,"outstanding_handoffs":0}"#
        ))
    );
    assert_eq!(
        report.get("external_executor"),
        Some(&Value::Str("unknown".into()))
    );
    assert_eq!(files(&directory.0), before);
    let encoded = String::from_utf8(fsm_core::canon::canon_bytes(
        report.get("execution_ownership").unwrap(),
    ))
    .unwrap();
    assert!(!encoded.contains("instance"));
    assert!(!encoded.contains("linux-systemd"));
    assert_eq!(
        discover(None, None).get("execution_ownership"),
        Some(&Value::Null)
    );
}
