//! Explicit native selection cannot dispatch an unclaimed direct child.

use fsm_core::json::Value;
use fsm_execute::run::{McpCall, Runner};
use std::collections::BTreeMap;

#[test]
fn native_selection_refuses_process_and_mcp_primitive_before_capture_creation() {
    let mut runner = Runner::new_native().unwrap();
    let captures = || {
        let mut entries: Vec<_> = std::fs::read_dir(runner.scratch_dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        entries.sort();
        entries
    };
    let before = captures();
    let scratch = runner.scratch_dir().to_owned();
    let argv = vec![
        env!("CARGO_BIN_EXE_fsm-containment-authority").into(),
        "--help".into(),
    ];
    let call = McpCall {
        tool: "notify".into(),
        arguments: Value::Obj(BTreeMap::new()),
    };
    for invocation in [None, Some(&call)] {
        assert_eq!(
            runner
                .spawn("unclaimed/1/0".into(), &argv, invocation)
                .unwrap_err()
                .code,
            "exec/mode"
        );
        assert!(runner.running_effects().is_empty());
        assert!(runner.finished_effects().is_empty());
        let mut after: Vec<_> = std::fs::read_dir(&scratch)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        after.sort();
        assert_eq!(after, before);
    }
}
