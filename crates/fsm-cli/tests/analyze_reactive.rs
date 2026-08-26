//! `machine_analyze` reports the reactive surface, validates against its
//! own declared output schema, and stays additive for a plain machine.
//!
//! Plan 0009 task 4702.

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{dispatch, registry, validate_args};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

const REACTIVE: &[u8] = br#"{"format":"fsm.machine/1","name":"reactive_parallel","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true},{"name":"stop","fields":[]}],"regions":[{"name":"a","initial":"review","states":[{"name":"review","initial":"open","states":[{"name":"open"},{"name":"approved","final":true}]},{"name":"a_done","terminal":true}]},{"name":"b","initial":"audit","states":[{"name":"audit","initial":"pending","states":[{"name":"pending"},{"name":"checked","final":true}]},{"name":"joined"},{"name":"b_done","terminal":true}]}],"transitions":[{"from":"open","on":"go","to":"approved"},{"from":"review","on":"$done.state.review","to":"a_done"},{"from":"pending","if":"ctx.n > 0","to":"checked"},{"from":"pending","on":"stop","to":"checked"},{"from":"audit","on":"$done.state.audit","to":"joined"},{"from":"joined","on":"$done.region.a","to":"b_done"}]}"#;

fn analyzed(source: &[u8]) -> Value {
    let spec = parse(source, &JsonLimits::DEFAULT).unwrap();
    let name = spec
        .get("name")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), spec)])),
    )
    .unwrap();
    let out = dispatch(
        &mut store,
        &mut clock,
        "machine_analyze",
        &Value::Obj(BTreeMap::from([("machine".into(), Value::Str(name))])),
    )
    .unwrap();
    let tool = registry()
        .into_iter()
        .find(|t| t.name == "machine_analyze")
        .unwrap();
    validate_args(&(tool.output_schema)(), &out).unwrap();
    out
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_arr)
        .unwrap_or_else(|| panic!("{key} is an array: {value:?}"))
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn eventless_count(value: &Value) -> Option<&str> {
    value.get("eventless_transitions").and_then(Value::as_num)
}

#[test]
fn machine_analyze_reports_the_reactive_surface_under_its_schema() {
    let out = analyzed(REACTIVE);
    assert_eq!(eventless_count(&out), Some("1"));
    assert_eq!(
        strings(&out, "done_events"),
        ["$done.state.review", "$done.state.audit", "$done.region.a"]
    );
    assert_eq!(strings(&out, "unhandled_done_events"), ["$done.region.b"]);
    assert_eq!(strings(&out, "internal_events"), ["tick"]);
    assert!(out.get("findings").is_some() && out.get("completeness").is_some());
}

#[test]
fn a_plain_machine_reports_an_empty_reactive_surface_under_the_same_schema() {
    let out = analyzed(include_bytes!(
        "../../fsm-core/tests/fixtures/machines/case_review.json"
    ));
    assert_eq!(eventless_count(&out), Some("0"));
    assert!(strings(&out, "done_events").is_empty());
    assert!(strings(&out, "unhandled_done_events").is_empty());
    assert!(strings(&out, "internal_events").is_empty());
}
