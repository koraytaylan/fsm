use fsm_cli::mcp::tools::{registry, validate_args};
use fsm_core::json::Value;
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
            "effect_ack",
            "instance_cancel",
            "instance_get",
            "instance_list",
            "instance_history",
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
            "instance_create" | "instance_send" | "effect_ack" | "instance_cancel" => {
                assert!(req.contains(&"request_id"), "{}", t.name);
            }
            n if n.starts_with("instance_") && n != "instance_list" => {
                assert!(req.contains(&"instance_id"), "{n}");
            }
            _ => {}
        }
        let out = (t.output_schema)();
        assert_eq!(
            out.get("additionalProperties").and_then(Value::as_bool),
            Some(true)
        );
        assert!(validate_args(&out, &Value::Obj(BTreeMap::new())).is_ok());
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
                    "object" => Value::Obj(BTreeMap::new()),
                    "boolean" => Value::Bool(false),
                    "number" => Value::Num("1".into()),
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
    assert!(err.details.get("field").and_then(Value::as_str) == Some("request_id"));

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
