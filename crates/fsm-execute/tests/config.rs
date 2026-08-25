//! The handler table is the executor's security boundary: every row here is
//! about what an operator's file is allowed to say, and what happens to a
//! value on its way into an argv.

use std::collections::BTreeMap;

use fsm_core::decimal::Dec;
use fsm_core::expr::eval::Val;
use fsm_core::json::Value;
use fsm_execute::config::{HandlerTable, substitute};
use fsm_execute::error::ExecError;

fn detail(error: &ExecError, key: &str) -> String {
    let details = error.details.as_ref().expect("error carries details");
    match details.get(key) {
        Some(Value::Str(text)) => text.clone(),
        Some(Value::Num(number)) => number.clone(),
        other => panic!("details.{key} is {other:?} in {error:?}"),
    }
}

fn rejected(source: &str) -> ExecError {
    match HandlerTable::parse(source) {
        Err(error) => {
            assert_eq!(error.code, "exec/config", "{error:?}");
            assert!(error.hint.is_some(), "every exec/config error states a fix");
            error
        }
        Ok(table) => panic!("expected a rejection, parsed {table:?}"),
    }
}

/// A one-handler table around one argv element, for the placeholder rows.
fn table_with_argv_element(element: &str) -> String {
    format!(
        r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"request_confirmation","argv":["/usr/local/bin/notify-supplier","{element}"],"timeout_ms":1000}}]}}"#
    )
}

#[test]
fn a_minimal_table_round_trips_its_command_and_timeout() {
    let table = HandlerTable::parse(include_str!("fixtures/handlers/valid_min.json")).unwrap();
    assert_eq!(table.handlers.len(), 1);
    let handler = &table.handlers["request_confirmation"];
    assert_eq!(handler.effect, "request_confirmation");
    assert_eq!(
        handler.argv,
        [
            "/usr/local/bin/notify-supplier",
            "--order",
            "{order_id}",
            "--reviewer",
            "{reviewer}",
            "--quiet"
        ]
    );
    assert_eq!(handler.timeout_ms, 120_000);
    assert_eq!(handler.on_ok, None);
    assert_eq!(handler.on_failed, None);
}

#[test]
fn an_advance_block_round_trips_event_payload_and_stamps() {
    let table = HandlerTable::parse(include_str!("fixtures/handlers/valid_advance.json")).unwrap();
    let handler = &table.handlers["request_confirmation"];
    let on_ok = handler.on_ok.as_ref().expect("on_ok is declared");
    assert_eq!(on_ok.event, "confirmed");
    assert_eq!(
        on_ok.payload,
        Value::Obj(BTreeMap::from([(
            "channel".into(),
            Value::Str("supplier".into())
        )]))
    );
    assert_eq!(on_ok.stamps, ["at"]);
    let on_failed = handler.on_failed.as_ref().expect("on_failed is declared");
    assert_eq!(on_failed.event, "confirmation_failed");
    assert_eq!(on_failed.payload, Value::Obj(BTreeMap::new()));
    assert!(on_failed.stamps.is_empty());
}

#[test]
fn a_table_without_the_exact_format_tag_is_refused() {
    let error = rejected(r#"{"format":"fsm.handlers/2","handlers":[]}"#);
    assert_eq!(detail(&error, "field"), "format");
    let error = rejected(r#"{"handlers":[]}"#);
    assert_eq!(detail(&error, "field"), "format");
}

#[test]
fn handlers_must_be_a_non_empty_array() {
    let error = rejected(r#"{"format":"fsm.handlers/1","handlers":[]}"#);
    assert_eq!(detail(&error, "field"), "handlers");
    let error = rejected(r#"{"format":"fsm.handlers/1","handlers":{}}"#);
    assert_eq!(detail(&error, "field"), "handlers");
}

#[test]
fn two_handlers_for_one_effect_name_the_duplicate() {
    let error = rejected(include_str!("fixtures/handlers/dup_effect.json"));
    assert_eq!(detail(&error, "handler_index"), "1");
    assert_eq!(detail(&error, "field"), "effect");
    assert_eq!(detail(&error, "effect"), "match_invoice");
    assert!(error.message.contains("match_invoice"), "{error:?}");
}

#[test]
fn an_empty_argv_names_the_handler_that_has_nothing_to_run() {
    let error = rejected(include_str!("fixtures/handlers/empty_argv.json"));
    assert_eq!(detail(&error, "handler_index"), "0");
    assert_eq!(detail(&error, "field"), "argv");
}

#[test]
fn a_non_positive_or_missing_timeout_is_refused() {
    let error = rejected(include_str!("fixtures/handlers/bad_timeout.json"));
    assert_eq!(detail(&error, "handler_index"), "0");
    assert_eq!(detail(&error, "field"), "timeout_ms");

    // The fixture's second handler declares no timeout at all; validation stops
    // at the first fault, so the missing form is pinned on its own.
    let missing = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"escalate_approval","argv":["/bin/true"]}]}"#,
    );
    assert_eq!(detail(&missing, "field"), "timeout_ms");
    let negative = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"escalate_approval","argv":["/bin/true"],"timeout_ms":-1}]}"#,
    );
    assert_eq!(detail(&negative, "field"), "timeout_ms");
    let fractional = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"escalate_approval","argv":["/bin/true"],"timeout_ms":1.5}]}"#,
    );
    assert_eq!(detail(&fractional, "field"), "timeout_ms");
}

