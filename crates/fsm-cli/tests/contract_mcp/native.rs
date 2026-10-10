//! One provisioned real stdio draft/repair/create workflow; no local substitute.

#[path = "native/staged.rs"]
pub(super) mod staged;

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
    // Independently authored from SPEC: prerequisite, suspension, work and
    // recovery; even the final restore site must satisfy the loaded contract.
    parse(br#"{
      "format":"fsm.machine/1","name":"native_contract_draft",
      "context":[
        {"name":"resource","ty":"str","init":"resource with spaces; $(literal) \"quoted\""},
        {"name":"run","ty":"str","init":"run-1"}
      ],
      "events":[{"name":"begin","fields":[]},{"name":"check_prerequisite_ok","fields":[]},{"name":"check_prerequisite_failed","fields":[]},
                {"name":"suspend_ok","fields":[]},{"name":"suspend_failed","fields":[]},
                {"name":"perform_work_ok","fields":[]},{"name":"perform_work_failed","fields":[]},
                {"name":"restore_ok","fields":[]},{"name":"restore_failed","fields":[]}],
      "effects":[{"name":"check_prerequisite","fields":[{"name":"resource","ty":"str"},{"name":"run","ty":"str"}]},
                 {"name":"suspend","fields":[{"name":"resource","ty":"str"},{"name":"run","ty":"str"}]},
                 {"name":"perform_work","fields":[{"name":"resource","ty":"str"},{"name":"run","ty":"str"}]},
                 {"name":"restore","fields":[{"name":"resource","ty":"str"},{"name":"run","ty":"str"}]}],
      "states":[{"name":"idle"},{"name":"working","entry":{"emit":[{"effect":"check_prerequisite","args":{"resource":"ctx.resource","run":"ctx.run"}}]}},
                {"name":"suspending","entry":{"emit":[{"effect":"suspend","args":{"resource":"ctx.resource","run":"ctx.run"}}]}},
                {"name":"performing","entry":{"emit":[{"effect":"perform_work","args":{"resource":"ctx.resource","run":"ctx.run"}}]}},
                {"name":"recovering","entry":{"emit":[{"effect":"restore","args":{"resource":"ctx.resource","run":"ctx.run"}}]}},
                {"name":"done","terminal":true},{"name":"failed","terminal":true}],
      "initial":"idle","transitions":[
        {"from":"idle","on":"begin","to":"working"},
        {"from":"working","on":"check_prerequisite_ok","to":"suspending"},
        {"from":"working","on":"check_prerequisite_failed","to":"failed"},
        {"from":"suspending","on":"suspend_ok","to":"performing"},
        {"from":"suspending","on":"suspend_failed","to":"recovering"},
        {"from":"performing","on":"perform_work_ok","to":"recovering"},
        {"from":"performing","on":"perform_work_failed","to":"recovering"},
        {"from":"recovering","on":"restore_ok","to":"done"},
        {"from":"recovering","on":"restore_failed","to":"failed"}
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
    let Value::Obj(working) = &mut states[4] else {
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

fn unchecked_machine() -> Value {
    let mut invalid = invalid_machine();
    let Value::Obj(fields) = &mut invalid else {
        unreachable!()
    };
    fields.insert("name".into(), Value::Str("unchecked_contract_draft".into()));
    invalid
}

fn repair_machine() -> Value {
    let identity = fsm_core::hashes::machine_id(&unchecked_machine());
    let digest = fsm_core::hashes::digest_of(&identity).unwrap();
    let mut repaired = machine();
    let Value::Obj(fields) = &mut repaired else {
        unreachable!()
    };
    fields.insert(
        "supersedes".into(),
        obj([
            ("machine", Value::Str(digest.into())),
            (
                "states",
                obj([
                    ("idle", Value::Str("idle".into())),
                    ("working", Value::Str("working".into())),
                    ("suspending", Value::Str("suspending".into())),
                    ("performing", Value::Str("performing".into())),
                    ("recovering", Value::Str("recovering".into())),
                ]),
            ),
            (
                "context",
                obj([
                    ("resource", Value::Str("ctx.resource".into())),
                    ("run", Value::Str("ctx.run".into())),
                ]),
            ),
        ]),
    );
    repaired
}

fn changed_table(original: &Value) -> Value {
    let mut changed = original.clone();
    let Value::Obj(fields) = &mut changed else {
        unreachable!()
    };
    let Value::Arr(handlers) = fields.get_mut("handlers").unwrap() else {
        unreachable!()
    };
    let handler = handlers
        .iter_mut()
        .find(|handler| handler.get("effect").and_then(Value::as_str) == Some("restore"))
        .unwrap();
    let Value::Obj(handler) = handler else {
        unreachable!()
    };
    handler.insert(
        "on_ok".into(),
        obj([("event", Value::Str("undeclared_restore_outcome".into()))]),
    );
    changed
}

fn refuse_unchecked_draft(
    client: &mut Client,
    store: &std::path::Path,
    resource: &std::path::Path,
) -> fsm_execute::effect::PendingEffect {
    for (identifier, name, arguments) in [
        (20, "machine_create", obj([("spec", unchecked_machine())])),
        (
            21,
            "instance_create",
            obj([
                ("machine", Value::Str("unchecked_contract_draft".into())),
                ("request_id", Value::Str("contract-run".into())),
            ]),
        ),
        (
            22,
            "instance_send",
            obj([
                ("instance_id", Value::Str("inst-contract-run".into())),
                ("request_id", Value::Str("contract-begin".into())),
                ("event", obj([("name", Value::Str("begin".into()))])),
            ]),
        ),
    ] {
        let result = client.call(
            identifier,
            "tools/call",
            obj([("name", Value::Str(name.into())), ("arguments", arguments)]),
        );
        assert_ne!(
            result.get("isError"),
            Some(&Value::Bool(true)),
            "{result:?}"
        );
    }
    let original = Store::open_read_only(store).unwrap();
    let records = original.records.clone();
    let state = original.state.clone();
    assert_eq!(state.instances["inst-contract-run"].pending.len(), 1);
    assert_eq!(state.execution.unresolved().count(), 0);
    let historical =
        fsm_execute::effect::resolve(&original, &state.instances["inst-contract-run"].pending[0])
            .unwrap();
    drop(original);
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        client.call(23, "ping", obj([]));
        let current = Store::open_read_only(store).unwrap();
        assert_eq!(
            current.records, records,
            "unchecked late-invalid contract published execution work"
        );
        assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
        assert_eq!(fs::read_to_string(resource.join("calls")).unwrap(), "");
        assert!(!resource.join("work").exists());
        std::thread::sleep(Duration::from_millis(10));
    }
    historical
}

fn start(entry: &Value) -> Client {
    let store = path(entry, "store");
    let handlers = store.join("contract-handlers.json");
    let mut command = Command::new(path(entry, "cli"));
    command
        .arg("--data-dir")
        .arg(&store)
        .args(["serve", "--execute", "--handlers"])
        .arg(&handlers)
        .env("HOME", path(entry, "home"))
        .stderr(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(store.join("stderr"))
                .unwrap(),
        );
    Client::spawn(command)
}

fn retire(client: &mut Client) {
    drop(client.input.take());
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(status) = client.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "native host must retire on EOF");
        std::thread::sleep(Duration::from_millis(5));
    }
    client.reader.take().unwrap().join().unwrap();
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
    let mut client = start(&entry);
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
                && finding.get("path").and_then(Value::as_str)
                    == Some("/states/4/entry/emit/0/args/run")
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
    let historical = refuse_unchecked_draft(&mut client, &store, &resource);
    let refused = Store::open_read_only(&store).unwrap();
    let refused_records = refused.records.clone();
    let refused_state = refused.state.clone();
    drop(refused);
    retire(&mut client);
    let reopened = Store::open(&store).unwrap();
    assert_eq!(reopened.records, refused_records);
    assert!(fsm_store::snapshot::store_states_eq(
        &refused_state,
        &reopened.state
    ));
    drop(reopened);
    client = start(&entry);
    let after_restart = client.check(
        26,
        obj([("machine", Value::Str("unchecked_contract_draft".into()))]),
    );
    assert_eq!(
        after_restart.get("status").and_then(Value::as_str),
        Some("invalid")
    );
    validate_args(schema, &after_restart).unwrap();
    for _ in 0..3 {
        client.call(27, "ping", obj([]));
        let current = Store::open_read_only(&store).unwrap();
        assert_eq!(current.records, refused_records);
        assert!(fsm_store::snapshot::store_states_eq(
            &refused_state,
            &current.state
        ));
        assert_eq!(
            fsm_execute::effect::resolve(&current, &historical.effect_id).unwrap(),
            historical
        );
        assert_eq!(fs::read_to_string(resource.join("calls")).unwrap(), "");
        assert!(!resource.join("work").exists());
    }
    let repair = repair_machine();
    let repaired = client.check(25, obj([("spec", repair.clone())]));
    assert_eq!(
        repaired.get("status").and_then(Value::as_str),
        Some("compatible")
    );
    validate_args(schema, &repaired).unwrap();
    let create = client.call(
        6,
        "tools/call",
        obj([
            ("name", Value::Str("machine_create".into())),
            ("arguments", obj([("spec", repair)])),
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
    // Keep the successful report, but load a different late outcome before
    // migrating; admission must use this host's actual table without a token.
    retire(&mut client);
    let changed = changed_table(entry.get("handlers").unwrap());
    fs::write(&handlers, canon_bytes(&changed)).unwrap();
    client = start(&entry);
    let migrated = client.call(
        8,
        "tools/call",
        obj([
            ("name", Value::Str("instance_migrate".into())),
            (
                "arguments",
                obj([
                    ("instance_id", Value::Str("inst-contract-run".into())),
                    ("to_machine", Value::Str("native_contract_draft".into())),
                    ("request_id", Value::Str("contract-repair".into())),
                ]),
            ),
        ]),
    );
    assert_ne!(
        migrated.get("isError"),
        Some(&Value::Bool(true)),
        "{migrated:?}"
    );
    let current = Store::open_read_only(&store).unwrap();
    assert_eq!(
        fsm_execute::effect::resolve(&current, &historical.effect_id).unwrap(),
        historical
    );
    assert_ne!(
        current.state.instance_machines["inst-contract-run"],
        historical.emitting_machine_id
    );
    let changed_records = current.records.clone();
    let changed_state = current.state.clone();
    drop(current);
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        client.call(28, "ping", obj([]));
        let current = Store::open_read_only(&store).unwrap();
        assert_eq!(
            current.records, changed_records,
            "saved good report bypassed the changed loaded table"
        );
        assert!(fsm_store::snapshot::store_states_eq(
            &changed_state,
            &current.state
        ));
        assert_eq!(fs::read_to_string(resource.join("calls")).unwrap(), "");
        assert!(!resource.join("work").exists());
        std::thread::sleep(Duration::from_millis(10));
    }
    let changed_report = client.check(
        29,
        obj([("machine", Value::Str("native_contract_draft".into()))]),
    );
    assert_eq!(
        changed_report.get("status").and_then(Value::as_str),
        Some("invalid")
    );
    assert_ne!(
        changed_report.get("contract_id"),
        repaired.get("contract_id")
    );
    retire(&mut client);
    fs::write(&handlers, canon_bytes(entry.get("handlers").unwrap())).unwrap();
    client = start(&entry);
    // No client requests after repair: observe the actual journal while the
    // restarted host autonomously executes, acknowledges and retires its handler.
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
                    4
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
        "check_prerequisite\nsuspend\nperform_work\nrestore\n"
    );
    assert_eq!(
        fs::read_to_string(resource.join("phase")).unwrap(),
        "active"
    );
    assert_eq!(
        fs::read_to_string(resource.join("work")).unwrap(),
        "first,second"
    );
    retire(&mut client);
    let writer = Store::open(&store).unwrap();
    assert_eq!(writer.state.execution.unresolved().count(), 0);
}

fn independent_table() -> fsm_execute::config::HandlerTable {
    let mut table = fsm_execute::config::HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{
        "effect":"check_prerequisite","argv":["/PRIVATE_STAGED_HELPER","handler-resource={resource}","handler-run={run}"],"timeout_ms":1000,
        "on_ok":{"event":"check_prerequisite_ok"},"on_failed":{"event":"check_prerequisite_failed"}
    }]}"#).unwrap();
    for effect in ["suspend", "perform_work", "restore"] {
        let mut handler = table.handlers["check_prerequisite"].clone();
        handler.effect = effect.into();
        handler.on_ok.as_mut().unwrap().event = format!("{effect}_ok");
        handler.on_failed.as_mut().unwrap().event = format!("{effect}_failed");
        table.handlers.insert(effect.into(), handler);
    }
    table
}

