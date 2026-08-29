//! The `fsm.cases/1` parser: what it accepts, and every way it refuses.
//!
//! Plan 0018 task 8401. The point of this format is that a mistyped key is an
//! error rather than a silently unasserted expectation, so most of this file
//! is refusals — and each one checks that the message *names the offending
//! key*, because a refusal an author cannot act on is barely better than the
//! silence it replaced.

use std::collections::BTreeMap;

use fsm_core::cases::format::{
    ACK_KEYS, AckOutcome, CASE_KEYS, CASES_FORMAT, DOCUMENT_KEYS, EXPECT_KEYS, Expect, SEND_KEYS,
    Step, parse_cases,
};
use fsm_core::limits::{MAX_CASE_BYTES, MAX_CASES_PER_FILE, MAX_SCRIPT_STEPS};

const GOLDEN: &str = include_str!("fixtures/cases_v1.json");

/// Each finding as `(code, path, message, hint)`.
///
/// The hint is part of the refusal, not decoration: this workspace's `Finding`
/// puts "what to write instead" there, so a test that only reads the message is
/// testing half of what an author sees.
fn refusal(source: &str) -> Vec<(String, String, String, String)> {
    parse_cases(source.as_bytes())
        .err()
        .unwrap_or_else(|| panic!("this document was accepted: {source}"))
        .into_iter()
        .map(|finding| {
            (
                finding.code.to_string(),
                finding.path,
                finding.message,
                finding.hint,
            )
        })
        .collect()
}

fn codes(source: &str) -> Vec<String> {
    refusal(source)
        .into_iter()
        .map(|(code, _, _, _)| code)
        .collect()
}