#[test]
fn an_advance_block_needs_an_event_and_an_object_payload() {
    let error = rejected(include_str!("fixtures/handlers/bad_advance.json"));
    assert_eq!(detail(&error, "handler_index"), "0");
    assert_eq!(detail(&error, "field"), "on_ok");

    // The fixture's second handler is the array-payload form.
    let array_payload = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"escalate_approval","argv":["/bin/true"],"timeout_ms":1000,"on_ok":{"event":"escalated","payload":["not","an","object"]}}]}"#,
    );
    assert_eq!(detail(&array_payload, "field"), "on_ok");
    let empty_stamp = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"escalate_approval","argv":["/bin/true"],"timeout_ms":1000,"on_failed":{"event":"escalated","stamps":[""]}}]}"#,
    );
    assert_eq!(detail(&empty_stamp, "field"), "on_failed");
}

#[test]
fn an_effect_name_must_be_a_non_empty_string() {
    let error = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"","argv":["/bin/true"],"timeout_ms":1000}]}"#,
    );
    assert_eq!(detail(&error, "field"), "effect");
}

#[test]
fn a_well_formed_placeholder_is_accepted() {
    let table = HandlerTable::parse(&table_with_argv_element("{ok_name_1}")).unwrap();
    assert_eq!(
        table.handlers["request_confirmation"].argv[1],
        "{ok_name_1}"
    );
    // A placeholder may sit inside a larger word, and a template may hold more
    // than one.
    let table =
        HandlerTable::parse(&table_with_argv_element("case-{case_id}/{reviewer}.json")).unwrap();
    assert_eq!(
        table.handlers["request_confirmation"].argv[1],
        "case-{case_id}/{reviewer}.json"
    );
}

#[test]
fn every_malformed_placeholder_is_rejected_with_its_offset() {
    for (element, offset) in [
        ("{Bad}", "0"),
        ("{with space}", "0"),
        ("{}", "0"),
        ("{unclosed", "0"),
        ("stray}", "5"),
        ("--order={order-id}", "8"),
    ] {
        let error = rejected(&table_with_argv_element(element));
        assert_eq!(detail(&error, "field"), "argv", "{element}");
        assert_eq!(detail(&error, "argv_index"), "1", "{element}");
        assert_eq!(detail(&error, "offset"), offset, "{element}");
    }
}

#[test]
fn the_committed_malformed_placeholder_fixture_names_its_handler_and_offset() {
    let error = rejected(include_str!("fixtures/handlers/bad_placeholder.json"));
    assert_eq!(detail(&error, "handler_index"), "0");
    assert_eq!(detail(&error, "field"), "argv");
    assert_eq!(detail(&error, "argv_index"), "2");
    assert_eq!(detail(&error, "offset"), "0");
}

#[test]
fn a_placeholder_may_never_choose_the_command() {
    // Effect args are expressions over context and event payload, so a
    // placeholder in argv[0] would let whoever sends an event pick the binary.
    let error = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"run_report","argv":["{tool}","--out","{path}"],"timeout_ms":1000}]}"#,
    );
    assert_eq!(detail(&error, "field"), "argv");
    assert_eq!(detail(&error, "argv_index"), "0");
    assert!(error.message.contains("argv[0]"), "{error:?}");

    let partly = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"run_report","argv":["/usr/local/bin/{tool}"],"timeout_ms":1000}]}"#,
    );
    assert_eq!(detail(&partly, "argv_index"), "0");
}

#[test]
fn an_unknown_key_is_refused_rather_than_silently_ignored() {
    // `on_okay` would validate and then never advance, which is
    // indistinguishable at run time from a deliberately undeclared advance.
    let handler_key = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"request_approval","argv":["/bin/true"],"timeout_ms":1000,"on_okay":{"event":"approved"}}]}"#,
    );
    assert_eq!(detail(&handler_key, "key"), "on_okay");
    assert_eq!(detail(&handler_key, "handler_index"), "0");

    let advance_key = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"request_approval","argv":["/bin/true"],"timeout_ms":1000,"on_ok":{"event":"approved","stamp":["at"]}}]}"#,
    );
    assert_eq!(detail(&advance_key, "key"), "stamp");
    assert_eq!(detail(&advance_key, "field"), "on_ok");

    let document_key =
        rejected(r#"{"format":"fsm.handlers/1","handlers":[],"poll_interval_ms":250}"#);
    assert_eq!(detail(&document_key, "key"), "poll_interval_ms");
}

