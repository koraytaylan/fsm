//! Real degraded sessions retain draft evidence but discard execution authority.
use super::*;
use std::{fs, path::Path};

fn unopenable(data: &Path) {
    let mut store = Store::open(data).unwrap();
    store
        .define_machine_on(&mut FixedClock::new(2000, 0), draft(), false, false)
        .unwrap();
    drop(store);
    // Noncanonical retained bytes make both writer and read-only opens fail;
    // this does not change a version stamp or permit automatic tail repair.
    let segment = data.join("journal/seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&segment).unwrap();
    let opening = bytes.iter().position(|byte| *byte == b'{').unwrap();
    bytes.insert(opening + 1, b' ');
    fs::write(segment, bytes).unwrap();
    assert!(Store::open_read_only(data).is_err());
}

fn reports(client: &mut Client, data: &Path) {
    let baseline = files(data);
    let lock = fs::read(data.join("journal/LOCK")).unwrap();
    let tools = client.call(2, "tools/list", obj([]));
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
    let mut invalid = draft();
    let Value::Obj(fields) = &mut invalid else {
        unreachable!()
    };
    fields.insert("initial".into(), Value::Str("absent".into()));
    for (identifier, arguments, status) in [
        (3, obj([("spec", draft())]), "unknown"),
        (4, obj([("spec", invalid)]), "invalid"),
        (
            5,
            obj([("machine", Value::Str("simple".into()))]),
            "unknown",
        ),
    ] {
        let report = client.check(identifier, arguments);
        validate_args(schema, &report).unwrap();
        assert_eq!(report.get("status").and_then(Value::as_str), Some(status));
        assert_eq!(report.get("contract_id"), Some(&Value::Null));
        let scope = report.get("scope").unwrap();
        assert_eq!(scope.get("effects_checked"), Some(&Value::Bool(false)));
        assert_eq!(scope.get("outcomes_checked"), Some(&Value::Bool(false)));
        let findings = report.get("findings").unwrap().as_arr().unwrap();
        assert!(findings.iter().any(|finding| {
            finding
                .get("cause")
                .and_then(|cause| cause.get("mode"))
                .and_then(Value::as_str)
                == Some("degraded")
        }));
        if identifier == 3 {
            assert_eq!(report.get("effects").unwrap().as_arr().unwrap().len(), 1);
        } else if identifier == 5 {
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.get("code").and_then(Value::as_str)
                        == Some("exec/contract_definition_unknown"))
            );
        }
        assert!(
            !String::from_utf8(canon_bytes(&report))
                .unwrap()
                .contains("PRIVATE_WORKFLOW")
        );
        assert_eq!(files(data), baseline);
        assert_eq!(fs::read(data.join("journal/LOCK")).unwrap(), lock);
    }
    assert!(Store::open_read_only(data).is_err());
}

#[test]
fn real_degraded_stdio_preserves_draft_findings_without_store_or_table_authority() {
    let directory = Directory::new();
    let data = directory.0.join("data");
    unopenable(&data);
    let mut client = Client::start(&data, false);
    reports(&mut client, &data);
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn real_degraded_embedded_stdio_discards_loaded_private_table_without_check_writes() {
    let directory = Directory::new();
    let data = directory.0.join("data");
    unopenable(&data);
    let handlers = directory.0.join("handlers.json");
    fs::write(
        &handlers,
        include_bytes!("../fixtures/contract/workflow-mcp-repaired.handlers.json"),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
    command
        .arg("--data-dir")
        .arg(&data)
        .args(["serve", "--execute", "--handlers"])
        .arg(handlers)
        .stderr(Stdio::null());
    let mut client = Client::spawn(command);
    reports(&mut client, &data);
}
