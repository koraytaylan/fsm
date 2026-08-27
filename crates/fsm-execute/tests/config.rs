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
fn a_rooted_posix_command_is_accepted_on_every_platform() {
    // `is_absolute` would demand a drive prefix on Windows and refuse the same
    // table there; a rooted path is what keeps PATH out of the decision.
    let table = HandlerTable::parse(include_str!("fixtures/handlers/valid_min.json")).unwrap();
    assert_eq!(
        table.handlers["request_confirmation"].argv[0],
        "/usr/local/bin/notify-supplier"
    );
}

#[test]
fn a_bare_command_name_is_refused_because_path_would_choose_the_binary() {
    let error = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"run_report","argv":["notify-supplier","--quiet"],"timeout_ms":1000}]}"#,
    );
    assert_eq!(detail(&error, "field"), "argv");
    assert_eq!(detail(&error, "argv_index"), "0");
    assert!(error.message.contains("PATH"), "{error:?}");

    let relative = rejected(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"run_report","argv":["./notify-supplier"],"timeout_ms":1000}]}"#,
    );
    assert_eq!(detail(&relative, "argv_index"), "0");
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

// ---------------------------------------------------------------------------
// Retry policy. Plan 0016 task 7402.
// ---------------------------------------------------------------------------

/// A table with one handler carrying whatever retry block is given.
fn with_retry(retry: &str) -> Result<HandlerTable, ExecError> {
    let block = if retry.is_empty() {
        String::new()
    } else {
        format!(r#","retry":{retry}"#)
    };
    HandlerTable::parse(&format!(
        r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000{block}}}]}}"#
    ))
}

fn handler(table: &HandlerTable) -> &fsm_execute::config::HandlerSpec {
    table.handlers.get("notify").expect("the handler")
}

#[test]
fn a_table_without_retry_means_exactly_todays_behaviour() {
    let table = with_retry("").expect("today's table still parses");
    let retry = &handler(&table).retry;
    assert_eq!(retry.attempts, 1, "one attempt, as before");
    assert!(
        !retry.retries("timeout"),
        "and nothing is retried, whatever the class"
    );
    assert_eq!(retry.backoff_ms, fsm_execute::config::DEFAULT_BACKOFF_MS);
    assert_eq!(
        retry.max_backoff_ms,
        fsm_execute::config::DEFAULT_MAX_BACKOFF_MS
    );
}

