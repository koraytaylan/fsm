use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{registry, validate_args};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use std::collections::BTreeMap;

#[test]
fn registry_order() {
    let names: Vec<_> = registry().into_iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "machine_create",
            "machine_list",
            "machine_get",
            "machine_analyze",
            "machine_diagram",
            "instance_create",
            "instance_send",
            "deadline_poll",
            "effect_ack",
            "instance_cancel",
            "instance_migrate",
            "invocation_start",
            "invocation_return",
            "signal_deliver",
            "instance_get",
            "instance_list",
            "instance_history",
            "explain_step",
            "journal_verify",
            "journal_replay",
            "store_doctor",
            "instance_elicit",
            "simulate",
        ]
    );
}

#[test]
fn input_schemas_strict() {
    for t in registry() {
        let s = (t.input_schema)();
        assert_eq!(s.get("type").and_then(Value::as_str), Some("object"));
        assert_eq!(
            s.get("additionalProperties").and_then(Value::as_bool),
            Some(false)
        );
        let req: Vec<&str> = s
            .get("required")
            .and_then(Value::as_arr)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        match t.name {
            "machine_create" => assert!(req.contains(&"spec")),
            "instance_create" | "instance_send" | "deadline_poll" | "effect_ack"
            | "instance_cancel" => {
                assert!(req.contains(&"request_id"), "{}", t.name);
            }
            n if n.starts_with("instance_") && n != "instance_list" => {
                assert!(req.contains(&"instance_id"), "{n}");
            }
            _ => {}
        }
        let out = (t.output_schema)();
        let req = out.get("required").and_then(Value::as_arr).unwrap();
        assert!(!req.is_empty(), "{} output required empty", t.name);
        assert!(
            validate_args(&out, &Value::Obj(BTreeMap::new())).is_err(),
            "{} empty object must fail required output fields",
            t.name
        );
    }
}

#[test]
fn validate_accept_and_reject() {
    for t in registry() {
        let mut args = BTreeMap::new();
        for r in (t.input_schema)()
            .get("required")
            .and_then(Value::as_arr)
            .unwrap_or(&[])
        {
            let name = r.as_str().unwrap();
            let prop = (t.input_schema)()
                .get("properties")
                .and_then(|p| p.get(name))
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            let ty = prop.get("type").and_then(Value::as_str).unwrap_or("string");
            let v = if let Some(en) = prop
                .get("enum")
                .and_then(Value::as_arr)
                .and_then(|a| a.first())
            {
                en.clone()
            } else {
                match ty {
                    "object" => {
                        let mut inner = BTreeMap::new();
                        for nr in prop.get("required").and_then(Value::as_arr).unwrap_or(&[]) {
                            if let Some(n) = nr.as_str() {
                                inner.insert(n.into(), Value::Str("x".into()));
                            }
                        }
                        Value::Obj(inner)
                    }
                    "boolean" => Value::Bool(false),
                    // `integer` since plan 0014: `explain_step` is the first
                    // tool whose *required* argument is a number, and a
                    // sample builder that fell through to a string would
                    // fail it for the wrong reason.
                    "number" | "integer" => Value::Num("1".into()),
                    "array" => Value::Arr(vec![]),
                    _ => Value::Str("x".into()),
                }
            };
            args.insert(name.into(), v);
        }
        assert!(
            validate_args(&(t.input_schema)(), &Value::Obj(args)).is_ok(),
            "{}",
            t.name
        );
    }
    let send = registry()
        .into_iter()
        .find(|t| t.name == "instance_send")
        .unwrap();
    let err = validate_args(
        &(send.input_schema)(),
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("event".into(), Value::Obj(BTreeMap::new())),
        ])),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/args_invalid");
    let fields = err.details.get("fields").and_then(Value::as_arr).unwrap();
    let names: Vec<&str> = fields.iter().filter_map(Value::as_str).collect();
    assert!(
        names
            .iter()
            .any(|n| *n == "request_id" || *n == "event.name"),
        "{err:?}"
    );

    let list = registry()
        .into_iter()
        .find(|t| t.name == "machine_list")
        .unwrap();
    let err = validate_args(
        &(list.input_schema)(),
        &Value::Obj(BTreeMap::from([("limit".into(), Value::Str("x".into()))])),
    )
    .unwrap_err();
    assert!(err.details.get("expected").is_some());

    let err = validate_args(
        &(list.input_schema)(),
        &Value::Obj(BTreeMap::from([("nope".into(), Value::Bool(true))])),
    )
    .unwrap_err();
    assert_eq!(
        err.details.get("field").and_then(Value::as_str),
        Some("nope")
    );

    let diag = registry()
        .into_iter()
        .find(|t| t.name == "machine_diagram")
        .unwrap();
    let err = validate_args(
        &(diag.input_schema)(),
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("m".into())),
            ("format".into(), Value::Str("png".into())),
        ])),
    )
    .unwrap_err();
    assert!(
        err.details
            .get("expected")
            .and_then(Value::as_str)
            .unwrap()
            .contains("mermaid")
    );
}