#[test]
fn a_timeout_is_bounded_above_so_a_kill_deadline_cannot_overflow() {
    let error = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"request_approval","argv":["/bin/true"],"timeout_ms":9223372036854775807}]}"#,
    );
    assert_eq!(detail(&error, "field"), "timeout_ms");
    assert_eq!(detail(&error, "max_timeout_ms"), "86400000");

    let at_the_ceiling = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"request_approval","argv":["/bin/true"],"timeout_ms":86400000}]}"#,
    )
    .unwrap();
    assert_eq!(
        at_the_ceiling.handlers["request_approval"].timeout_ms,
        86_400_000
    );
}

#[test]
fn the_offset_of_a_fault_counts_characters_not_bytes() {
    let error = rejected(&table_with_argv_element("¥¥}"));
    assert_eq!(detail(&error, "offset"), "2");
}

#[test]
fn a_non_string_argv_element_names_its_index() {
    let error = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"archive_case","argv":["/bin/true",7],"timeout_ms":1000}]}"#,
    );
    assert_eq!(detail(&error, "field"), "argv");
    assert_eq!(detail(&error, "argv_index"), "1");
}

#[test]
fn substitution_renders_each_value_in_its_canonical_form() {
    let argv: Vec<String> = ["/usr/local/bin/notify-supplier", "--order", "{order_id}"]
        .iter()
        .map(|element| (*element).to_string())
        .collect();
    let args = BTreeMap::from([("order_id".to_string(), Val::Str("order-4711".into()))]);
    assert_eq!(
        substitute(&argv, &args).unwrap(),
        ["/usr/local/bin/notify-supplier", "--order", "order-4711"]
    );

    let argv = vec!["{quantity}".to_string(), "{amount}".to_string()];
    let args = BTreeMap::from([
        ("quantity".to_string(), Val::Int(42)),
        (
            "amount".to_string(),
            Val::Dec(Dec::parse("19.50", 2).unwrap()),
        ),
    ]);
    assert_eq!(substitute(&argv, &args).unwrap(), ["42", "19.50"]);
}

#[test]
fn a_placeholder_with_no_matching_effect_argument_names_it() {
    let argv = vec!["--order".to_string(), "{order_id}".to_string()];
    let args = BTreeMap::from([("reviewer".to_string(), Val::Str("dana".into()))]);
    let error = substitute(&argv, &args).unwrap_err();
    assert_eq!(error.code, "exec/config");
    assert!(error.message.contains("order_id"), "{error:?}");
    assert_eq!(detail(&error, "placeholder"), "order_id");
}

#[test]
fn a_substituted_value_is_never_re_split_or_expanded() {
    let argv = vec!["--note".to_string(), "{note}".to_string()];
    let args = BTreeMap::from([(
        "note".to_string(),
        Val::Str("two words; $(rm -rf /) `id` | tee *".into()),
    )]);
    let rendered = substitute(&argv, &args).unwrap();
    assert_eq!(rendered.len(), 2, "one template element, one argv element");
    assert_eq!(rendered[1], "two words; $(rm -rf /) `id` | tee *");
}

#[test]
fn substitution_leaves_the_literal_parts_of_a_template_alone() {
    let argv = vec!["case-{case_id}/{reviewer}.json".to_string()];
    let args = BTreeMap::from([
        ("case_id".to_string(), Val::Str("c-9".into())),
        ("reviewer".to_string(), Val::Str("dana".into())),
    ]);
    assert_eq!(substitute(&argv, &args).unwrap(), ["case-c-9/dana.json"]);
}

#[test]
fn a_table_that_is_not_json_is_refused_with_the_byte_that_broke_it() {
    let error = rejected("not json at all");
    assert!(error.message.contains("JSON"), "{error:?}");
    // The parser's own sentence and offset, not a Debug dump: the offset is
    // the only thing that locates the fault in an operator's file.
    assert!(!error.message.contains("JsonError"), "{error:?}");
    let source = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"a","argv":["/bin/true"],"timeout_ms":1,}]}"#;
    let trailing = rejected(source);
    let offset: usize = detail(&trailing, "offset").parse().unwrap();
    assert_eq!(
        source.as_bytes()[offset] as char,
        '}',
        "the offset points at the brace the trailing comma orphaned"
    );
}
