//! Protected bindings preserve the independently authored MCP workflow contract.
use super::*;

pub(super) fn stage(staging: &Path) -> PathBuf {
    let artifact = staging.join("workflow-mcp-handler");
    super::stage_artifact(
        &artifact,
        "FSM_NATIVE_WORKFLOW_HANDLER_ARTIFACT",
        "FSM_NATIVE_WORKFLOW_HANDLER_SHA256",
    );
    artifact
}

pub(super) fn table(helper: &Path, resource: &Path) -> Value {
    let mut table = fsm_core::json::parse(
        include_bytes!(
            "../../../fsm-cli/tests/fixtures/contract/workflow-mcp-repaired.handlers.json"
        ),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap();
    let Value::Obj(fields) = &mut table else {
        unreachable!()
    };
    let Value::Arr(handlers) = fields.get_mut("handlers").unwrap() else {
        unreachable!()
    };
    for handler in handlers {
        let Value::Obj(fields) = handler else {
            unreachable!()
        };
        // Fixed executable/directory bindings are private; tool names, nested
        // argument templates and every public field stay independently authored.
        fields.insert(
            "argv".into(),
            Value::Arr(
                [
                    helper.to_str().unwrap().to_owned(),
                    "workflow-mcp".into(),
                    resource.to_str().unwrap().to_owned(),
                    "compensate".into(),
                ]
                .into_iter()
                .map(Value::Str)
                .collect(),
            ),
        );
    }
    table
}

#[test]
fn staged_native_mcp_table_preserves_independent_report() {
    let machine = fsm_core::json::parse(
        include_bytes!("../../../fsm-cli/tests/fixtures/contract/workflow.machine.json"),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap();
    let compiled = fsm_core::spec::compile_accepted(&machine).unwrap();
    let table = fsm_execute::config::HandlerTable::parse(
        &String::from_utf8(canon_bytes(&table(
            Path::new("/protected/marker"),
            Path::new("/resource"),
        )))
        .unwrap(),
    )
    .unwrap();
    let expected =
        include_str!("../../../fsm-cli/tests/fixtures/contract/workflow-mcp-repaired.report.json")
            .replace("$MACHINE_ID", &compiled.machine_id);
    let expected =
        fsm_core::json::parse(expected.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        fsm_execute::contract::analyze_contract(
            &compiled,
            &std::collections::BTreeMap::new(),
            &table,
            fsm_execute::contract::Limits::default(),
        )
        .unwrap()
        .to_value(),
        expected
    );
}
