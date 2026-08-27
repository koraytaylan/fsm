//! The second handler kind, and the sentence that keeps the boundary closed.
//!
//! A handler table is the executor's whole security boundary: it closes the
//! set of commands the executor can ever run. Adding a kind that talks a
//! protocol instead of reading an exit code must not widen that boundary by
//! one inch, so every row below is either about the rules staying **identical**
//! across the two kinds, or about the new keys belonging to exactly one of
//! them.

use std::collections::BTreeMap;

use fsm_core::decimal::Dec;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::config::{HandlerKind, HandlerTable, classes_for, substitute_arguments};
use fsm_execute::error::ExecError;

/// A one-handler table with the given keys spliced in after `argv`.
fn table(extra: &str) -> Result<HandlerTable, ExecError> {
    HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",
            "handlers":[{{
                "effect":"summarize_case",
                "argv":["/usr/local/bin/case-tools","--stdio"],
                "timeout_ms":60000{extra}
            }}]
        }}"#
    ))
}

fn rejected(extra: &str) -> ExecError {
    match table(extra) {
        Err(error) => {
            assert_eq!(error.code, "exec/config", "{error:?}");
            assert!(error.hint.is_some(), "every exec/config error states a fix");
            error
        }
        Ok(parsed) => panic!("expected a rejection, parsed {parsed:?}"),
    }
}

fn only_handler(table: &HandlerTable) -> &fsm_execute::config::HandlerSpec {
    table
        .handlers
        .values()
        .next()
        .expect("the table declares one handler")
}

fn detail(error: &ExecError, key: &str) -> String {
    let details = error.details.as_ref().expect("error carries details");
    match details.get(key) {
        Some(Value::Str(text)) => text.clone(),
        Some(Value::Num(number)) => number.clone(),
        other => panic!("details.{key} is {other:?} in {error:?}"),
    }
}

fn json(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("the fixture is valid JSON")
}

