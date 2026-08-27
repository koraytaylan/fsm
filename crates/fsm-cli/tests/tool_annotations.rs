//! Hints a host can act on, derived from facts the code already keeps.
//!
//! The point of asserting these against `MUTATING_TOOLS` rather than against
//! a list of expected values is that a second list is exactly the failure
//! being avoided: a `readOnlyHint` that disagrees with the read/write split
//! would have a host auto-approving a writer.
//!
//! Plan 0013 task 6201.

use std::collections::BTreeSet;

use fsm_cli::mcp::tools::{MUTATING_TOOLS, annotations, registry, tools_list_result};
use fsm_core::json::Value;

fn listed() -> Vec<Value> {
    tools_list_result()
        .get("tools")
        .and_then(Value::as_arr)
        .expect("a tools array")
        .to_vec()
}

fn hint(tool: &Value, name: &str) -> bool {
    tool.get("annotations")
        .and_then(|a| a.get(name))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("{name} missing from {tool:?}"))
}

#[test]
fn every_tool_carries_a_title_and_four_hints() {
    let tools = listed();
    assert_eq!(tools.len(), registry().len());
    for tool in &tools {
        let name = tool.get("name").and_then(Value::as_str).unwrap();
        let title = tool.get("title").and_then(Value::as_str).unwrap_or("");
        assert!(!title.is_empty(), "{name} has no title");
        for needed in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            let _ = hint(tool, needed);
        }
    }
}

#[test]
fn the_hints_are_derived_from_the_read_write_split() {
    for tool in &listed() {
        let name = tool.get("name").and_then(Value::as_str).unwrap();
        let mutating = MUTATING_TOOLS.contains(&name);
        assert_eq!(
            hint(tool, "readOnlyHint"),
            !mutating,
            "{name}: readOnlyHint disagrees with MUTATING_TOOLS"
        );
        assert_eq!(
            hint(tool, "idempotentHint"),
            mutating,
            "{name}: every mutating tool is keyed by request_id and exactly-once"
        );
    }
}

#[test]
fn the_derivation_is_live_rather_than_a_coincidence() {
    // A tool that is not in the registry at all still gets hints, and they
    // follow the constant rather than the current registry's values: this is
    // the check that the two are one expression and not two lists that happen
    // to agree today.
    let invented = annotations("machine_create_v2");
    assert_eq!(invented.get("readOnlyHint"), Some(&Value::Bool(true)));
    assert_eq!(invented.get("idempotentHint"), Some(&Value::Bool(false)));
    for name in MUTATING_TOOLS {
        let derived = annotations(name);
        assert_eq!(
            derived.get("readOnlyHint"),
            Some(&Value::Bool(false)),
            "{name}"
        );
        assert_eq!(
            derived.get("idempotentHint"),
            Some(&Value::Bool(true)),
            "{name}"
        );
    }
}

#[test]
fn cancelling_an_instance_is_the_only_destructive_tool() {
    let destructive: Vec<String> = listed()
        .iter()
        .filter(|tool| hint(tool, "destructiveHint"))
        .map(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(destructive, ["instance_cancel"]);
}

#[test]
fn nothing_here_reaches_the_open_world() {
    // Effects reach the world; the executor runs them. No tool call on this
    // surface touches anything but one data directory.
    for tool in &listed() {
        let name = tool.get("name").and_then(Value::as_str).unwrap();
        assert!(!hint(tool, "openWorldHint"), "{name}");
    }
}

#[test]
fn titles_are_distinct_and_are_not_the_names_again() {
    let mut seen = BTreeSet::new();
    for tool in &listed() {
        let name = tool.get("name").and_then(Value::as_str).unwrap();
        let title = tool.get("title").and_then(Value::as_str).unwrap();
        assert_ne!(title, name, "a title that repeats the name says nothing");
        assert!(seen.insert(title.to_string()), "two tools titled {title}");
    }
}
