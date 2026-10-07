//! A client discovers actual handler contracts, authors a workflow, and drives
//! the real stdio server and executor through success and compensating failure.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};

#[cfg(target_os = "linux")]
#[path = "workflow_race/mod.rs"]
mod workflow_race;

const OPERATIONS: [&str; 7] = [
    "check_prerequisite",
    "check_identity",
    "check_access",
    "check_target",
    "suspend",
    "perform_work",
    "restore",
];
const RESOURCE: &str = "resource with spaces; $(literal) \"quoted\"";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf, Option<Value>);

impl Directory {
    fn resource(&self) -> PathBuf {
        self.1.as_ref().map_or_else(
            || self.0.join("resource"),
            |entry| PathBuf::from(text(entry, "resource")),
        )
    }

    fn store(&self) -> PathBuf {
        self.1.as_ref().map_or_else(
            || self.0.join("store"),
            |entry| PathBuf::from(text(entry, "store")),
        )
    }

    fn executable(&self) -> PathBuf {
        self.1.as_ref().map_or_else(
            || PathBuf::from(env!("CARGO_BIN_EXE_fsm")),
            |entry| PathBuf::from(text(entry, "cli")),
        )
    }

    fn new() -> Self {
        #[cfg(target_os = "linux")]
        if let Some(manifest) = std::env::var_os("FSM_NATIVE_WORKFLOW_MANIFEST") {
            use std::os::unix::fs::MetadataExt;
            let metadata = fs::symlink_metadata(&manifest).unwrap();
            assert!(metadata.is_file());
            assert_eq!(metadata.uid(), 0);
            assert_eq!(metadata.mode() & 0o7777, 0o444);
            let mut encoded = Vec::new();
            fs::File::open(&manifest)
                .unwrap()
                .take(65537)
                .read_to_end(&mut encoded)
                .unwrap();
            assert!(encoded.len() <= 65536);
            let entries = parse(&encoded, &JsonLimits::DEFAULT).unwrap();
            let index = NEXT.fetch_add(1, Ordering::Relaxed) as usize;
            let entry = entries
                .as_arr()
                .unwrap()
                .get(index)
                .expect("one registered fixture per scenario")
                .clone();
            let root = PathBuf::from(text(&entry, "directory"));
            return Self(root, Some(entry));
        }
        loop {
            let path = std::env::temp_dir().join(format!(
                "fsm-mcp-workflow-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    fs::create_dir(path.join("resource")).unwrap();
                    return Self(path, None);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory: {error}"),
            }
        }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        if self.1.is_none() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn string(value: &str) -> Value {
    Value::Str(value.to_owned())
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn text(value: &Value, field: &str) -> String {
    value.get(field).unwrap().as_str().unwrap().to_owned()
}

/// Re-executing the test binary keeps handlers portable and dependency-free.
#[test]
fn workflow_handler() {
    let arguments: Vec<String> = std::env::args().collect();
    let Some(operation) = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("handler-operation="))
    else {
        return;
    };
    #[cfg(target_os = "linux")]
    if arguments
        .iter()
        .any(|argument| argument == "handler-memory-limits=1073741824,0")
    {
        let membership = fs::read_to_string("/proc/self/cgroup").unwrap();
        let group = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .unwrap();
        let directory = Path::new("/sys/fs/cgroup").join(group.trim_start_matches('/'));
        assert_eq!(
            fs::read_to_string(directory.join("memory.max"))
                .unwrap()
                .trim(),
            "1073741824"
        );
        assert_eq!(
            fs::read_to_string(directory.join("memory.swap.max"))
                .unwrap()
                .trim(),
            "0"
        );
    }
    assert!(arguments.contains(&format!("handler-resource={RESOURCE}")));
    assert!(arguments.contains(&"handler-run=run-1".to_owned()));
    let directory = PathBuf::from(
        arguments
            .iter()
            .find_map(|argument| argument.strip_prefix("handler-directory="))
            .expect("explicit workflow directory argument"),
    );
    let mut calls = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("calls"))
        .unwrap();
    writeln!(calls, "{operation}").unwrap();
    #[cfg(target_os = "linux")]
    if operation == "check_prerequisite"
        && arguments
            .iter()
            .any(|argument| argument == "handler-failures=race")
    {
        workflow_race::hold_tree(&directory);
    }
    let phase = directory.join("phase");
    let failed = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("handler-failures="))
        .expect("explicit workflow failure argument")
        .split(',')
        .any(|failure| failure == operation);
    match operation {
        "suspend" => fs::write(&phase, "suspended").unwrap(),
        "perform_work" => {
            assert_eq!(fs::read_to_string(&phase).unwrap(), "suspended");
            let template = directory.join(".work-template");
            let shared = template.exists();
            if shared {
                // A real operation publishes preprovisioned external data;
                // work stays absent until this handler actually executes.
                fs::hard_link(template, directory.join("work")).unwrap();
            }
            // A partial external result must not suppress compensating work.
            fs::write(
                directory.join("work"),
                if failed { "first" } else { "first,second" },
            )
            .unwrap();
            #[cfg(unix)]
            if !shared {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(directory.join("work"), fs::Permissions::from_mode(0o644))
                    .unwrap();
            }
        }
        "restore" => {
            assert_eq!(fs::read_to_string(&phase).unwrap(), "suspended");
            if !failed {
                fs::write(&phase, "active").unwrap();
            }
        }
        _ => assert_eq!(fs::read_to_string(&phase).unwrap(), "active"),
    }
    std::process::exit(if failed { 7 } else { 0 });
}

fn write_handlers(directory: &Path, resource: &Path, failures: &str, native: bool) {
    let executable = std::env::current_exe().unwrap();
    let handlers = OPERATIONS
        .iter()
        .map(|operation| {
            object([
                ("effect", string(operation)),
                (
                    "argv",
                    Value::Arr({
                        let mut arguments = vec![
                            string(executable.to_str().unwrap()),
                            string("workflow_handler"),
                            string("--exact"),
                            string("--nocapture"),
                            string(&format!("handler-operation={operation}")),
                            string("handler-resource={resource}"),
                            string("handler-run={run}"),
                            string(&format!("handler-directory={}", resource.to_str().unwrap())),
                            string(&format!("handler-failures={failures}")),
                        ];
                        if native {
                            arguments.push(string("handler-memory-limits=1073741824,0"));
                        }
                        arguments
                    }),
                ),
                ("timeout_ms", Value::Num("30000".into())),
                (
                    "on_ok",
                    object([("event", string(&format!("{operation}_ok")))]),
                ),
                (
                    "on_failed",
                    object([("event", string(&format!("{operation}_failed")))]),
                ),
            ])
        })
        .collect();
    fs::write(
        directory.join("handlers.json"),
        canon_bytes(&object([
            ("format", string("fsm.handlers/1")),
            ("handlers", Value::Arr(handlers)),
        ])),
    )
    .unwrap();
}

struct Client {
    process: Child,
    input: ChildStdin,
    responses: Receiver<Result<Value, String>>,
    reader: Option<JoinHandle<()>>,
    request: u64,
    errors: PathBuf,
}

impl Client {
    fn start(fixture: &Directory) -> Self {
        let directory = &fixture.0;
        let errors = directory.join("stderr");
        let mut command = Command::new(fixture.executable());
        if let Some(entry) = &fixture.1 {
            command.env("HOME", text(entry, "home"));
        }
        let mut process = command
            .arg("--data-dir")
            .arg(fixture.store())
            .args(["serve", "--execute", "--handlers"])
            .arg(directory.join("handlers.json"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&errors).unwrap())
            .spawn()
            .unwrap();
        let input = process.stdin.take().unwrap();
        let output = process.stdout.take().unwrap();
        let (sender, responses) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let response = line.map_err(|error| error.to_string()).and_then(|line| {
                    parse(line.as_bytes(), &JsonLimits::DEFAULT)
                        .map_err(|error| format!("invalid MCP response {line:?}: {error:?}"))
                });
                if sender.send(response).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            process,
            input,
            responses,
            reader: Some(reader),
            request: 0,
            errors,
        };
        let initialized = client.request(
            "initialize",
            value(r#"{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"workflow-test","version":"1"}}"#),
        );
        assert!(text(&initialized, "instructions").contains("fsm://executor"));
        writeln!(
            client.input,
            "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}"
        )
        .unwrap();
        client.input.flush().unwrap();
        client
    }

