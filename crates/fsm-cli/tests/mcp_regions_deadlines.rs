//! MCP surface proofs for parallel regions and deadline polling.
//!
//! The tests pin registry and schema exposure, then drive a timed parallel
//! instance through JSON-shaped tool arguments as an external caller would.

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{dispatch, names, registry};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

fn temporary_store() -> Store {
    Store::open_memory().unwrap()
}

fn timed_parallel_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"mcp_timed_parallel",
            "context":[],
            "events":[
                {"name":"finish","fields":[]},
                {"name":"skip","fields":[]}
            ],
            "regions":[
                {
                    "name":"timer",
                    "states":[
                        {"name":"waiting"},
                        {"name":"expired","terminal":true}
                    ],
                    "initial":"waiting"
                },
                {
                    "name":"work",
                    "states":[
                        {"name":"working"},
                        {"name":"done","terminal":true}
                    ],
                    "initial":"working"
                }
            ],
            "on_unhandled":"ignore",
            "transitions":[{"from":"working","on":"finish","to":"done"}],
            "deadlines":[{
                "name":"expire",
                "from":"waiting",
                "after":"dur(10, ms)",
                "to":"expired"
            }]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn deadline_poll_is_registered_after_instance_send_with_configuration_schema() {
    let tool_names = names();
    assert_eq!(tool_names.len(), 18);
    let send = tool_names
        .iter()
        .position(|name| *name == "instance_send")
        .unwrap();
    assert_eq!(tool_names[send + 1], "deadline_poll");

    let poll = registry()
        .into_iter()
        .find(|tool| tool.name == "deadline_poll")
        .unwrap();
    let input = (poll.input_schema)();
    let required = input.get("required").and_then(Value::as_arr).unwrap();
    assert!(
        required
            .iter()
            .any(|value| value.as_str() == Some("instance_id"))
    );
    assert!(
        required
            .iter()
            .any(|value| value.as_str() == Some("request_id"))
    );
    assert!(
        input
            .get("properties")
            .and_then(|value| value.get("expect_seq"))
            .is_some()
    );

    let output = (poll.output_schema)();
    let properties = output.get("properties").unwrap();
    assert_eq!(
        properties
            .get("configuration")
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str),
        Some("object")
    );
    assert!(properties.get("deadlines_pending").is_some());
    let required = output.get("required").and_then(Value::as_arr).unwrap();
    assert!(!required.iter().any(|value| value.as_str() == Some("state")));
}

#[test]
fn deadline_poll_advances_one_region_without_a_fake_primary_leaf() {
    let mut store = temporary_store();
    let mut define_clock = FixedClock::new(1, 1);
    let defined = dispatch(
        &mut store,
        &mut define_clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), timed_parallel_machine())])),
    )
    .unwrap();
    let summary = defined.get("summary").unwrap();
    assert_eq!(
        summary.get("topology").and_then(Value::as_str),
        Some("parallel")
    );
    assert_eq!(summary.get("deadlines").and_then(Value::as_num), Some("1"));
    assert!(summary.get("initial").is_none());

    let analyzed = dispatch(
        &mut store,
        &mut define_clock,
        "machine_analyze",
        &Value::Obj(BTreeMap::from([(
            "machine".into(),
            Value::Str("mcp_timed_parallel".into()),
        )])),
    )
    .unwrap();
    assert!(analyzed.get("reachability").is_some());
    let diagram = dispatch(
        &mut store,
        &mut define_clock,
        "machine_diagram",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("mcp_timed_parallel".into())),
            ("format".into(), Value::Str("mermaid".into())),
        ])),
    )
    .unwrap();
    assert!(
        diagram
            .get("diagram")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("after dur(10, ms)"))
    );
    let simulated = dispatch(
        &mut store,
        &mut define_clock,
        "simulate",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("mcp_timed_parallel".into())),
            ("events".into(), Value::Arr(Vec::new())),
        ])),
    )
    .unwrap();
    assert_eq!(
        simulated
            .get("initial")
            .and_then(|value| value.get("configuration"))
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str),
        Some("parallel")
    );
    assert!(
        simulated
            .get("initial")
            .and_then(|value| value.get("state"))
            .is_none()
    );

    let mut create_clock = FixedClock::new(100, 1);
    let created = dispatch(
        &mut store,
        &mut create_clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("mcp_timed_parallel".into())),
            ("request_id".into(), Value::Str("create".into())),
        ])),
    )
    .unwrap();
    assert_eq!(
        created
            .get("configuration")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str),
        Some("parallel")
    );
    assert!(created.get("leaf").is_none());
    assert!(created.get("state").is_none());
    assert_eq!(
        created
            .get("deadlines_pending")
            .and_then(Value::as_arr)
            .unwrap()[0]
            .get("due_ms")
            .and_then(Value::as_str),
        Some("110")
    );
    let listed = dispatch(
        &mut store,
        &mut create_clock,
        "instance_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    let row = &listed.get("instances").and_then(Value::as_arr).unwrap()[0];
    assert_eq!(
        row.get("configuration")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str),
        Some("parallel")
    );
    assert!(row.get("state").is_none());

    let ignored = dispatch(
        &mut store,
        &mut create_clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-create".into())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([("name".into(), Value::Str("skip".into()))])),
            ),
            ("request_id".into(), Value::Str("ignored".into())),
        ])),
    )
    .unwrap();
    assert_eq!(ignored.get("ignored").and_then(Value::as_bool), Some(true));
    let ignored_transition = ignored.get("transition").and_then(Value::as_obj).unwrap();
    assert!(ignored_transition.get("source_state").is_none());
    let send_schema = (registry()
        .iter()
        .find(|tool| tool.name == "instance_send")
        .unwrap()
        .output_schema)();
    fsm_cli::mcp::tools::validate_args(&send_schema, &ignored).unwrap();

    let mut early_clock = FixedClock::new(109, 1);
    let early = dispatch(
        &mut store,
        &mut early_clock,
        "deadline_poll",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-create".into())),
            ("request_id".into(), Value::Str("early".into())),
        ])),
    )
    .unwrap();
    assert_eq!(
        early.get("deadline_not_due").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        early.get("next_due_ms").and_then(Value::as_str),
        Some("110")
    );

    let mut due_clock = FixedClock::new(110, 1);
    let applied = dispatch(
        &mut store,
        &mut due_clock,
        "deadline_poll",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-create".into())),
            ("request_id".into(), Value::Str("due".into())),
        ])),
    )
    .unwrap();
    assert_eq!(
        applied.get("deadline_applied").and_then(Value::as_bool),
        Some(true)
    );
    assert!(applied.get("leaf").is_none());
    assert!(applied.get("state").is_none());
    assert_eq!(
        applied
            .get("configuration")
            .and_then(|value| value.get("leaves"))
            .and_then(|value| value.get("timer"))
            .and_then(Value::as_str),
        Some("expired")
    );
    assert_eq!(
        applied
            .get("configuration")
            .and_then(|value| value.get("leaves"))
            .and_then(|value| value.get("work"))
            .and_then(Value::as_str),
        Some("working")
    );
    assert_eq!(
        applied
            .get("transition")
            .and_then(|value| value.get("region"))
            .and_then(Value::as_str),
        Some("timer")
    );
}
