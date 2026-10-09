//! One provisioned real stdio draft/repair/create workflow; no local substitute.

use super::*;
use fsm_core::record::RecordKind;
use std::io::Read;
use std::{
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    time::Instant,
};

fn manifest() -> Value {
    let path = PathBuf::from(
        std::env::var_os("FSM_NATIVE_WORKFLOW_MANIFEST").expect("protected coordinator manifest"),
    );
    assert!(path.is_absolute());
    for directory in path.parent().unwrap().ancestors() {
        let metadata = fs::symlink_metadata(directory).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.uid(), 0);
        assert_eq!(metadata.mode() & 0o022, 0);
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000 | 0o4000)
        .open(&path)
        .unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), 0);
    assert_eq!(metadata.mode() & 0o7777, 0o444);
    assert_eq!(metadata.nlink(), 1);
    assert!(metadata.len() <= 65536);
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(65537)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 65536);
    let observed = fs::symlink_metadata(&path).unwrap();
    assert_eq!(
        (observed.dev(), observed.ino()),
        (metadata.dev(), metadata.ino())
    );
    let entries = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(entries.as_arr().unwrap().len(), 1);
    entries.as_arr().unwrap()[0].clone()
}

fn path(entry: &Value, name: &str) -> PathBuf {
    let path = PathBuf::from(entry.get(name).unwrap().as_str().unwrap());
    assert!(path.is_absolute());
    path
}

fn machine() -> Value {
    // Independently authored from SPEC: one automatic effect with two required
    // str arguments and two externally sendable terminal outcomes.
    parse(br#"{
      "format":"fsm.machine/1","name":"native_contract_draft",
      "context":[
        {"name":"resource","ty":"str","init":"\"resource with spaces; $(literal) \\\"quoted\\\"\""},
        {"name":"run","ty":"str","init":"\"run-1\""}
      ],
      "events":[{"name":"begin","fields":[]},{"name":"check_prerequisite_ok","fields":[]},{"name":"check_prerequisite_failed","fields":[]}],
      "effects":[{"name":"check_prerequisite","fields":[{"name":"resource","ty":"str"},{"name":"run","ty":"str"}]}],
      "states":[{"name":"idle"},{"name":"working","entry":{"emit":[{"effect":"check_prerequisite","args":{"resource":"ctx.resource","run":"ctx.run"}}]}},{"name":"done","terminal":true},{"name":"failed","terminal":true}],
      "initial":"idle","transitions":[
        {"from":"idle","on":"begin","to":"working"},
        {"from":"working","on":"check_prerequisite_ok","to":"done"},
        {"from":"working","on":"check_prerequisite_failed","to":"failed"}
      ]
    }"#, &JsonLimits::DEFAULT).unwrap()
}

fn invalid_machine() -> Value {
    let mut spec = machine();
    let Value::Obj(states) = &mut spec else {
        unreachable!()
    };
    let Value::Arr(states) = states.get_mut("states").unwrap() else {
        unreachable!()
    };
    let Value::Obj(working) = &mut states[1] else {
        unreachable!()
    };
    let Value::Obj(entry) = working.get_mut("entry").unwrap() else {
        unreachable!()
    };
    let Value::Arr(emits) = entry.get_mut("emit").unwrap() else {
        unreachable!()
    };
    let Value::Obj(emit) = &mut emits[0] else {
        unreachable!()
    };
    let Value::Obj(arguments) = emit.get_mut("args").unwrap() else {
        unreachable!()
    };
    arguments.remove("run");
    spec
}