/// Every refusal, joined, so a test can ask whether something was named
/// anywhere an author would look.
fn rendered(source: &str) -> String {
    refusal(source)
        .into_iter()
        .map(|(code, path, message, hint)| format!("{code} {path} {message} — {hint}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A document with `body` spliced in as its case list.
fn document(cases: &str) -> String {
    format!("{{\"format\":\"{CASES_FORMAT}\",\"machine\":\"m\",\"cases\":{cases}}}")
}

/// One case with `script` and `expect` spliced in.
fn one_case(script: &str, expect: &str) -> String {
    document(&format!(
        "[{{\"name\":\"c\",\"script\":{script},\"expect\":{expect}}}]"
    ))
}

#[test]
fn the_committed_golden_parses_to_the_expected_structure() {
    let file = parse_cases(GOLDEN.as_bytes()).expect("the golden parses");
    assert_eq!(file.machine, "case_review");
    assert_eq!(file.cases.len(), 4);

    let first = &file.cases[0];
    assert_eq!(first.name, "a_scored_review_above_the_bar_is_approved");
    assert_eq!(
        first.context,
        BTreeMap::from([("score".into(), "0".into())])
    );
    assert_eq!(first.script.len(), 4);
    assert_eq!(
        first.script[0],
        Step::Send {
            event: "docs_ok".into(),
            payload: fsm_core::json::Value::Obj(BTreeMap::new()),
        }
    );
    assert!(matches!(
        &first.script[1],
        Step::Ack {
            effect,
            outcome: AckOutcome::Ok,
            result: Some(_),
        } if effect == "notify"
    ));
    assert_eq!(
        first.expect.asserted(),
        ["configuration", "context", "enabled", "effects", "terminal"]
    );

    // The third case is the one that exercises `poll` and a failed ack.
    let third = &file.cases[2];
    assert_eq!(third.script[1], Step::Poll { now_ms: 60_000 });
    assert!(matches!(
        &third.script[2],
        Step::Ack {
            outcome: AckOutcome::Failed,
            result: None,
            ..
        }
    ));
}

#[test]
fn an_expect_naming_one_field_asserts_only_that_field() {
    // The property the format exists to keep: absence is "not asserted", never
    // "expect empty". A reader will assume the opposite, so it is asserted on
    // the parsed structure rather than left to the prose.
    let file = parse_cases(GOLDEN.as_bytes()).expect("the golden parses");
    let partial = &file.cases[1];
    assert_eq!(partial.expect.asserted(), ["configuration"]);
    assert!(partial.expect.context.is_none());
    assert!(partial.expect.effects.is_none());
    assert!(partial.expect.terminal.is_none());

    // And a case with no `expect` at all asserts nothing, which is legal: its
    // script still has to run.
    let none = &file.cases[3];
    assert!(none.expect.is_empty());
    assert_eq!(none.expect.asserted(), Vec::<&str>::new());
    assert_eq!(Expect::default(), none.expect);
}

#[test]
fn an_unknown_key_is_refused_at_every_level_and_named() {
    // Four levels, one case each. A typo'd `expects` that parses to an empty
    // expectation is the exact failure this format exists to prevent, so each
    // refusal must name the key the author actually wrote.
    let cases = [
        (
            "document",
            document("[]").replace("\"machine\":\"m\"", "\"machine\":\"m\",\"machines\":\"m\""),
            "machines",
        ),
        (
            "case",
            document("[{\"name\":\"c\",\"script\":[],\"expects\":{}}]"),
            "expects",
        ),
        (
            "script step",
            one_case("[{\"send\":\"e\",\"payloud\":{}}]", "{}"),
            "payloud",
        ),
        (
            "expect",
            one_case("[]", "{\"configuraton\":[]}"),
            "configuraton",
        ),
    ];
    for (level, source, key) in cases {
        let rendered = rendered(&source);
        assert!(
            rendered.contains("case/unknown_key"),
            "an unknown key at the {level} level was not refused: {rendered}"
        );
        assert!(
            rendered.contains(key),
            "the {level}-level refusal does not name {key}: {rendered}"
        );
    }
}

#[test]
fn an_unknown_key_refusal_lists_what_is_accepted() {
    // A refusal that names the key but not the alternatives sends the author
    // to the source. Each level's accepted set comes from the parser's own
    // constants, so a new key cannot ship with a stale message.
    let rendered = rendered(&one_case("[]", "{\"configuraton\":[]}"));
    for key in EXPECT_KEYS {
        assert!(rendered.contains(key), "{rendered}");
    }
    assert!(!DOCUMENT_KEYS.is_empty() && !CASE_KEYS.is_empty() && !SEND_KEYS.is_empty());
}

#[test]
fn a_script_step_names_exactly_one_of_send_poll_ack() {
    let none = rendered(&one_case("[{\"payload\":{}}]", "{}"));
    assert!(
        none.contains("none of send, poll, ack"),
        "a step with no discriminator was not refused clearly: {none}"
    );

    let both = rendered(&one_case("[{\"send\":\"e\",\"poll\":1}]", "{}"));
    assert!(
        both.contains("send") && both.contains("poll"),
        "a step with two discriminators does not name both: {both}"
    );
}

#[test]
fn a_poll_without_a_time_is_refused_because_there_is_no_clock_to_ask() {
    let missing = rendered(&one_case("[{\"poll\":true}]", "{}"));
    assert!(
        missing.contains("poll") && missing.contains("now_ms"),
        "{missing}"
    );
    // A non-integer time is refused too: a fractional millisecond is not a
    // time this engine can be polled at.
    let fractional = rendered(&one_case("[{\"poll\":1.5}]", "{}"));
    assert!(fractional.contains("poll"), "{fractional}");
}

#[test]
fn an_ack_needs_an_outcome_and_the_outcome_must_be_one_of_two() {
    let missing = rendered(&one_case("[{\"ack\":\"notify\"}]", "{}"));
    assert!(missing.contains("outcome"), "{missing}");

    let unknown = rendered(&one_case(
        "[{\"ack\":\"notify\",\"outcome\":\"maybe\"}]",
        "{}",
    ));
    assert!(
        unknown.contains("maybe") && unknown.contains("ok") && unknown.contains("failed"),
        "the refusal does not name what was written or what is accepted: {unknown}"
    );
    assert!(ACK_KEYS.contains(&"result"));
}

#[test]
fn a_format_other_than_the_current_one_is_refused_and_names_what_was_found() {
    let wrong = rendered("{\"format\":\"fsm.cases/2\",\"machine\":\"m\",\"cases\":[]}");
    assert!(
        wrong.contains("fsm.cases/2") && wrong.contains(CASES_FORMAT),
        "{wrong}"
    );
    let missing = rendered("{\"machine\":\"m\",\"cases\":[]}");
    assert!(missing.contains("format"), "{missing}");
}

/// `n` cases, each with its own name, since duplicates are refused.
fn many_cases(n: usize) -> String {
    document(&format!(
        "[{}]",
        (0..n)
            .map(|index| format!("{{\"name\":\"c{index}\",\"script\":[]}}"))
            .collect::<Vec<_>>()
            .join(",")
    ))
}

#[test]
fn the_cases_ceiling_admits_the_limit_and_refuses_one_more() {
    assert!(
        parse_cases(many_cases(MAX_CASES_PER_FILE).as_bytes()).is_ok(),
        "exactly {MAX_CASES_PER_FILE} cases was refused"
    );
    assert!(codes(&many_cases(MAX_CASES_PER_FILE + 1)).contains(&"case/limit_cases".to_string()));
}

#[test]
fn a_file_with_no_cases_is_refused() {
    // It parsed, ran, reported "0 passed, 0 failed" and exited zero — a
    // permanently green check asserting nothing, which is the failure this
    // format's own module doc calls strictly worse than having no case file.
    let rendered = rendered(&document("[]"));
    assert!(rendered.contains("case/shape"), "{rendered}");
    assert!(rendered.contains("asserts nothing"), "{rendered}");
}

#[test]
fn two_cases_with_one_name_are_refused() {
    // A name is how a reader, `--case`, and every report address a case. A
    // duplicate makes all three ambiguous and each resolves it differently:
    // regeneration wrote both expectations into the first case's block and
    // left the second untouched.
    let rendered = rendered(&document(
        "[{\"name\":\"same\",\"script\":[]},{\"name\":\"same\",\"script\":[]}]",
    ));
    assert!(rendered.contains("case/shape"), "{rendered}");
    assert!(rendered.contains("same"), "{rendered}");
    assert!(rendered.contains("own name"), "{rendered}");
}

#[test]
fn the_script_ceiling_admits_the_limit_and_refuses_one_more() {
    let step = "{\"send\":\"e\"}";
    let exact = one_case(
        &format!("[{}]", vec![step; MAX_SCRIPT_STEPS].join(",")),
        "{}",
    );
    assert!(
        parse_cases(exact.as_bytes()).is_ok(),
        "exactly {MAX_SCRIPT_STEPS} steps was refused"
    );
    let over = one_case(
        &format!("[{}]", vec![step; MAX_SCRIPT_STEPS + 1].join(",")),
        "{}",
    );
    assert!(codes(&over).contains(&"case/limit_steps".to_string()));
}

#[test]
fn the_byte_ceiling_admits_the_limit_and_refuses_one_more_without_parsing_it() {
    // The oversized document is deliberately *not* valid JSON past its first
    // bytes: if it were refused for its syntax the test would pass while the
    // ceiling did nothing, so the only way this can pass is by refusing on
    // length before the parser is reached.
    let over = "\u{7b}".repeat(MAX_CASE_BYTES + 1);
    assert_eq!(over.len(), MAX_CASE_BYTES + 1);
    let refusal = refusal(&over);
    assert_eq!(refusal.len(), 1, "the parser walked an oversized document");
    assert_eq!(refusal[0].0, "case/limit_bytes");

    // And a document of exactly the ceiling is admitted on length: it fails,
    // if at all, on its contents. Padded through `machine`, which the parser
    // carries for reporting only, around one real case — an empty file is
    // refused on its own account.
    let one = many_cases(1);
    let padding = MAX_CASE_BYTES - one.len();
    let exact = one.replace(
        "\"machine\":\"m\"",
        &format!("\"machine\":\"{}\"", "m".repeat(padding + 1)),
    );
    assert_eq!(exact.len(), MAX_CASE_BYTES);
    assert!(
        parse_cases(exact.as_bytes()).is_ok(),
        "a document of exactly the ceiling was refused"
    );
}

#[test]
fn hostile_input_is_refused_by_the_shared_json_limits_rather_than_by_recursion() {
    // No second parser and no second limit mechanism: depth is `JsonLimits`'
    // job, and a case file that nests past it must be refused rather than
    // recursed into.
    let deep = format!(
        "{{\"format\":\"{CASES_FORMAT}\",\"machine\":\"m\",\"cases\":[{{\"name\":\"c\",\
         \"script\":[{{\"send\":\"e\",\"payload\":{}{}}}]}}]}}",
        "{\"a\":".repeat(200),
        "\"x\"}".repeat(200),
    );
    let codes = codes(&deep);
    assert!(
        codes.contains(&"case/shape".to_string()),
        "a deeply nested payload was accepted: {codes:?}"
    );
}

#[test]
fn every_finding_carries_a_path_an_author_can_follow() {
    // A refusal without a location is a refusal an author has to search for.
    for (code, path, _, _) in refusal(&one_case("[{\"send\":\"e\",\"payloud\":{}}]", "{}")) {
        assert!(
            path.starts_with('/'),
            "{code} has no JSON-Pointer path: {path:?}"
        );
    }
}
