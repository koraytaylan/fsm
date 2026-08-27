//! The form a person fills in, generated from the machine's own types.
//!
//! Plan 0013 task 6402.

use fsm_cli::mcp::elicit::{payload_from_content, schema_for_event};
use fsm_core::json::{JsonLimits, Value, parse};

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

/// One event with one field of every declared type, and one with none.
const CASE: &str = r#"{"format":"fsm.machine/1","name":"elicit_case","enums":{"Grade":["low","mid","high"]},"states":[{"name":"open"},{"name":"held"},{"name":"shut","terminal":true}],"initial":"open","context":[],"events":[{"name":"record","fields":[{"name":"note","ty":"str"},{"name":"count","ty":"int"},{"name":"ok","ty":"bool"},{"name":"grade","ty":{"enum":"Grade"}},{"name":"amount","ty":{"decimal":"2"}},{"name":"at","ty":"timestamp"},{"name":"wait","ty":"duration"}]},{"name":"nod","fields":[]},{"name":"tick","fields":[],"internal":true}],"transitions":[{"from":"open","on":"record","to":"held"},{"from":"open","on":"nod","to":"held"},{"from":"open","on":"tick","to":"shut"}]}"#;

fn machine() -> fsm_core::machine::CompiledMachine {
    let spec = fsm_core::spec::parse_machine(&value(CASE)).expect("the fixture parses");
    fsm_core::spec::compile(spec).expect("the fixture compiles")
}

fn schema(event: &str) -> Value {
    schema_for_event(&machine(), event).expect("a declared, sendable event")
}

fn property(schema: &Value, field: &str) -> Value {
    schema
        .get("properties")
        .and_then(|p| p.get(field))
        .unwrap_or_else(|| panic!("{field} missing from {schema:?}"))
        .clone()
}

#[test]
fn one_field_of_each_type_produces_the_documented_schema() {
    let got = String::from_utf8(fsm_core::canon::canon_bytes(&schema("record"))).unwrap();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/elicit/record.schema.json");
    if std::env::var("REGEN_ELICIT").ok().as_deref() == Some("1") {
        std::fs::write(&path, &got).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(got, expected.trim_end());
}

#[test]
fn a_decimal_is_a_string_that_says_how_many_digits() {
    let amount = property(&schema("record"), "amount");
    assert_eq!(amount.get("type").and_then(Value::as_str), Some("string"));
    assert_ne!(
        amount.get("type").and_then(Value::as_str),
        Some("number"),
        "a number invites a float, which is the one value this engine refuses anywhere"
    );
    assert_eq!(
        amount.get("description").and_then(Value::as_str),
        Some("decimal with exactly 2 fraction digits")
    );
}

#[test]
fn a_timestamp_is_a_string_a_client_knows_how_to_render() {
    let at = property(&schema("record"), "at");
    assert_eq!(at.get("type").and_then(Value::as_str), Some("string"));
    assert_eq!(at.get("format").and_then(Value::as_str), Some("date-time"));
    let wait = property(&schema("record"), "wait");
    assert_eq!(wait.get("type").and_then(Value::as_str), Some("string"));
    assert!(
        wait.get("description")
            .and_then(Value::as_str)
            .is_some_and(|d| d.contains("duration")),
        "{wait:?}"
    );
}

#[test]
fn an_enum_carries_its_variants_in_the_order_they_were_declared() {
    let grade = property(&schema("record"), "grade");
    assert_eq!(grade.get("type").and_then(Value::as_str), Some("string"));
    let variants: Vec<&str> = grade
        .get("enum")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(variants, ["low", "mid", "high"]);
}

#[test]
fn every_field_is_required_and_nothing_nests() {
    let schema = schema("record");
    assert_eq!(schema.get("type").and_then(Value::as_str), Some("object"));
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        required,
        ["note", "count", "ok", "grade", "amount", "at", "wait"],
        "validate_event demands the exact declared set"
    );
    for (name, property) in schema.get("properties").and_then(Value::as_obj).unwrap() {
        let kind = property.get("type").and_then(Value::as_str).unwrap();
        assert!(
            matches!(kind, "string" | "integer" | "boolean"),
            "{name} is {kind}, which the protocol forbids in an elicitation schema"
        );
    }
}

#[test]
fn an_event_with_no_fields_elicits_an_empty_form() {
    let schema = schema("nod");
    assert_eq!(
        schema
            .get("properties")
            .and_then(Value::as_obj)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        schema
            .get("required")
            .and_then(Value::as_arr)
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn nobody_can_be_asked_to_send_what_only_the_machine_raises() {
    let error = schema_for_event(&machine(), "tick").unwrap_err();
    assert_eq!(error.code, "req/event_internal");
    let error = schema_for_event(&machine(), "nonesuch").unwrap_err();
    assert_eq!(error.code, "req/event_unknown");
}

#[test]
fn a_conforming_answer_becomes_a_payload_the_engine_accepts() {
    let content = value(
        r#"{"note":"seen","count":7,"ok":true,"grade":"mid","amount":"12.50","at":"1700000000000","wait":"250"}"#,
    );
    let payload = payload_from_content(&machine(), "record", &content).expect("a valid answer");
    // The integer property came back as a JSON number and reaches the engine
    // as the string it takes.
    assert_eq!(
        payload.get("count").and_then(Value::as_str),
        Some("7"),
        "{payload:?}"
    );
    assert_eq!(payload.get("ok").and_then(Value::as_bool), Some(true));
    // And the engine agrees, through the same function the send path uses.
    fsm_core::step::validate_event(&machine(), "record", &payload).expect("accepted");
}

#[test]
fn an_answer_that_contradicts_its_own_form_is_refused_like_any_payload() {
    let base = r#"{"note":"seen","count":7,"ok":true,"grade":"mid","amount":AMOUNT,"at":"1700000000000","wait":"250"}"#;

    // A raw number where the schema said string: the one value this engine
    // refuses everywhere.
    let error = payload_from_content(
        &machine(),
        "record",
        &value(&base.replace("AMOUNT", "12.5")),
    )
    .unwrap_err();
    assert_eq!(error.code, "req/number_token");

    // Wrong scale, missing field, unknown field: the ordinary vocabulary.
    // (`"12.5"` is *accepted* for a two-digit decimal — the engine reads a
    // shorter fraction as the same number — so the refusal is a longer one.)
    let error = payload_from_content(
        &machine(),
        "record",
        &value(&base.replace("AMOUNT", "\"12.505\"")),
    )
    .unwrap_err();
    assert_eq!(error.code, "req/field_scale");

    let missing = r#"{"note":"seen","ok":true,"grade":"mid","amount":"12.50","at":"1700000000000","wait":"250"}"#;
    assert_eq!(
        payload_from_content(&machine(), "record", &value(missing))
            .unwrap_err()
            .code,
        "req/field_missing"
    );

    let extra = base
        .replace("AMOUNT", "\"12.50\"")
        .replace(r#""wait":"250""#, r#""wait":"250","extra":"no""#);
    assert_eq!(
        payload_from_content(&machine(), "record", &value(&extra))
            .unwrap_err()
            .code,
        "req/field_unknown",
        "an answer with a key nobody asked for is refused, never silently dropped"
    );

    let wrong_variant = base
        .replace("AMOUNT", "\"12.50\"")
        .replace(r#""grade":"mid""#, r#""grade":"enormous""#);
    assert_eq!(
        payload_from_content(&machine(), "record", &value(&wrong_variant))
            .unwrap_err()
            .code,
        "req/field_type"
    );
}