pub(super) fn run() {
    let entry = manifest();
    let store = path(&entry, "store");
    let resource = path(&entry, "resource");
    let table = entry
        .get("handlers")
        .expect("original protected operator table");
    let handlers = store.join("contract-handlers.json");
    fs::write(&handlers, canon_bytes(table)).unwrap();
    let mut command = Command::new(path(&entry, "cli"));
    command
        .arg("--data-dir")
        .arg(&store)
        .args(["serve", "--execute", "--handlers"])
        .arg(&handlers)
        .env("HOME", path(&entry, "home"))
        .stderr(fs::File::create(store.join("stderr")).unwrap());
    let mut client = Client::spawn(command);
    let discovery = client.call(
        2,
        "resources/read",
        obj([("uri", Value::Str("fsm://executor".into()))]),
    );
    let contents = discovery.get("contents").unwrap().as_arr().unwrap();
    let capabilities = parse(
        contents[0]
            .get("text")
            .unwrap()
            .as_str()
            .unwrap()
            .as_bytes(),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(
        capabilities.get("format").and_then(Value::as_str),
        Some("fsm.executor/2")
    );
    assert_eq!(
        capabilities.get("progress").and_then(Value::as_str),
        Some("autonomous")
    );
    let tools = client.call(3, "tools/list", obj([]));
    let schema = tools
        .get("tools")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("executor_check"))
        .unwrap()
        .get("outputSchema")
        .unwrap();
    let before = Store::open_read_only(&store).unwrap().records.clone();
    let bytes = files(&store);
    let invalid = invalid_machine();
    fsm_core::spec::compile_accepted(&invalid).unwrap();
    let rejected = client.check(4, obj([("spec", invalid)]));
    assert_eq!(
        rejected.get("status").and_then(Value::as_str),
        Some("invalid")
    );
    assert!(
        rejected
            .get("findings")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .any(|finding| finding.get("code").and_then(Value::as_str)
                == Some("exec/contract_argument_missing")
                && finding
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| path.ends_with("/args/run"))
                && finding
                    .get("hint")
                    .and_then(Value::as_str)
                    .is_some_and(|hint| hint.contains("supply")))
    );
    let repaired = client.check(5, obj([("spec", machine())]));
    assert_eq!(
        repaired.get("status").and_then(Value::as_str),
        Some("compatible")
    );
    validate_args(schema, &rejected).unwrap();
    validate_args(schema, &repaired).unwrap();
    let compiled = fsm_core::spec::compile_accepted(&machine()).unwrap();
    let table =
        fsm_execute::config::HandlerTable::parse(&String::from_utf8(canon_bytes(table)).unwrap())
            .unwrap();
    assert_eq!(
        repaired,
        fsm_execute::contract::analyze_contract(
            &compiled,
            &BTreeMap::new(),
            &table,
            fsm_execute::contract::Limits::default()
        )
        .unwrap()
        .to_value()
    );
    for handler in table.handlers.values() {
        let report = String::from_utf8(canon_bytes(&rejected)).unwrap()
            + &String::from_utf8(canon_bytes(&repaired)).unwrap();
        assert!(!report.contains(&handler.argv[0]));
        for argument in handler
            .argv
            .iter()
            .filter(|argument| argument.starts_with("handler-directory="))
        {
            assert!(!report.contains(argument));
        }
    }
    assert_eq!(files(&store), bytes);
    let snapshot = Store::open_read_only(&store).unwrap();
    assert_eq!(snapshot.records, before);
    assert!(snapshot.state.machines.is_empty());
    assert!(snapshot.state.instances.is_empty());
    assert_eq!(fs::read_to_string(resource.join("calls")).unwrap(), "");
    assert!(!resource.join("work").exists());
    drop(snapshot);
    let create = client.call(
        6,
        "tools/call",
        obj([
            ("name", Value::Str("machine_create".into())),
            ("arguments", obj([("spec", machine())])),
        ]),
    );
    assert_ne!(create.get("isError"), Some(&Value::Bool(true)));
    let stored_records = Store::open_read_only(&store).unwrap().records.clone();
    let stored = client.check(
        7,
        obj([("machine", Value::Str("native_contract_draft".into()))]),
    );
    assert_eq!(stored, repaired);
    assert_eq!(
        Store::open_read_only(&store).unwrap().records,
        stored_records
    );
    client.call(
        8,
        "tools/call",
        obj([
            ("name", Value::Str("instance_create".into())),
            (
                "arguments",
                obj([
                    ("machine", Value::Str("native_contract_draft".into())),
                    ("request_id", Value::Str("contract-run".into())),
                ]),
            ),
        ]),
    );
    client.call(
        9,
        "tools/call",
        obj([
            ("name", Value::Str("instance_send".into())),
            (
                "arguments",
                obj([
                    ("instance_id", Value::Str("inst-contract-run".into())),
                    ("request_id", Value::Str("contract-begin".into())),
                    ("event", obj([("name", Value::Str("begin".into()))])),
                ]),
            ),
        ]),
    );
    // No client requests after triggering: observe the actual journal while the
    // original host autonomously executes, acknowledges and retires its handler.
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let snapshot = Store::open_read_only(&store).unwrap();
        if snapshot.state.instances["inst-contract-run"].status
            == fsm_core::machine::Status::Completed
            && snapshot.state.execution.unresolved().count() == 0
        {
            assert_eq!(
                snapshot
                    .instance_view("inst-contract-run", None, None)
                    .unwrap()
                    .get("state")
                    .and_then(Value::as_str),
                Some("done")
            );
            assert_eq!(snapshot.state.execution.unresolved().count(), 0);
            for kind in [
                RecordKind::ExecutionClaimed,
                RecordKind::ExecutionStopped,
                RecordKind::ExecutionSettled,
            ] {
                assert_eq!(
                    snapshot
                        .records
                        .iter()
                        .filter(|record| record.kind == kind)
                        .count(),
                    1
                );
            }
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native draft workflow must complete while clients are quiet"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        fs::read_to_string(resource.join("calls")).unwrap(),
        "check_prerequisite\n"
    );
    drop(client.input.take());
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(status) = client.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original native owner must retire on EOF"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    client.reader.take().unwrap().join().unwrap();
    let writer = Store::open(&store).unwrap();
    assert_eq!(writer.state.execution.unresolved().count(), 0);
}

#[test]
fn authored_native_pair_has_an_independent_missing_argument_and_repair() {
    let table = fsm_execute::config::HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{
        "effect":"check_prerequisite","argv":["/PRIVATE_STAGED_HELPER","handler-resource={resource}","handler-run={run}"],"timeout_ms":1000,
        "on_ok":{"event":"check_prerequisite_ok"},"on_failed":{"event":"check_prerequisite_failed"}
    }]}"#).unwrap();
    for (spec, expected) in [(invalid_machine(), "invalid"), (machine(), "compatible")] {
        let machine = fsm_core::spec::compile_accepted(&spec).unwrap();
        let result = fsm_execute::contract::analyze_contract(
            &machine,
            &BTreeMap::new(),
            &table,
            fsm_execute::contract::Limits::default(),
        )
        .unwrap()
        .to_value();
        assert_eq!(result.get("status").and_then(Value::as_str), Some(expected));
    }
}