#[test]
fn machine_list_and_history_match_independent_contracts() {
    // These schemas are intentionally authored in the test instead of being
    // assembled from the registry helpers under test.
    let machine_list_contract = parse(
        br#"{"type":"object","required":["machines"],"additionalProperties":false,"properties":{"machines":{"type":"array","items":{"type":"object","required":["machine_id","name","defined_seq","topology","regions","states","events","deadlines","instances"],"additionalProperties":false,"properties":{"machine_id":{"type":"string"},"name":{"type":"string"},"defined_seq":{"type":"number"},"topology":{"type":"string"},"regions":{"type":"number"},"states":{"type":"number"},"events":{"type":"number"},"deadlines":{"type":"number"},"instances":{"type":"object","required":["running","completed","cancelled"],"additionalProperties":false,"properties":{"running":{"type":"number"},"completed":{"type":"number"},"cancelled":{"type":"number"}}}}}},"next_cursor":{"type":"string"}}}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let history_contract = parse(
        br#"{"type":"object","required":["instance_id","entries","chain_verified"],"additionalProperties":true,"properties":{"instance_id":{"type":"string"},"entries":{"type":"array","items":{"type":"object","required":["seq","ts","kind","hash"],"additionalProperties":true,"properties":{"seq":{"type":"number"},"ts":{"type":"number"},"kind":{"type":"string"},"event":{"type":"string"},"request_id":{"type":"string"},"from_leaf":{"type":"string"},"to_leaf":{"type":"string"},"context_after":{"type":"object"},"trace":{"type":"object"},"hash":{"type":"string"}}}},"next_from_seq":{"type":"number"},"chain_verified":{"type":"boolean"}}}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();

    let case = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_create",
        &Value::Obj(BTreeMap::from([("spec".into(), case)])),
    )
    .unwrap();
    let created = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str("case_review".into())),
            ("request_id".into(), Value::Str("create-1".into())),
        ])),
    )
    .unwrap();
    let instance_id = created
        .get("instance_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.clone())),
            (
                "event".into(),
                Value::Obj(BTreeMap::from([(
                    "name".into(),
                    Value::Str("docs_ok".into()),
                )])),
            ),
            ("request_id".into(), Value::Str("send-1".into())),
        ])),
    )
    .unwrap();

    let listed = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "machine_list",
        &Value::Obj(BTreeMap::new()),
    )
    .unwrap();
    validate_args(&machine_list_contract, &listed).unwrap();
    let row = &listed.get("machines").and_then(Value::as_arr).unwrap()[0];
    assert_eq!(row.get("states").and_then(Value::as_num), Some("8"));
    assert_eq!(row.get("events").and_then(Value::as_num), Some("6"));
    assert!(row.get("state_count").is_none(), "{row:?}");
    assert!(row.get("event_count").is_none(), "{row:?}");

    let history = fsm_cli::mcp::tools::dispatch(
        &mut store,
        &mut clock,
        "instance_history",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id)),
            ("include_trace".into(), Value::Bool(true)),
        ])),
    )
    .unwrap();
    validate_args(&history_contract, &history).unwrap();
    let applied = history
        .get("entries")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .find(|entry| entry.get("kind").and_then(Value::as_str) == Some("EventApplied"))
        .unwrap();
    assert_eq!(
        applied.get("event").and_then(Value::as_str),
        Some("docs_ok")
    );
    assert!(applied.get("trace").and_then(Value::as_obj).is_some());

    let reg = registry();
    let list_item = (reg
        .iter()
        .find(|t| t.name == "machine_list")
        .unwrap()
        .output_schema)()
    .get("properties")
    .and_then(|p| p.get("machines"))
    .and_then(|a| a.get("items"))
    .cloned()
    .unwrap();
    assert_eq!(
        list_item
            .get("properties")
            .and_then(|p| p.get("states"))
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str),
        Some("number")
    );
    assert_eq!(
        list_item
            .get("properties")
            .and_then(|p| p.get("events"))
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str),
        Some("number")
    );
    let history_item = (reg
        .iter()
        .find(|t| t.name == "instance_history")
        .unwrap()
        .output_schema)()
    .get("properties")
    .and_then(|p| p.get("entries"))
    .and_then(|a| a.get("items"))
    .cloned()
    .unwrap();
    assert_eq!(
        history_item.get("type").and_then(Value::as_str),
        Some("object")
    );
}
