//! The exact independent staged fixture drives checks and real compensating work.

use super::*;

const MACHINE: &[u8] = include_bytes!("../../fixtures/contract/workflow.machine.json");

fn expected(source: &str) -> Value {
    let machine = parse(MACHINE, &JsonLimits::DEFAULT).unwrap();
    let compiled = fsm_core::spec::compile_accepted(&machine).unwrap();
    parse(
        source
            .replace("$MACHINE_ID", &compiled.machine_id)
            .as_bytes(),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn check_cli(entry: &Value, selector: &str, reference: &str, report: &Value) {
    let output = Command::new(path(entry, "cli"))
        .args(["--json", "--data-dir"])
        .arg(path(entry, "store"))
        .args(["execute", "--check", "--handlers"])
        .arg(path(entry, "store").join("contract-handlers.json"))
        .args([selector, reference])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let mut bytes = canon_bytes(report);
    bytes.push(b'\n');
    assert_eq!(output.stdout, bytes);
    assert!(output.stderr.is_empty(), "{output:?}");
}

pub(crate) fn run() {
    let entry = manifest();
    let store = path(&entry, "store");
    let resource = path(&entry, "resource");
    let original = entry.get("handlers").unwrap();
    let mut invalid = original.clone();
    let Value::Obj(fields) = &mut invalid else {
        unreachable!()
    };
    let Value::Arr(handlers) = fields.get_mut("handlers").unwrap() else {
        unreachable!()
    };
    let Value::Obj(recover) = handlers
        .iter_mut()
        .find(|handler| handler.get("effect").and_then(Value::as_str) == Some("recover"))
        .unwrap()
    else {
        unreachable!()
    };
    recover.insert(
        "on_ok".into(),
        obj([("event", Value::Str("undeclared".into()))]),
    );
    let table_path = store.join("contract-handlers.json");
    fs::write(&table_path, canon_bytes(&invalid)).unwrap();
    let machine_path = store.join("contract-machine.json");
    fs::write(&machine_path, MACHINE).unwrap();
    let invalid_report = expected(include_str!(
        "../../fixtures/contract/workflow-invalid.report.json"
    ));
    let good_report = expected(include_str!(
        "../../fixtures/contract/workflow-repaired.report.json"
    ));
    let records = Store::open_read_only(&store).unwrap().records.clone();
    check_cli(
        &entry,
        "--machine-file",
        machine_path.to_str().unwrap(),
        &invalid_report,
    );
    assert_eq!(Store::open_read_only(&store).unwrap().records, records);
    let mut client = start(&entry);
    let draft = parse(MACHINE, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        client.check(10, obj([("spec", draft.clone())])),
        invalid_report
    );
    assert_eq!(Store::open_read_only(&store).unwrap().records, records);
    for (identifier, name, arguments) in [
        (11, "machine_create", obj([("spec", draft)])),
        (
            12,
            "instance_create",
            obj([
                ("machine", Value::Str("contract_workflow".into())),
                ("request_id", Value::Str("staged-run".into())),
            ]),
        ),
    ] {
        let response = client.call(
            identifier,
            "tools/call",
            obj([("name", Value::Str(name.into())), ("arguments", arguments)]),
        );
        assert_ne!(
            response.get("isError"),
            Some(&Value::Bool(true)),
            "{response:?}"
        );
    }
    let refused = Store::open_read_only(&store).unwrap();
    let state = refused.state.clone();
    let records = refused.records.clone();
    let effect =
        fsm_execute::effect::resolve(&refused, &state.instances["inst-staged-run"].pending[0])
            .unwrap();
    drop(refused);
    assert_eq!(
        client.check(
            13,
            obj([("machine", Value::Str("contract_workflow".into()))])
        ),
        invalid_report
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        client.call(14, "ping", obj([]));
        let current = Store::open_read_only(&store).unwrap();
        assert_eq!(current.records, records);
        assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
        assert_eq!(fs::read_to_string(resource.join("calls")).unwrap(), "");
        assert!(!resource.join("work").exists());
        assert_eq!(
            fs::read_to_string(resource.join("phase")).unwrap(),
            "active"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    retire(&mut client);
    check_cli(&entry, "--machine", "contract_workflow", &invalid_report);
    let writer = Store::open(&store).unwrap();
    assert_eq!(writer.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&state, &writer.state));
    assert_eq!(
        fsm_execute::effect::resolve(&writer, &effect.effect_id).unwrap(),
        effect
    );
    drop(writer);
    fs::write(&table_path, canon_bytes(original)).unwrap();
    client = start(&entry);
    assert_eq!(
        client.check(
            15,
            obj([("machine", Value::Str("contract_workflow".into()))])
        ),
        good_report
    );
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        let current = Store::open_read_only(&store).unwrap();
        if current.state.instances["inst-staged-run"].status == fsm_core::machine::Status::Completed
            && current.state.execution.unresolved().count() == 0
        {
            for kind in [
                RecordKind::ExecutionClaimed,
                RecordKind::ExecutionStopped,
                RecordKind::ExecutionSettled,
            ] {
                assert_eq!(
                    current
                        .records
                        .iter()
                        .filter(|record| record.kind == kind)
                        .count(),
                    3
                );
            }
            break;
        }
        assert!(
            Instant::now() < deadline,
            "staged recovery did not complete with quiet clients"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        fs::read_to_string(resource.join("calls")).unwrap(),
        "inspect\nwork\nrecover\n"
    );
    assert_eq!(fs::read_to_string(resource.join("work")).unwrap(), "first");
    assert_eq!(
        fs::read_to_string(resource.join("phase")).unwrap(),
        "active"
    );
    retire(&mut client);
    assert_eq!(
        Store::open(&store)
            .unwrap()
            .state
            .execution
            .unresolved()
            .count(),
        0
    );
    assert_eq!(
        fsm_store::journal_io::verify(&store).health,
        fsm_store::journal_io::JournalHealth::Ok
    );
}