    fn request(&mut self, method: &str, parameters: Value) -> Value {
        self.request += 1;
        let identifier = Value::Num(self.request.to_string());
        let request = object([
            ("jsonrpc", string("2.0")),
            ("id", identifier.clone()),
            ("method", string(method)),
            ("params", parameters),
        ]);
        self.input.write_all(&canon_bytes(&request)).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let response = self
                .responses
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "MCP {method}: {error}; stderr: {}",
                        fs::read_to_string(&self.errors).unwrap_or_default()
                    )
                })
                .unwrap();
            if response.get("id") == Some(&identifier) {
                assert!(response.get("error").is_none(), "{response:?}");
                return response.get("result").unwrap().clone();
            }
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request(
            "tools/call",
            object([("name", string(name)), ("arguments", arguments)]),
        );
        assert_ne!(
            response.get("isError"),
            Some(&Value::Bool(true)),
            "{response:?}"
        );
        response.get("structuredContent").unwrap().clone()
    }

    fn discover_handlers(&mut self) -> BTreeMap<String, Value> {
        let resources = self.request("resources/list", object([]));
        assert!(
            resources
                .get("resources")
                .unwrap()
                .as_arr()
                .unwrap()
                .iter()
                .any(|resource| resource.get("uri").and_then(Value::as_str)
                    == Some("fsm://executor"))
        );
        let response = self.request(
            "resources/read",
            object([("uri", string("fsm://executor"))]),
        );
        let content = &response.get("contents").unwrap().as_arr().unwrap()[0];
        let capabilities = value(&text(content, "text"));
        assert_eq!(text(&capabilities, "mode"), "embedded");
        assert_eq!(
            capabilities.get("executes_effects"),
            Some(&Value::Bool(true))
        );
        assert_eq!(text(&capabilities, "progress"), "client_requests");
        let handlers = capabilities.get("handlers").unwrap().as_arr().unwrap();
        assert_eq!(handlers.len(), OPERATIONS.len());
        handlers
            .iter()
            .map(|handler| {
                assert_eq!(text(handler, "kind"), "process");
                assert_eq!(
                    handler.get("required_args"),
                    Some(&value(r#"["resource","run"]"#))
                );
                assert!(handler.get("argv").is_none());
                (text(handler, "effect"), handler.clone())
            })
            .collect()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn emission(handler: &Value) -> Value {
    let arguments = handler
        .get("required_args")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .map(|argument| {
            let name = argument.as_str().unwrap();
            (name.to_owned(), string(&format!("ctx.{name}")))
        })
        .collect();
    object([(
        "emit",
        Value::Arr(vec![object([
            ("effect", handler.get("effect").unwrap().clone()),
            ("args", Value::Obj(arguments)),
        ])]),
    )])
}

fn transition(from: &str, event: &str, to: &str) -> Value {
    object([
        ("from", string(from)),
        ("on", string(event)),
        ("to", string(to)),
    ])
}

fn machine(handlers: &BTreeMap<String, Value>) -> Value {
    let mut events = vec![value(r#"{"name":"begin","fields":[]}"#)];
    let mut effects = Vec::new();
    let mut states = vec![object([("name", string("ready"))])];
    let mut transitions = vec![transition("ready", "begin", OPERATIONS[0])];
    for (index, operation) in OPERATIONS.iter().enumerate() {
        let handler = &handlers[*operation];
        let fields = handler
            .get("required_args")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .map(|argument| object([("name", argument.clone()), ("ty", string("str"))]))
            .collect();
        effects.push(object([
            ("name", string(operation)),
            ("fields", Value::Arr(fields)),
        ]));
        states.push(object([
            ("name", string(operation)),
            ("entry", emission(handler)),
        ]));
        for outcome in ["on_ok", "on_failed"] {
            let event = text(handler.get(outcome).unwrap(), "event");
            events.push(object([
                ("name", string(&event)),
                ("fields", Value::Arr(vec![])),
            ]));
            let destination = if outcome == "on_ok" {
                OPERATIONS.get(index + 1).copied().unwrap_or("succeeded")
            } else if index < 4 {
                "rejected"
            } else if *operation == "restore" {
                "cleanup_failed"
            } else {
                "recovering"
            };
            transitions.push(transition(operation, &event, destination));
        }
    }
    states.push(object([
        ("name", string("recovering")),
        ("entry", emission(&handlers["restore"])),
    ]));
    for (outcome, destination) in [
        ("on_ok", "failed_restored"),
        ("on_failed", "cleanup_failed"),
    ] {
        transitions.push(transition(
            "recovering",
            &text(handlers["restore"].get(outcome).unwrap(), "event"),
            destination,
        ));
    }
    for terminal in ["succeeded", "rejected", "failed_restored", "cleanup_failed"] {
        states.push(object([
            ("name", string(terminal)),
            ("terminal", Value::Bool(true)),
        ]));
    }
    object([
        ("format", string("fsm.machine/1")),
        ("name", string("discovered_workflow")),
        ("initial", string("ready")),
        (
            "context",
            Value::Arr(vec![
                object([
                    ("name", string("resource")),
                    ("ty", string("str")),
                    ("init", string(RESOURCE)),
                ]),
                object([
                    ("name", string("run")),
                    ("ty", string("str")),
                    ("init", string("run-1")),
                ]),
            ]),
        ),
        ("events", Value::Arr(events)),
        ("effects", Value::Arr(effects)),
        ("states", Value::Arr(states)),
        ("transitions", Value::Arr(transitions)),
    ])
}

fn run_scenario(failures: &str, terminal: &str, expected_calls: &[&str], phase: &str) {
    let directory = Directory::new();
    fs::write(directory.resource().join("phase"), "active").unwrap();
    write_handlers(
        &directory.0,
        &directory.resource(),
        failures,
        directory.1.is_some(),
    );
    let mut client = Client::start(&directory);
    let handlers = client.discover_handlers();
    client.call("machine_create", object([("spec", machine(&handlers))]));
    client.call(
        "instance_create",
        value(r#"{"machine":"discovered_workflow","request_id":"run"}"#),
    );
    client.call(
        "instance_send",
        value(r#"{"instance_id":"inst-run","request_id":"begin","event":{"name":"begin"}}"#),
    );
    #[cfg(target_os = "linux")]
    let mut competitor =
        (failures == "race").then(|| workflow_race::contend(&directory, &mut client));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let instance = client.call("instance_get", value(r#"{"instance_id":"inst-run"}"#));
        if text(&instance, "status") == "completed" {
            assert_eq!(
                text(instance.get("configuration").unwrap(), "leaf"),
                terminal,
                "failures={failures}"
            );
            assert_eq!(instance.get("effects_pending"), Some(&value("[]")));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "workflow stalled: {instance:?}; executor stderr: {}",
            bounded_executor_errors(&client.errors)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    #[cfg(target_os = "linux")]
    if let Some(competitor) = &mut competitor {
        competitor.stop(&directory);
    }
    let history = client.call(
        "instance_history",
        value(r#"{"instance_id":"inst-run","limit":500}"#),
    );
    assert_eq!(history.get("chain_verified"), Some(&Value::Bool(true)));
    let entries = history.get("entries").unwrap().as_arr().unwrap();
    let acknowledgements = if directory.1.is_some() {
        // Native settlement acknowledges atomically without an EffectAcked
        // append; attempted/interrupted dispositions never count as an ack.
        let store = fsm_store::store::Store::open_read_only(&directory.store()).unwrap();
        let acked = store
            .records
            .iter()
            .filter(|record| {
                record.kind == fsm_core::record::RecordKind::ExecutionSettled
                    && record.body.get("disposition").and_then(Value::as_str) == Some("acked")
            })
            .count();
        assert_eq!(
            entries
                .iter()
                .filter(
                    |entry| entry.get("kind").and_then(Value::as_str) == Some("ExecutionSettled")
                )
                .count(),
            acked
        );
        acked
    } else {
        entries
            .iter()
            .filter(|entry| entry.get("kind").and_then(Value::as_str) == Some("EffectAcked"))
            .count()
    };
    assert_eq!(acknowledgements, expected_calls.len());
    assert_eq!(
        text(&client.call("journal_verify", object([])), "health"),
        "Ok"
    );
    let calls = fs::read_to_string(directory.resource().join("calls")).unwrap();
    assert_eq!(calls.lines().collect::<Vec<_>>(), expected_calls);
    assert_eq!(
        fs::read_to_string(directory.resource().join("phase")).unwrap(),
        phase
    );
    if terminal == "succeeded" {
        assert_eq!(
            fs::read_to_string(directory.resource().join("work")).unwrap(),
            "first,second"
        );
    }
    if terminal == "rejected" {
        assert!(!directory.resource().join("work").exists());
    }
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires native provisioning; plan 0022 WORKFLOW-NATIVE-REVIEW.md and mandatory native CI"
)]
fn discovered_handlers_complete_the_workflow_in_order() {
    run_scenario("", "succeeded", &OPERATIONS, "active");
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires native provisioning; plan 0022 WORKFLOW-NATIVE-REVIEW.md and mandatory native CI"
)]
fn each_failed_preflight_stops_before_external_changes() {
    for (index, operation) in OPERATIONS[..4].iter().enumerate() {
        run_scenario(operation, "rejected", &OPERATIONS[..=index], "active");
    }
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires native provisioning; plan 0022 WORKFLOW-NATIVE-REVIEW.md and mandatory native CI"
)]
fn failures_after_suspension_restore_the_resource() {
    for operation in ["suspend", "perform_work"] {
        let mut expected = OPERATIONS[..if operation == "suspend" { 5 } else { 6 }].to_vec();
        expected.push("restore");
        run_scenario(operation, "failed_restored", &expected, "active");
    }
}

#[test]
#[cfg_attr(
    target_os = "linux",
    ignore = "requires native provisioning; plan 0022 WORKFLOW-NATIVE-REVIEW.md and mandatory native CI"
)]
fn cleanup_failures_are_explicit_after_success_or_partial_work() {
    for failures in ["restore", "perform_work,restore"] {
        run_scenario(failures, "cleanup_failed", &OPERATIONS, "suspended");
    }
}

#[test]
fn workflow_helper_uses_explicit_arguments_without_operator_environment() {
    for failure in ["", "perform_work", "restore"] {
        let directory = Directory::new();
        fs::write(directory.resource().join("phase"), "active").unwrap();
        for operation in OPERATIONS {
            let output = Command::new(std::env::current_exe().unwrap())
                .env_clear()
                .env(
                    "TMPDIR",
                    std::env::var_os("TMPDIR").expect("explicit task cache"),
                )
                .args([
                    "workflow_handler",
                    "--exact",
                    "--nocapture",
                    "handler-run=run-1",
                ])
                .arg(format!("handler-operation={operation}"))
                .arg(format!("handler-failures={failure}"))
                .arg(format!(
                    "handler-directory={}",
                    directory.resource().to_str().unwrap()
                ))
                .arg(format!("handler-resource={RESOURCE}"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(if operation == failure { 7 } else { 0 }),
                "operation={operation}, failure={failure}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(
            fs::read_to_string(directory.resource().join("calls")).unwrap(),
            format!("{}\n", OPERATIONS.join("\n"))
        );
        assert_eq!(
            fs::read_to_string(directory.resource().join("work")).unwrap(),
            if failure == "perform_work" {
                "first"
            } else {
                "first,second"
            }
        );
        assert_eq!(
            fs::read_to_string(directory.resource().join("phase")).unwrap(),
            if failure == "restore" {
                "suspended"
            } else {
                "active"
            }
        );
    }
}

// Bound bytes before decoding so a stalled executor cannot make the failure
// assertion allocate its entire diagnostic file.
fn bounded_executor_errors(path: &Path) -> String {
    fs::File::open(path)
        .map(read_diagnostic_prefix)
        .unwrap_or_default()
}

fn read_diagnostic_prefix(reader: impl Read) -> String {
    let mut prefix = Vec::new();
    let _ = reader.take(8192).read_to_end(&mut prefix);
    String::from_utf8_lossy(&prefix).into_owned()
}

#[test]
fn stalled_workflow_diagnostics_bound_input_bytes_and_tolerate_partial_utf8() {
    let mut diagnostic = vec![b'x'; 8191];
    diagnostic.extend_from_slice("éhidden suffix".as_bytes());
    let mut reader = std::io::Cursor::new(diagnostic);
    let prefix = read_diagnostic_prefix(&mut reader);
    assert_eq!(reader.position(), 8192);
    assert_eq!(prefix, format!("{}�", "x".repeat(8191)));
    assert!(!prefix.contains("hidden suffix"));
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires native provisioning; plan 0022 WORKFLOW-NATIVE-REVIEW.md"]
fn standalone_and_embedded_exclude_a_live_handler_tree() {
    run_scenario("race", "succeeded", &OPERATIONS, "active");
}
