use fsm_cli::mcp::prompts::{INSTRUCTIONS, get, list};
use fsm_core::json::Value;
use std::collections::BTreeMap;

#[test]
fn list_one_prompt() {
    let v = list();
    let arr = v.get("prompts").and_then(Value::as_arr).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0].get("name").and_then(Value::as_str),
        Some("author_machine")
    );
    let args = arr[0].get("arguments").and_then(Value::as_arr).unwrap();
    assert_eq!(args[0].get("name").and_then(Value::as_str), Some("goal"));
    assert_eq!(args[0].get("required").and_then(Value::as_bool), Some(true));
}

#[test]
fn get_interpolates() {
    let args = Value::Obj(BTreeMap::from([(
        "goal".into(),
        Value::Str("track a mediation case".into()),
    )]));
    let v = get("author_machine", Some(&args)).unwrap();
    let text = v.get("messages").and_then(Value::as_arr).unwrap()[0]
        .get("content")
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap();
    assert!(text.contains("track a mediation case"));
    let spec = text.find("fsm://docs/spec").unwrap();
    let dry = text.find("dry_run").unwrap();
    let persist = text.find("persist").unwrap();
    let sim = text.find("simulate").unwrap();
    let inst = text.find("instance_create").unwrap();
    let send = text.find("instance_send").unwrap();
    assert!(spec < dry && dry < persist && persist < sim && sim < inst && inst < send);
}

#[test]
fn get_errors() {
    let err = get("author_machine", None).unwrap_err();
    assert!(err.details.get("field").and_then(Value::as_str) == Some("goal"));
    let err = get("nope", None).unwrap_err();
    assert!(err.hint.contains("author_machine"));
}

#[test]
fn instructions() {
    let n = INSTRUCTIONS.split_whitespace().count();
    assert!(n <= 130, "{n}");
    for p in [
        "enabled_events",
        "dry_run",
        "effect_ack",
        "request_id",
        "JSON strings",
    ] {
        assert!(INSTRUCTIONS.contains(p), "{p}");
    }
}