#[test]
fn authored_native_pair_has_an_independent_missing_argument_and_repair() {
    let table = independent_table();
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
        let findings = result.get("findings").unwrap().as_arr().unwrap();
        if expected == "invalid" {
            assert_eq!(findings.len(), 1);
            assert_eq!(
                findings[0].get("code").and_then(Value::as_str),
                Some("exec/contract_argument_missing")
            );
            assert_eq!(
                findings[0].get("path").and_then(Value::as_str),
                Some("/states/4/entry/emit/0/args/run")
            );
        } else {
            assert!(findings.is_empty());
        }
    }
}

#[test]
fn changed_loaded_table_invalidates_the_repaired_staged_contract() {
    let source = parse(br#"{"format":"fsm.handlers/1","handlers":[
      {"effect":"check_prerequisite","argv":["/operator/check","{resource}","{run}"],"timeout_ms":1000,"on_ok":{"event":"check_prerequisite_ok"},"on_failed":{"event":"check_prerequisite_failed"}},
      {"effect":"suspend","argv":["/operator/suspend","{resource}","{run}"],"timeout_ms":1000,"on_ok":{"event":"suspend_ok"},"on_failed":{"event":"suspend_failed"}},
      {"effect":"perform_work","argv":["/operator/work","{resource}","{run}"],"timeout_ms":1000,"on_ok":{"event":"perform_work_ok"},"on_failed":{"event":"perform_work_failed"}},
      {"effect":"restore","argv":["/operator/restore","{resource}","{run}"],"timeout_ms":1000,"on_ok":{"event":"restore_ok"},"on_failed":{"event":"restore_failed"}}
    ]}"#, &JsonLimits::DEFAULT).unwrap();
    let machine = fsm_core::spec::compile_accepted(&repair_machine()).unwrap();
    let analyze = |source: &Value| {
        let table = fsm_execute::config::HandlerTable::parse(
            &String::from_utf8(canon_bytes(source)).unwrap(),
        )
        .unwrap();
        fsm_execute::contract::analyze_contract(
            &machine,
            &BTreeMap::new(),
            &table,
            fsm_execute::contract::Limits::default(),
        )
        .unwrap()
    };
    let original = analyze(&source);
    let changed = analyze(&changed_table(&source));
    assert_eq!(
        original.status,
        fsm_execute::contract::CheckStatus::Compatible
    );
    assert_eq!(changed.status, fsm_execute::contract::CheckStatus::Invalid);
    assert_ne!(
        original.to_value().get("contract_id"),
        changed.to_value().get("contract_id")
    );
}

