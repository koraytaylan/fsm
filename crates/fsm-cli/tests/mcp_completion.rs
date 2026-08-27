//! Completing identifiers this server already holds.
//!
//! This suite owns the rules that must hold whoever supplies the candidates:
//! the response shape, the truncation contract, case-sensitive prefixes,
//! supplier order, and the two error rulings.
//!
//! Plan 0013 task 6301.

use std::io::Cursor;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::complete::{MAX_VALUES, Ref, complete, completion_from};
use fsm_cli::mcp::notify::{Notifier, SessionIo, SharedSink};
use fsm_cli::mcp::serve::serve_session;
use fsm_core::json::{JsonLimits, Value, parse};

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn completion(result: &Value) -> &Value {
    result.get("completion").expect("a completion object")
}

fn values(result: &Value) -> Vec<String> {
    completion(result)
        .get("values")
        .and_then(Value::as_arr)
        .expect("values")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn total(result: &Value) -> usize {
    completion(result)
        .get("total")
        .and_then(Value::as_num)
        .unwrap()
        .parse()
        .unwrap()
}

fn has_more(result: &Value) -> bool {
    completion(result)
        .get("hasMore")
        .and_then(Value::as_bool)
        .unwrap()
}

fn ask(request: &str) -> Result<Value, String> {
    complete(Some(&value(request)), None).map_err(|invalid| invalid.0)
}

#[test]
fn the_server_advertises_completions() {
    let sink = SharedSink::new();
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n";
    serve_session(
        None,
        &mut FixedClock::new(1_000, 1),
        Cursor::new(input.as_bytes()),
        sink.writer(),
    )
    .unwrap();
    let reply = parse(
        sink.text().lines().next().unwrap().as_bytes(),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let capabilities = reply
        .get("result")
        .and_then(|r| r.get("capabilities"))
        .unwrap();
    assert!(
        capabilities.get("completions").is_some(),
        "completions is advertised: {capabilities:?}"
    );
}

#[test]
fn a_well_formed_request_answers_in_the_documented_shape() {
    let result = ask(
        r#"{"ref":{"type":"ref/prompt","name":"author_machine"},"argument":{"name":"goal","value":""}}"#,
    )
    .expect("a well-formed request");
    assert!(values(&result).is_empty(), "no supplier yet");
    assert_eq!(total(&result), 0);
    assert!(!has_more(&result));
}

#[test]
fn a_hundred_values_come_back_and_the_rest_are_admitted_to() {
    let many: Vec<String> = (0..250).map(|n| format!("inst-{n:03}")).collect();
    let result = completion_from(many, "");
    assert_eq!(values(&result).len(), MAX_VALUES);
    assert_eq!(total(&result), 250, "the count is before truncation");
    assert!(
        has_more(&result),
        "returning 100 of 250 in silence makes completion feel broken"
    );

    let few: Vec<String> = (0..40).map(|n| format!("inst-{n:02}")).collect();
    let result = completion_from(few, "");
    assert_eq!(values(&result).len(), 40);
    assert_eq!(total(&result), 40);
    assert!(!has_more(&result));
}

#[test]
fn a_prefix_matches_case_sensitively() {
    let candidates = vec![
        "Review_case".to_string(),
        "review_case".to_string(),
        "reviewer".to_string(),
    ];
    let result = completion_from(candidates.clone(), "rev");
    assert_eq!(values(&result), ["review_case", "reviewer"]);
    let result = completion_from(candidates, "Rev");
    assert_eq!(
        values(&result),
        ["Review_case"],
        "every identifier here is case-sensitive, and a suggestion that then \
         fails validation is worse than none"
    );
}

#[test]
fn the_suppliers_order_is_the_answers_order() {
    // Listings are most-recent-first, which is most-likely-first. Sorting
    // would throw that away.
    let candidates = vec![
        "inst-zulu".to_string(),
        "inst-alpha".to_string(),
        "inst-mike".to_string(),
    ];
    let result = completion_from(candidates.clone(), "inst-");
    assert_eq!(values(&result), candidates);
}

#[test]
fn an_unknown_reference_type_is_invalid_params() {
    let error =
        ask(r#"{"ref":{"type":"ref/machine","name":"x"},"argument":{"name":"id","value":""}}"#)
            .expect_err("this server cannot answer that at all");
    assert!(error.contains("ref/prompt"), "{error}");
    let error = ask(r#"{"argument":{"name":"id","value":""}}"#).expect_err("no ref");
    assert!(error.contains("ref is required"), "{error}");
}

#[test]
fn an_unknown_argument_is_an_empty_answer_rather_than_an_error() {
    // A client that completes speculatively must not be broken by asking:
    // "I have no suggestions" is a valid answer.
    let result = ask(
        r#"{"ref":{"type":"ref/prompt","name":"author_machine"},"argument":{"name":"nonesuch","value":"a"}}"#,
    )
    .expect("a known ref with an unknown argument is answerable");
    assert!(values(&result).is_empty());
    assert_eq!(total(&result), 0);
    assert!(!has_more(&result));
}

#[test]
fn a_session_with_no_store_degrades_rather_than_fails() {
    let result = ask(
        r#"{"ref":{"type":"ref/resource","uri":"fsm://instance/{id}"},"argument":{"name":"id","value":"inst-"}}"#,
    )
    .expect("a storeless session answers");
    assert!(values(&result).is_empty());
}

#[test]
fn resolved_arguments_reach_the_supplier_unchanged() {
    // The 2025-06-18 revision passes previously-resolved arguments as
    // context; `6303` completes an event name from the instance already
    // named in it, so the request must be accepted and carried now.
    let result = ask(
        r#"{"ref":{"type":"ref/prompt","name":"author_machine"},"argument":{"name":"goal","value":""},"context":{"arguments":{"instance_id":"inst-1"}}}"#,
    )
    .expect("context is accepted");
    assert_eq!(total(&result), 0);
}

#[test]
fn the_two_reference_types_parse_into_the_two_variants() {
    // Asserted through the public enum so a supplier can match on it.
    let prompt = Ref::Prompt("author_machine".to_string());
    let resource = Ref::Resource("fsm://instance/{id}".to_string());
    assert_ne!(prompt, resource);
    assert_eq!(prompt, Ref::Prompt("author_machine".to_string()));
}

#[test]
fn a_session_carries_both_of_its_halves() {
    // `6401` writes a request through the notifier and reads its response
    // from the same input. The seam exists now so that task never has to
    // reshape the serve loop.
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut input = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{}}\nsecond\n".to_vec());
    let mut io = SessionIo::new(&notifier, &mut input);
    io.notifier()
        .notify("notifications/message", Value::Obj(Default::default()))
        .unwrap();
    assert_eq!(
        io.read_line().unwrap().as_deref(),
        Some(r#"{"jsonrpc":"2.0","id":9,"result":{}}"#)
    );
    assert_eq!(io.read_line().unwrap().as_deref(), Some("second"));
    assert_eq!(io.read_line().unwrap(), None, "and then end of input");
    assert!(sink.text().contains("notifications/message"));
}

#[test]
fn a_client_that_can_be_asked_is_distinguishable_from_one_that_cannot() {
    use fsm_cli::mcp::elicit::client_supports;
    let asking = value(
        r#"{"protocolVersion":"2025-06-18","capabilities":{"elicitation":{}},"clientInfo":{"name":"c","version":"1"}}"#,
    );
    let quiet = value(
        r#"{"protocolVersion":"2025-06-18","capabilities":{"roots":{}},"clientInfo":{"name":"c","version":"1"}}"#,
    );
    assert!(client_supports(Some(&asking)));
    assert!(!client_supports(Some(&quiet)));
    assert!(!client_supports(None));
}