#[test]
fn a_full_mcp_handler_parses_with_its_tool_and_arguments() {
    let parsed = table(
        r#","kind":"mcp","tool":"summarize",
           "arguments":{"case_id":"{case_id}","mode":"brief","depth":2},
           "retry":{"attempts":3,"backoff_ms":2000,"on":["mcp_error","timeout"]},
           "on_ok":{"event":"summarized"},
           "on_failed":{"event":"summary_failed"}"#,
    )
    .expect("a full mcp handler validates");
    let handler = only_handler(&parsed);
    let HandlerKind::Mcp { tool, arguments } = &handler.kind else {
        panic!("expected the mcp kind, got {:?}", handler.kind);
    };
    assert_eq!(tool, "summarize");
    assert_eq!(
        arguments,
        &json(r#"{"case_id":"{case_id}","mode":"brief","depth":2}"#)
    );
    assert!(handler.kind.is_mcp());
    assert_eq!(handler.kind.as_str(), "mcp");
    // Everything else means exactly what it means for a process handler, so
    // an operator learns one model and applies it twice.
    assert_eq!(handler.timeout_ms, 60_000);
    assert_eq!(handler.retry.attempts, 3);
    assert_eq!(handler.retry.backoff_ms, 2_000);
    assert_eq!(
        handler.on_ok.as_ref().map(|a| a.event.as_str()),
        Some("summarized")
    );
    assert_eq!(
        handler.on_failed.as_ref().map(|a| a.event.as_str()),
        Some("summary_failed")
    );
}

#[test]
fn a_handler_with_no_kind_is_a_process_handler_exactly_as_before() {
    let parsed = table("").expect("a table with no kind validates");
    let handler = only_handler(&parsed);
    assert_eq!(handler.kind, HandlerKind::Process);
    assert_eq!(handler.kind.as_str(), "process");
    assert!(!handler.kind.is_mcp());
    // And saying it out loud means the same thing.
    let spelled = table(r#","kind":"process""#).expect("an explicit process kind validates");
    assert_eq!(only_handler(&spelled).kind, HandlerKind::Process);
}

#[test]
fn an_mcp_handler_must_name_the_tool_it_calls() {
    let missing = rejected(r#","kind":"mcp""#);
    assert!(missing.message.contains("tool"), "{}", missing.message);
    assert_eq!(detail(&missing, "field"), "tool");

    let empty = rejected(r#","kind":"mcp","tool":"""#);
    assert!(empty.message.contains("tool"), "{}", empty.message);

    let wrong_type = rejected(r#","kind":"mcp","tool":7"#);
    assert!(
        wrong_type.message.contains("tool"),
        "{}",
        wrong_type.message
    );
}

#[test]
fn arguments_alone_defaults_to_an_empty_object() {
    let parsed = table(r#","kind":"mcp","tool":"summarize""#).expect("arguments is optional");
    let HandlerKind::Mcp { arguments, .. } = &only_handler(&parsed).kind else {
        panic!("expected the mcp kind");
    };
    assert_eq!(arguments, &Value::Obj(BTreeMap::new()));
}

#[test]
fn a_key_that_would_do_nothing_is_refused_rather_than_ignored() {
    // A `tool` on a process handler is a key somebody will expect to work.
    let tool = rejected(r#","tool":"summarize""#);
    assert!(tool.message.contains("tool"), "{}", tool.message);
    assert!(tool.message.contains("mcp"), "{}", tool.message);
    let arguments = rejected(r#","arguments":{"case_id":"{case_id}"}"#);
    assert!(
        arguments.message.contains("arguments"),
        "{}",
        arguments.message
    );
    // Explicitly, too — not only by omission of `kind`.
    let explicit = rejected(r#","kind":"process","tool":"summarize""#);
    assert!(explicit.message.contains("tool"), "{}", explicit.message);
}

#[test]
fn an_unknown_kind_names_the_two_that_exist() {
    let error = rejected(r#","kind":"grpc""#);
    assert!(error.message.contains("grpc"), "{}", error.message);
    for kind in ["process", "mcp"] {
        assert!(error.message.contains(kind), "{}", error.message);
    }
    // And a kind that is not even a string.
    let wrong_type = rejected(r#","kind":true"#);
    assert!(
        wrong_type.message.contains("process"),
        "{}",
        wrong_type.message
    );
}

#[test]
fn the_argv_rules_are_identical_for_both_kinds() {
    // This is the security argument, and it depends on the rule being uniform
    // rather than on a second rule that happens to agree today.
    for kind in ["", r#","kind":"mcp","tool":"summarize""#] {
        let placeholder = HandlerTable::parse(&format!(
            r#"{{"format":"fsm.handlers/1","handlers":[{{
                "effect":"summarize_case",
                "argv":["{{binary}}","--stdio"],
                "timeout_ms":1000{kind}
            }}]}}"#
        ))
        .expect_err("argv[0] may never be a placeholder");
        assert_eq!(placeholder.code, "exec/config");
        assert!(
            placeholder.message.contains("literal path"),
            "{}",
            placeholder.message
        );

        let bare = HandlerTable::parse(&format!(
            r#"{{"format":"fsm.handlers/1","handlers":[{{
                "effect":"summarize_case",
                "argv":["case-tools","--stdio"],
                "timeout_ms":1000{kind}
            }}]}}"#
        ))
        .expect_err("argv[0] may never be resolved through PATH");
        assert_eq!(bare.code, "exec/config");
        assert!(bare.message.contains("PATH"), "{}", bare.message);

        let empty = HandlerTable::parse(&format!(
            r#"{{"format":"fsm.handlers/1","handlers":[{{
                "effect":"summarize_case",
                "argv":[],
                "timeout_ms":1000{kind}
            }}]}}"#
        ))
        .expect_err("an empty argv has nothing to run");
        assert_eq!(empty.code, "exec/config");
    }
}

