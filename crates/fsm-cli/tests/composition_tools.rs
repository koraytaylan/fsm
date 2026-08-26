//! The three composition tools: the path for a session with no executor
//! running, which is the only way this feature is visible to a model.
//!
//! Plan 0010 task 5102.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{MUTATING_TOOLS, dispatch, registry, validate_args};
use fsm_cli::store::Store;
use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-comp-tools-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CHILD: &str = r#"{"format":"fsm.machine/1","name":"leaf","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[{"name":"outcome","ty":"str","init":"pending"}],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done","do":[{"target":"outcome","value":"\"ok\""}]}]}"#;

fn parent_src() -> String {
    let digest = digest_of(&machine_id(&value(CHILD))).unwrap().to_string();
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"review","machine":"{digest}","returns":{{"decision":"outcome"}}}}]}},{{"name":"settled"}}],"initial":"idle","context":[{{"name":"seen","ty":"str","init":""}},{{"name":"peer","ty":"str","init":"inst-peer"}}],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"$done.invoke.review","to":"settled","do":[{{"target":"seen","value":"evt.decision"}}]}}]}}"#
    )
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect(),
    )
}

fn str_value(text: &str) -> Value {
    Value::Str(text.to_string())
}

/// A store whose parent is in `busy` with one pending slot.
fn waiting(directory: &TestDirectory) -> (Store, FixedClock) {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for spec in [CHILD.to_string(), parent_src()] {
        dispatch(
            &mut store,
            &mut clock,
            "machine_create",
            &obj(&[("spec", value(&spec))]),
        )
        .unwrap();
    }
    dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", str_value("parent")),
            ("request_id", str_value("p1")),
        ]),
    )
    .unwrap();
    dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", str_value("inst-p1")),
            ("event", obj(&[("name", str_value("open"))])),
            ("request_id", str_value("open-1")),
        ]),
    )
    .unwrap();
    (store, clock)
}

fn validated(tool: &str, output: &Value) {
    let spec = registry()
        .into_iter()
        .find(|candidate| candidate.name == tool)
        .unwrap_or_else(|| panic!("{tool} is registered"));
    validate_args(&(spec.output_schema)(), output)
        .unwrap_or_else(|e| panic!("{tool} output does not validate: {e:?}"));
}

#[test]
fn the_three_tools_perform_their_operations_and_validate() {
    let directory = TestDirectory::create();
    let (mut store, mut clock) = waiting(&directory);
    let started = dispatch(
        &mut store,
        &mut clock,
        "invocation_start",
        &obj(&[
            ("instance_id", str_value("inst-p1")),
            ("slot", str_value("review")),
            ("request_id", str_value("inv-1")),
        ]),
    )
    .unwrap();
    validated("invocation_start", &started);
    let child = child_instance_id("inst-p1", "review");
    assert_eq!(
        started.get("child_instance_id").and_then(Value::as_str),
        Some(child.as_str())
    );

    dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", str_value(&child)),
            ("event", obj(&[("name", str_value("finish"))])),
            ("request_id", str_value("fin-1")),
        ]),
    )
    .unwrap();
    let returned = dispatch(
        &mut store,
        &mut clock,
        "invocation_return",
        &obj(&[
            ("instance_id", str_value("inst-p1")),
            ("slot", str_value("review")),
            ("request_id", str_value("ret-1")),
        ]),
    )
    .unwrap();
    validated("invocation_return", &returned);
    assert_eq!(
        returned.get("outcome").and_then(Value::as_str),
        Some("completed")
    );
    assert_eq!(
        store.state.instances["inst-p1"].ctx["seen"].canonical_string(),
        "ok"
    );

    // And a delivery, from a sender that signals its peer.
    let sender = r#"{"format":"fsm.machine/1","name":"sender","states":[{"name":"idle"},{"name":"working","entry":{"signal":[{"to":"ctx.peer","event":"finish"}]}}],"initial":"idle","context":[{"name":"peer","ty":"str","init":"inst-p1"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"idle","on":"go","to":"working"}]}"#;
    dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &obj(&[("spec", value(sender))]),
    )
    .unwrap();
    dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", str_value("sender")),
            ("request_id", str_value("s1")),
        ]),
    )
    .unwrap();
    dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", str_value("inst-s1")),
            ("event", obj(&[("name", str_value("go"))])),
            ("request_id", str_value("go-1")),
        ]),
    )
    .unwrap();
    let signal_id = store.state.instances["inst-s1"]
        .signals
        .keys()
        .next()
        .cloned()
        .unwrap();
    let delivered = dispatch(
        &mut store,
        &mut clock,
        "signal_deliver",
        &obj(&[
            ("instance_id", str_value("inst-s1")),
            ("signal_id", str_value(&signal_id)),
            ("request_id", str_value("sig-1")),
        ]),
    )
    .unwrap();
    validated("signal_deliver", &delivered);
    assert_eq!(
        delivered.get("target_instance_id").and_then(Value::as_str),
        Some("inst-p1")
    );
}

