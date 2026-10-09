//! Real stdio discovery and read-only draft checks, independent of native provisioning.

#[cfg(target_os = "linux")]
#[path = "contract_mcp/native.rs"]
mod native;

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires disposable native CI, protected original operator table and exact staged CLI/test artifacts"]
fn native_draft_repair_execution() {
    native::run();
}

use fsm_cli::{
    clock::FixedClock,
    mcp::tools::{registry, validate_args},
    store::Store,
};
use fsm_core::{
    canon::canon_bytes,
    json::{JsonLimits, Value, parse},
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("fsm-contract-mcp-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Client {
    fn start(path: &std::path::Path, read_only: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
        command.stderr(Stdio::null());
        command.arg("--data-dir").arg(path).arg("serve");
        if read_only {
            command.arg("--read-only");
        }
        Self::spawn(command)
    }

    fn spawn(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (send, replies) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Ok(value) = parse(line.as_bytes(), &JsonLimits::DEFAULT)
                    && send.send(value).is_err()
                {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input: Some(input),
            replies,
            reader: Some(reader),
        };
        client.call(
            1,
            "initialize",
            obj([("protocolVersion", Value::Str("2025-06-18".into()))]),
        );
        client
            .input
            .as_mut()
            .unwrap()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .unwrap();
        client.input.as_mut().unwrap().flush().unwrap();
        client
    }
    fn call(&mut self, id: u64, method: &str, parameters: Value) -> Value {
        let request = obj([
            ("jsonrpc", Value::Str("2.0".into())),
            ("id", Value::Num(id.to_string())),
            ("method", Value::Str(method.into())),
            ("params", parameters),
        ]);
        self.input
            .as_mut()
            .unwrap()
            .write_all(&canon_bytes(&request))
            .unwrap();
        self.input.as_mut().unwrap().write_all(b"\n").unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let reply = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .expect("bounded MCP response");
            if reply.get("id") == Some(&Value::Num(id.to_string())) {
                return reply
                    .get("result")
                    .expect("successful RPC envelope")
                    .clone();
            }
        }
    }
    fn check(&mut self, id: u64, arguments: Value) -> Value {
        let result = self.call(
            id,
            "tools/call",
            obj([
                ("name", Value::Str("executor_check".into())),
                ("arguments", arguments),
            ]),
        );
        assert_ne!(
            result.get("isError"),
            Some(&Value::Bool(true)),
            "{result:?}"
        );
        result.get("structuredContent").unwrap().clone()
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn obj<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn draft() -> Value {
    parse(
        include_bytes!("fixtures/contract/draft.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}
fn files(path: &std::path::Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else if path.file_name().unwrap() != "LOCK" {
            result.insert(path.clone(), std::fs::read(path).unwrap());
        }
    }
    result
}

#[test]
fn stdio_discovers_checks_repairs_and_creates_without_check_writes() {
    let directory = Directory::new();
    let data = directory.0.join("data");
    let store = Store::open(&data).unwrap();
    let before = store.records.clone();
    drop(store);
    let mut client = Client::start(&data, false);
    let tools = client.call(2, "tools/list", obj([]));
    let tool = tools
        .get("tools")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("executor_check"))
        .unwrap();
    assert_eq!(
        tool.get("annotations").unwrap().get("readOnlyHint"),
        Some(&Value::Bool(true))
    );
    let baseline = files(&data);
    let mut invalid = draft();
    let Value::Obj(fields) = &mut invalid else {
        unreachable!()
    };
    fields.insert("initial".into(), Value::Str("absent".into()));
    let invalid_report = client.check(3, obj([("spec", invalid)]));
    assert_eq!(
        invalid_report.get("status").and_then(Value::as_str),
        Some("invalid")
    );
    assert_eq!(invalid_report.get("machine_id"), Some(&Value::Null));
    let repaired = client.check(4, obj([("spec", draft())]));
    assert_eq!(
        repaired.get("status").and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(repaired.get("contract_id"), Some(&Value::Null));
    let schema = tool.get("outputSchema").unwrap();
    validate_args(schema, &invalid_report).unwrap();
    validate_args(schema, &repaired).unwrap();
    assert_eq!(files(&data), baseline);
    assert_eq!(Store::open_read_only(&data).unwrap().records, before);
    assert!(
        Store::open_read_only(&data)
            .unwrap()
            .state
            .machines
            .is_empty()
    );
    let created = client.call(
        5,
        "tools/call",
        obj([
            ("name", Value::Str("machine_create".into())),
            ("arguments", obj([("spec", draft())])),
        ]),
    );
    assert_ne!(created.get("isError"), Some(&Value::Bool(true)));
    let stored = client.check(6, obj([("machine", Value::Str("simple".into()))]));
    assert_eq!(stored, repaired);
    let invalid_arguments = client.call(
        7,
        "tools/call",
        obj([
            ("name", Value::Str("executor_check".into())),
            (
                "arguments",
                obj([
                    ("spec", draft()),
                    ("handlers", Value::Str("PRIVATE_OVERRIDE".into())),
                ]),
            ),
        ]),
    );
    assert_eq!(invalid_arguments.get("isError"), Some(&Value::Bool(true)));
    assert!(
        !String::from_utf8(canon_bytes(&invalid_arguments))
            .unwrap()
            .contains("PRIVATE_OVERRIDE")
    );
    assert_eq!(
        Store::open_read_only(&data).unwrap().state.machines.len(),
        1
    );
}

#[test]
fn read_only_stdio_checks_stored_definition_while_writer_is_held() {
    let directory = Directory::new();
    let data = directory.0.join("data");
    let mut writer = Store::open(&data).unwrap();
    writer
        .define_machine_on(&mut FixedClock::new(2000, 0), draft(), false, false)
        .unwrap();
    let records = writer.records.clone();
    let baseline = files(&data);
    let mut client = Client::start(&data, true);
    let result = client.check(2, obj([("machine", Value::Str("simple".into()))]));
    assert_eq!(
        result.get("status").and_then(Value::as_str),
        Some("unknown")
    );
    assert!(
        result
            .get("findings")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .any(|finding| finding
                .get("cause")
                .and_then(|cause| cause.get("mode"))
                .and_then(Value::as_str)
                == Some("read-only"))
    );
    assert_eq!(files(&data), baseline);
    assert_eq!(Store::open_read_only(&data).unwrap().records, records);
}

#[test]
fn closed_schema_refuses_ambiguous_selectors_and_private_overrides() {
    let tool = registry()
        .into_iter()
        .find(|tool| tool.name == "executor_check")
        .unwrap();
    let schema = (tool.input_schema)();
    let golden = parse(
        include_bytes!("fixtures/contract/executor.tool.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(&schema, golden.get("inputSchema").unwrap());
    assert_eq!(&(tool.output_schema)(), golden.get("outputSchema").unwrap());
    for invalid in [
        obj([]),
        obj([("spec", draft()), ("machine", Value::Str("simple".into()))]),
        obj([("machine", Value::Str(String::new()))]),
        obj([
            ("spec", draft()),
            ("handlers", Value::Str("PRIVATE_OVERRIDE".into())),
        ]),
    ] {
        let error = validate_args(&schema, &invalid).unwrap_err();
        assert!(
            !String::from_utf8(canon_bytes(&error.to_value()))
                .unwrap()
                .contains("PRIVATE_OVERRIDE")
        );
    }
    validate_args(&schema, &obj([("spec", draft())])).unwrap();
    validate_args(&schema, &obj([("machine", Value::Str("simple".into()))])).unwrap();
}