#[test]
fn a_malformed_placeholder_in_arguments_names_its_json_path() {
    let nested = rejected(
        r#","kind":"mcp","tool":"summarize","arguments":{"filter":{"by":{"owner":"{Case}"}}}"#,
    );
    assert_eq!(detail(&nested, "path"), "arguments.filter.by.owner");
    assert!(
        nested.message.contains("arguments.filter.by.owner"),
        "{}",
        nested.message
    );

    let in_array =
        rejected(r#","kind":"mcp","tool":"summarize","arguments":{"tags":["ok","{unclosed"]}"#);
    assert_eq!(detail(&in_array, "path"), "arguments.tags[1]");

    // A stray close brace is a fault too, and the same scan finds it.
    let stray = rejected(r#","kind":"mcp","tool":"summarize","arguments":{"note":"a } b"}"#);
    assert_eq!(detail(&stray, "path"), "arguments.note");
    assert!(
        stray
            .details
            .as_ref()
            .is_some_and(|d| d.get("offset").is_some())
    );
}

#[test]
fn arguments_must_be_an_object() {
    let error = rejected(r#","kind":"mcp","tool":"summarize","arguments":["case"]"#);
    assert!(error.message.contains("object"), "{}", error.message);
}

#[test]
fn only_string_values_are_templated() {
    let parsed = table(
        r#","kind":"mcp","tool":"summarize","arguments":{
            "case_id":"{case_id}",
            "{case_id}":"a key is a key",
            "depth":2,
            "verbose":true,
            "nothing":null,
            "nested":{"note":"case {case_id} at depth {depth}"},
            "tags":["{case_id}","literal"]
        }"#,
    )
    .expect("mixed argument types validate");
    let HandlerKind::Mcp { arguments, .. } = &only_handler(&parsed).kind else {
        panic!("expected the mcp kind");
    };
    let args = BTreeMap::from([
        ("case_id".to_string(), Val::Str("case-91".into())),
        ("depth".to_string(), Val::Int(3)),
    ]);
    let filled = substitute_arguments(arguments, &args).expect("every placeholder resolves");

    assert_eq!(
        filled.get("case_id").and_then(Value::as_str),
        Some("case-91")
    );
    // An object *key* is copied verbatim. A tool's input schema names its
    // properties, and letting an effect argument choose one would let
    // machine-emitted data reshape the call.
    assert_eq!(
        filled.get("{case_id}").and_then(Value::as_str),
        Some("a key is a key")
    );
    assert_eq!(filled.get("depth").and_then(Value::as_num), Some("2"));
    assert_eq!(filled.get("verbose"), Some(&Value::Bool(true)));
    assert_eq!(filled.get("nothing"), Some(&Value::Null));
    assert_eq!(
        filled
            .get("nested")
            .and_then(|nested| nested.get("note"))
            .and_then(Value::as_str),
        Some("case case-91 at depth 3")
    );
    let tags = filled
        .get("tags")
        .and_then(Value::as_arr)
        .expect("an array");
    assert_eq!(tags[0].as_str(), Some("case-91"));
    assert_eq!(tags[1].as_str(), Some("literal"));
}

#[test]
fn a_substituted_value_keeps_the_canonical_rendering_and_stays_a_string() {
    let template = json(r#"{"amount":"{quantity}","when":"{at}","exact":"{ratio}"}"#);
    let args = BTreeMap::from([
        ("quantity".to_string(), Val::Int(7)),
        ("at".to_string(), Val::Ts(1_700_000_000_000)),
        (
            "ratio".to_string(),
            Val::Dec(Dec::parse("1.500", 3).expect("a decimal with scale")),
        ),
    ]);
    let filled = substitute_arguments(&template, &args).expect("every placeholder resolves");
    // A placeholder filling a whole string still produces a string. Re-typing
    // a value from what it renders as would make the template's meaning
    // depend on the data flowing through it.
    assert_eq!(filled.get("amount"), Some(&Value::Str("7".into())));
    assert_eq!(filled.get("exact"), Some(&Value::Str("1.500".into())));
    assert!(matches!(filled.get("when"), Some(Value::Str(_))));
}

#[test]
fn a_placeholder_naming_an_absent_argument_fails_at_run_time() {
    let template = json(r#"{"case_id":"{case_id}"}"#);
    let error = substitute_arguments(&template, &BTreeMap::new())
        .expect_err("this effect emitted no such argument");
    assert_eq!(error.code, "exec/config");
    assert_eq!(detail(&error, "placeholder"), "case_id");
}

#[test]
fn mcp_error_is_a_class_for_the_mcp_kind_only() {
    let accepted =
        table(r#","kind":"mcp","tool":"summarize","retry":{"attempts":2,"on":["mcp_error"]}"#)
            .expect("mcp_error belongs to an mcp handler");
    assert!(only_handler(&accepted).retry.retries("mcp_error"));

    let refused = rejected(r#","retry":{"attempts":2,"on":["mcp_error"]}"#);
    assert!(refused.message.contains("mcp_error"), "{}", refused.message);
    assert!(
        refused.message.contains("mcp"),
        "the hint must say which kind it applies to: {}",
        refused.message
    );
    assert_eq!(detail(&refused, "field"), "retry.on");
}

#[test]
fn the_classes_of_each_kind_are_the_ones_it_can_produce() {
    assert_eq!(
        classes_for(&HandlerKind::Process),
        ["nonzero_exit", "timeout", "spawn"]
    );
    assert_eq!(
        classes_for(&HandlerKind::Mcp {
            tool: "summarize".into(),
            arguments: Value::Obj(BTreeMap::new()),
        }),
        ["nonzero_exit", "timeout", "spawn", "mcp_error"]
    );
    // An absent `retry.on` means every class the kind can produce, so a
    // process handler never carries a class that retries nothing.
    let process = table(r#","retry":{"attempts":2}"#).expect("a process retry block");
    assert!(!only_handler(&process).retry.retries("mcp_error"));
    let mcp = table(r#","kind":"mcp","tool":"summarize","retry":{"attempts":2}"#)
        .expect("an mcp retry block");
    assert!(only_handler(&mcp).retry.retries("mcp_error"));
}

#[test]
fn cancelled_is_refused_on_both_kinds() {
    for kind in ["", r#","kind":"mcp","tool":"summarize""#] {
        let error = rejected(&format!(
            r#"{kind},"retry":{{"attempts":2,"on":["cancelled"]}}"#
        ));
        assert!(
            error.message.contains("must never be restarted"),
            "{}",
            error.message
        );
    }
}