#[test]
fn staged_receiver_repair_preserves_the_original_pending_operation() {
    let mut store = Store::open_memory().unwrap();
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    store
        .define_machine_on(&mut clock, unchecked_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "unchecked_contract_draft",
            "original",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "original",
            "begin",
            &mut obj([]),
            "begin",
            None,
            &[],
        )
        .unwrap();
    let pending = store.state.instances["original"].pending.clone();
    let historical = fsm_execute::effect::resolve(&store, &pending[0]).unwrap();
    assert_eq!(
        historical.args.get("resource"),
        Some(&fsm_core::expr::eval::Val::Str(
            "resource with spaces; $(literal) \"quoted\"".into()
        ))
    );
    assert_eq!(
        historical.args.get("run"),
        Some(&fsm_core::expr::eval::Val::Str("run-1".into()))
    );
    let table = independent_table();
    assert_eq!(
        fsm_execute::contract::check_pending(&store, &historical, &table)
            .unwrap_err()
            .code,
        "exec/contract_invalid"
    );
    store
        .define_machine_on(&mut clock, repair_machine(), false, false)
        .unwrap();
    store
        .migrate_instance_on(&mut clock, "original", "native_contract_draft", "repair")
        .unwrap();
    assert_eq!(store.state.instances["original"].pending, pending);
    assert_eq!(
        fsm_execute::effect::resolve(&store, &pending[0]).unwrap(),
        historical
    );
    assert_ne!(
        historical.emitting_machine_id,
        store.state.instance_machines["original"]
    );
    fsm_execute::contract::check_pending(&store, &historical, &table).unwrap();
    for operation in ["check_prerequisite", "suspend", "perform_work", "restore"] {
        let effect =
            fsm_execute::effect::resolve(&store, &store.state.instances["original"].pending[0])
                .unwrap();
        assert_eq!(effect.effect_name, operation);
        fsm_execute::contract::check_pending(&store, &effect, &table).unwrap();
        store
            .ack_effect_outcome_on(
                &mut clock,
                "original",
                &effect.effect_id,
                &fsm_execute::rid::ack_rid(&effect.effect_id),
                "ok",
                None,
            )
            .unwrap();
        let event = format!("{operation}_ok");
        store
            .send_event_stamp_on(
                &mut clock,
                "original",
                &event,
                &mut obj([]),
                &fsm_execute::rid::event_rid(&effect.effect_id, &event),
                None,
                &[],
            )
            .unwrap();
    }
    assert_eq!(
        store.state.instances["original"].status,
        fsm_core::machine::Status::Completed
    );
    assert!(store.state.instances["original"].pending.is_empty());
}