#[test]
fn a_full_block_parses_and_a_partial_one_takes_the_documented_defaults() {
    let table = with_retry(
        r#"{"attempts":4,"backoff_ms":250,"max_backoff_ms":5000,"on":["timeout","spawn"]}"#,
    )
    .expect("a full block");
    let retry = &handler(&table).retry;
    assert_eq!(retry.attempts, 4);
    assert_eq!(retry.backoff_ms, 250);
    assert_eq!(retry.max_backoff_ms, 5_000);
    assert_eq!(retry.on, ["timeout", "spawn"]);
    assert!(retry.retries("timeout"));
    assert!(
        !retry.retries("nonzero_exit"),
        "a class the table did not name is not retried"
    );

    let table = with_retry(r#"{"attempts":3}"#).expect("attempts alone");
    let retry = &handler(&table).retry;
    assert_eq!(retry.attempts, 3);
    assert_eq!(retry.backoff_ms, fsm_execute::config::DEFAULT_BACKOFF_MS);
    assert_eq!(
        retry.max_backoff_ms,
        fsm_execute::config::DEFAULT_MAX_BACKOFF_MS
    );
    // An absent `on` means every class *this kind can produce*. `with_retry`
    // builds a process handler, and a process handler cannot fail with
    // `mcp_error`, so listing it would be a class that retries nothing.
    assert!(retry.retries("nonzero_exit") && retry.retries("timeout") && retry.retries("spawn"));
    assert!(!retry.retries("mcp_error"), "not a process failure");
}

#[test]
fn the_attempt_bounds_are_one_and_sixteen() {
    for attempts in [1, fsm_execute::config::MAX_ATTEMPTS] {
        assert!(
            with_retry(&format!(r#"{{"attempts":{attempts}}}"#)).is_ok(),
            "{attempts} is inside the bounds"
        );
    }
    for attempts in [0, fsm_execute::config::MAX_ATTEMPTS + 1] {
        let error =
            with_retry(&format!(r#"{{"attempts":{attempts}}}"#)).expect_err("outside the bounds");
        assert_eq!(error.code, "exec/config", "{attempts}");
        assert_eq!(detail(&error, "handler_index"), "0");
    }
}

#[test]
fn a_ceiling_below_the_floor_is_refused() {
    let error = with_retry(r#"{"attempts":3,"backoff_ms":5000,"max_backoff_ms":1000}"#)
        .expect_err("a ceiling below the floor");
    assert_eq!(error.code, "exec/config");
    assert!(
        error.message.contains("max_backoff_ms"),
        "{}",
        error.message
    );
    for field in ["backoff_ms", "max_backoff_ms"] {
        assert!(
            error
                .details
                .as_ref()
                .is_some_and(|d| d.get(field).is_some()),
            "both numbers belong in the details: {error:?}"
        );
    }
    // Zero and negative waits are refused too.
    assert!(with_retry(r#"{"backoff_ms":0}"#).is_err());
    assert!(with_retry(r#"{"max_backoff_ms":-1}"#).is_err());
}

#[test]
fn an_unknown_failure_class_is_refused_with_the_valid_list() {
    let error = with_retry(r#"{"attempts":2,"on":["flakey"]}"#).expect_err("no such class");
    assert_eq!(error.code, "exec/config");
    assert!(error.message.contains("flakey"), "{}", error.message);
    // The list is the classes valid *for this handler's kind*: telling a
    // process handler's author that `mcp_error` was available would send them
    // to write a line that retries nothing.
    for class in fsm_execute::config::classes_for(&fsm_execute::config::HandlerKind::Process) {
        assert!(
            error.message.contains(class),
            "the valid list must be in the message: {}",
            error.message
        );
    }
}

#[test]
fn cancelled_is_refused_by_name_and_the_reason_is_pinned() {
    // The rule most likely to be requested as a feature later, so its words
    // are pinned: a handler killed because its instance was cancelled must
    // never be restarted.
    let error = with_retry(r#"{"attempts":3,"on":["timeout","cancelled"]}"#)
        .expect_err("cancelled is not retryable");
    assert_eq!(error.code, "exec/config");
    assert!(
        error
            .message
            .contains("cancelled is not a retryable failure class"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("must never be restarted"),
        "the reason travels with the refusal: {}",
        error.message
    );
}

#[test]
fn a_misspelled_key_is_refused_by_the_closed_set() {
    // The same reasoning that refuses `on_okay` today.
    let misspelled = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000,"retries":{"attempts":3}}]}"#,
    )
    .expect_err("retries is not retry");
    assert_eq!(misspelled.code, "exec/config");

    let inside = with_retry(r#"{"attemps":3}"#).expect_err("a typo inside the block");
    assert_eq!(inside.code, "exec/config");
    assert_eq!(detail(&inside, "handler_index"), "0");
}

#[test]
fn every_committed_example_table_still_means_one_attempt() {
    // No committed table changes meaning when this lands.
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut checked = 0;
    for entry in std::fs::read_dir(root.join("examples"))
        .expect("examples")
        .flatten()
    {
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".handlers.json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable");
        let table = HandlerTable::parse(&text)
            .unwrap_or_else(|error| panic!("{} no longer validates: {error:?}", path.display()));
        for (effect, spec) in &table.handlers {
            assert_eq!(
                spec.retry.attempts,
                1,
                "{}'s {effect} changed meaning",
                path.display()
            );
            assert_eq!(
                spec.kind,
                fsm_execute::config::HandlerKind::Process,
                "{}'s {effect} is still a process handler",
                path.display()
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no example tables were checked");
}

#[test]
fn the_plans_codes_are_registered_and_well_formed() {
    let codes = fsm_execute::error::ALL_CODES;
    for code in [
        "exec/retries_exhausted",
        "exec/mcp_protocol",
        "exec/mcp_tool",
        "exec/inflight_deferred",
    ] {
        assert!(codes.contains(&code), "{code} is not registered");
    }
    let mut seen = std::collections::BTreeSet::new();
    for code in codes {
        assert!(code.starts_with("exec/"), "{code}");
        assert!(code.len() > "exec/".len(), "{code}");
        assert!(seen.insert(*code), "{code} is listed twice");
    }
}
