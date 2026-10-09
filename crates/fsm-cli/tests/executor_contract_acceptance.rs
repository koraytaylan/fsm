//! SPEC-authored staged reports; diagnostic parity does not prove native effects.
use fsm_core::{
    canon::canon_bytes,
    json::{JsonLimits, Value, parse},
    spec::compile_accepted,
};
use fsm_store::{clock::FixedClock, store::Store};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const MACHINE: &str = include_str!("fixtures/contract/workflow.machine.json");
const STAMPED: &str = include_str!("fixtures/contract/workflow-stamped.machine.json");
const UNRESOLVED: &str = include_str!("fixtures/contract/workflow-unresolved.machine.json");
struct Case<'a> {
    source: &'a str,
    table: &'a str,
    report: &'a str,
    exit: i32,
}
fn cases() -> [Case<'static>; 5] {
    [
        Case {
            source: MACHINE,
            table: include_str!("fixtures/contract/workflow-repaired.handlers.json"),
            report: include_str!("fixtures/contract/workflow-repaired.report.json"),
            exit: 0,
        },
        Case {
            source: MACHINE,
            table: include_str!("fixtures/contract/workflow-invalid.handlers.json"),
            report: include_str!("fixtures/contract/workflow-invalid.report.json"),
            exit: 1,
        },
        Case {
            source: STAMPED,
            table: include_str!("fixtures/contract/workflow-unknown.handlers.json"),
            report: include_str!("fixtures/contract/workflow-unknown.report.json"),
            exit: 3,
        },
        Case {
            source: MACHINE,
            table: include_str!("fixtures/contract/workflow-manual.handlers.json"),
            report: include_str!("fixtures/contract/workflow-manual.report.json"),
            exit: 0,
        },
        Case {
            source: MACHINE,
            table: include_str!("fixtures/contract/workflow-no-outcome.handlers.json"),
            report: include_str!("fixtures/contract/workflow-no-outcome.report.json"),
            exit: 0,
        },
    ]
}
fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}
fn expected(case: &Case<'_>) -> Value {
    // Only the compiler-derived definition identity is substituted; every
    // finding, type, path, outcome, progress flag and public contract hash was
    // independently authored from SPEC before running the analyzer.
    let machine = compile_accepted(&value(case.source)).unwrap();
    value(&case.report.replace("$MACHINE_ID", &machine.machine_id))
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fsm-contract-acceptance-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn inputs(&self, case: &Case<'_>) {
        fs::write(self.0.join("machine.json"), case.source).unwrap();
        fs::write(self.0.join("handlers.json"), case.table).unwrap();
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn files(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else if path.file_name().unwrap() != "LOCK" {
            result.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    result
}
fn check_cli(directory: &Directory, data: &Path, selector: &str, reference: &str, case: &Case<'_>) {
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(data)
        .args(["execute", "--check", "--handlers"])
        .arg(directory.0.join("handlers.json"))
        .args([selector, reference])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(case.exit), "{output:?}");
    let report = parse(&output.stdout, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(report, expected(case));
    let mut expected_bytes = canon_bytes(&expected(case));
    expected_bytes.push(b'\n');
    assert_eq!(output.stdout, expected_bytes);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_WORKFLOW"));
    assert!(output.stderr.is_empty(), "{output:?}");
}
#[test]
fn offline_and_stored_cli_match_independent_staged_reports_without_writes() {
    for case in cases() {
        let directory = Directory::new();
        directory.inputs(&case);
        let absent = directory.0.join("absent");
        check_cli(
            &directory,
            &absent,
            "--machine-file",
            directory.0.join("machine.json").to_str().unwrap(),
            &case,
        );
        assert!(!absent.exists());
        let data = directory.0.join("data");
        let mut writer = Store::open(&data).unwrap();
        writer
            .define_machine_on(
                &mut FixedClock::new(2000, 0),
                value(case.source),
                false,
                false,
            )
            .unwrap();
        let baseline = files(&data);
        let records = writer.records.clone();
        let machine = compile_accepted(&value(case.source)).unwrap();
        for reference in ["contract_workflow", machine.machine_id.as_str()] {
            check_cli(&directory, &data, "--machine", reference, &case);
            assert_eq!(files(&data), baseline);
            assert_eq!(Store::open_read_only(&data).unwrap().records, records);
        }
    }
}

#[test]
fn offline_unresolved_composition_is_unknown_without_inventing_a_store_definition() {
    let case = Case {
        source: UNRESOLVED,
        table: include_str!("fixtures/contract/workflow-repaired.handlers.json"),
        report: include_str!("fixtures/contract/workflow-unresolved.report.json"),
        exit: 3,
    };
    let directory = Directory::new();
    directory.inputs(&case);
    let data = directory.0.join("absent");
    check_cli(
        &directory,
        &data,
        "--machine-file",
        directory.0.join("machine.json").to_str().unwrap(),
        &case,
    );
    assert!(!data.exists());
}

#[test]
fn complete_stored_catalogue_turns_offline_unknown_composition_into_known_compatibility() {
    let child_source = include_str!("fixtures/contract/workflow-child.machine.json");
    let child = compile_accepted(&value(child_source)).unwrap();
    let digest = child.machine_id.rsplit_once("@sha256:").unwrap().1;
    let source = UNRESOLVED.replace(&"a".repeat(64), digest);
    let table = include_str!("fixtures/contract/workflow-repaired.handlers.json");
    let offline = Case {
        source: &source,
        table,
        report: include_str!("fixtures/contract/workflow-unresolved.report.json"),
        exit: 3,
    };
    let directory = Directory::new();
    directory.inputs(&offline);
    let data = directory.0.join("data");
    check_cli(
        &directory,
        &data,
        "--machine-file",
        directory.0.join("machine.json").to_str().unwrap(),
        &offline,
    );
    assert!(!data.exists());
    let mut writer = Store::open(&data).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    writer
        .define_machine_on(&mut clock, value(child_source), false, false)
        .unwrap();
    writer
        .define_machine_on(&mut clock, value(&source), false, false)
        .unwrap();
    let baseline = files(&data);
    let report = include_str!("fixtures/contract/workflow-resolved.report.json")
        .replace("$CHILD_ID", &child.machine_id);
    let stored = Case {
        source: &source,
        table,
        report: &report,
        exit: 0,
    };
    check_cli(&directory, &data, "--machine", "contract_workflow", &stored);
    assert_eq!(files(&data), baseline);
    assert_eq!(
        Store::open_read_only(&data).unwrap().records,
        writer.records
    );
}

#[test]
fn documented_order_machine_check_is_compatible_without_store_or_supplier_start() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = Directory::new();
    let data = directory.0.join("absent");
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&data)
        .args(["execute", "--check", "--handlers"])
        .arg(root.join("examples/order_lifecycle.handlers.json"))
        .arg("--machine-file")
        .arg(root.join("examples/order_lifecycle.json"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let report = parse(&output.stdout, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        report.get("format").and_then(Value::as_str),
        Some("fsm.executor-check/1")
    );
    assert_eq!(
        report.get("status").and_then(Value::as_str),
        Some("compatible")
    );
    assert_eq!(
        report.get("scope").unwrap().get("outcomes_checked"),
        Some(&Value::Bool(true))
    );
    assert!(
        report
            .get("progress")
            .unwrap()
            .as_arr()
            .unwrap()
            .contains(&Value::Str("runtime-dependent".into()))
    );
    assert!(!data.exists());
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod embedded {
    use super::*;
    use fsm_core::canon::canon_bytes;
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, ChildStdin, Stdio},
        sync::mpsc,
        time::Duration,
    };
    fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
        Value::Obj(
            fields
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }
    struct Client {
        child: Child,
        input: Option<ChildStdin>,
        replies: mpsc::Receiver<Value>,
        reader: Option<std::thread::JoinHandle<()>>,
    }
    impl Client {
        fn start(directory: &Directory, data: &Path) -> Self {
            let mut child = Command::new(env!("CARGO_BIN_EXE_fsm"))
                .arg("--data-dir")
                .arg(data)
                .args(["serve", "--execute", "--handlers"])
                .arg(directory.0.join("handlers.json"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let input = child.stdin.take().unwrap();
            let output = child.stdout.take().unwrap();
            let (send, replies) = mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(output).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(reply) = parse(line.as_bytes(), &JsonLimits::DEFAULT)
                        && send.send(reply).is_err()
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
                object([("protocolVersion", Value::Str("2025-06-18".into()))]),
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
        fn call(&mut self, id: u64, method: &str, params: Value) -> Value {
            let request = object([
                ("jsonrpc", Value::Str("2.0".into())),
                ("id", Value::Num(id.to_string())),
                ("method", Value::Str(method.into())),
                ("params", params),
            ]);
            let input = self.input.as_mut().unwrap();
            input.write_all(&canon_bytes(&request)).unwrap();
            input.write_all(b"\n").unwrap();
            input.flush().unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                let reply = self
                    .replies
                    .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                    .expect("bounded original MCP reply");
                if reply.get("id") == Some(&Value::Num(id.to_string())) {
                    return reply
                        .get("result")
                        .expect("successful RPC envelope")
                        .clone();
                }
            }
        }
        fn finish(mut self) {
            self.input.take();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "original MCP host failed during shutdown: {status}"
                    );
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "original MCP host did not release its writer"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            if let Some(reader) = self.reader.take() {
                reader.join().unwrap();
            }
        }
        fn check(&mut self, id: u64, arguments: Value) -> Value {
            let result = self.call(
                id,
                "tools/call",
                object([
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
            self.input.take();
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while self.child.try_wait().ok().flatten().is_none()
                && std::time::Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(reader) = self.reader.take() {
                let _ = reader.join();
            }
        }
    }
    #[test]
    fn original_embedded_mcp_draft_and_stored_checks_match_independent_staged_reports() {
        for case in cases() {
            let directory = Directory::new();
            directory.inputs(&case);
            let data = directory.0.join("data");
            let mut store = Store::open(&data).unwrap();
            store
                .define_machine_on(
                    &mut FixedClock::new(2000, 0),
                    value(case.source),
                    false,
                    false,
                )
                .unwrap();
            drop(store);
            let mut client = Client::start(&directory, &data);
            let baseline = files(&data);
            let draft = client.check(2, object([("spec", value(case.source))]));
            let stored = client.check(
                3,
                object([("machine", Value::Str("contract_workflow".into()))]),
            );
            assert_eq!(draft, expected(&case));
            assert_eq!(stored, expected(&case));
            for report in [&draft, &stored] {
                assert!(
                    !String::from_utf8(canon_bytes(report))
                        .unwrap()
                        .contains("PRIVATE_WORKFLOW")
                );
            }
            assert_eq!(files(&data), baseline);
            let state = Store::open_read_only(&data).unwrap();
            assert_eq!(state.state.machines.len(), 1);
            assert!(state.state.instances.is_empty());
            assert_eq!(state.state.execution.unresolved().count(), 0);
            let records = state.records.clone();
            drop(state);
            client.finish();
            let writer = Store::open(&data).unwrap();
            assert_eq!(writer.records, records);
            assert!(writer.state.instances.is_empty());
        }
    }
}