#[test]
fn all_three_are_mutating_and_require_a_request_id() {
    for tool in ["invocation_start", "invocation_return", "signal_deliver"] {
        assert!(MUTATING_TOOLS.contains(&tool), "{tool} must be gated");
        let spec = registry().into_iter().find(|t| t.name == tool).unwrap();
        let required: Vec<String> = (spec.input_schema)()
            .get("required")
            .and_then(Value::as_arr)
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        assert!(
            required.contains(&"request_id".to_string()),
            "{tool} must require request_id"
        );
    }
}

#[test]
fn a_replayed_call_is_a_duplicate() {
    let directory = TestDirectory::create();
    let (mut store, mut clock) = waiting(&directory);
    let args = obj(&[
        ("instance_id", str_value("inst-p1")),
        ("slot", str_value("review")),
        ("request_id", str_value("inv-1")),
    ]);
    let first = dispatch(&mut store, &mut clock, "invocation_start", &args).unwrap();
    assert_eq!(first.get("duplicate").and_then(Value::as_bool), Some(false));
    let again = dispatch(&mut store, &mut clock, "invocation_start", &args).unwrap();
    assert_eq!(again.get("duplicate").and_then(Value::as_bool), Some(true));
}

#[test]
fn the_cli_and_the_tool_agree_byte_for_byte() {
    // Two stores driven identically, one through the binary and one through
    // the tool: the `--json` bytes must equal the structured result.
    let cli_dir = TestDirectory::create();
    let tool_dir = TestDirectory::create();
    let mut written = Vec::new();
    for directory in [cli_dir.path(), tool_dir.path()] {
        let mut store = Store::open(directory).unwrap();
        let mut clock = FixedClock::new(1_000, 1);
        for spec in [CHILD.to_string(), parent_src()] {
            store
                .define_machine_on(&mut clock, value(&spec), false, false)
                .unwrap();
        }
        store
            .create_instance_ctx_on(
                &mut clock,
                "parent",
                "inst-p1",
                "p1",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        store
            .send_event(
                "inst-p1",
                "open",
                Value::Obj(BTreeMap::new()),
                "open-1",
                None,
            )
            .unwrap();
        written.push(store.records.len());
    }

    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args([
            "instance",
            "invoke",
            "inst-p1",
            "review",
            "--request-id=inv-1",
            "--json",
        ])
        .arg(format!("--data-dir={}", cli_dir.path().display()))
        .env("FSM_CLOCK_MS", "5000")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut store = Store::open(tool_dir.path()).unwrap();
    let mut clock = FixedClock::new(5_000, 1);
    let structured = dispatch(
        &mut store,
        &mut clock,
        "invocation_start",
        &obj(&[
            ("instance_id", str_value("inst-p1")),
            ("slot", str_value("review")),
            ("request_id", str_value("inv-1")),
        ]),
    )
    .unwrap();
    let mut expected = fsm_core::canon::canon_bytes(&structured);
    expected.push(b'\n');
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(expected).unwrap(),
        "the CLI's --json and the tool's structured result are one output"
    );
}

#[test]
fn each_cli_command_reports_its_usage_without_arguments() {
    let directory = TestDirectory::create();
    let (store, _) = waiting(&directory);
    drop(store);
    // The argument parser names the missing positional before the command
    // body runs, which is the same diagnostic every other two-positional
    // command gives.
    for (command, missing) in [
        ("invoke", "slot"),
        ("return", "slot"),
        ("signal", "signal-id"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(["instance", command, "inst-p1"])
            .arg(format!("--data-dir={}", directory.path().display()))
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{command} needs two positionals");
        let rendered = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            rendered.contains(&format!("missing positional {missing}")),
            "{command}: {rendered}"
        );
    }
}
