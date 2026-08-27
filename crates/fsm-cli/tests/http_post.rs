//! One JSON-RPC message per POST, and how its answer travels.
//!
//! Plan 0015 task 7002.

use std::collections::BTreeMap;
use std::io::{BufReader, Cursor};

use fsm_cli::clock::FixedClock;
use fsm_cli::http::endpoint::{ALLOWED_METHODS, DEFAULT_PATH, Endpoint};
use fsm_cli::http::request::{Request, read_request};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-post-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"http_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "http_case",
            "inst-h",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

/// One raw request, parsed the way the socket would deliver it.
fn raw(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
    let mut text = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        text.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    text.push_str("\r\n");
    text.push_str(body);
    let mut input = BufReader::new(Cursor::new(text.into_bytes()));
    read_request(&mut input).expect("the fixture is a well-formed request")
}

/// Serve one request and return the raw HTTP response.
fn serve(endpoint: &Endpoint, request: &Request) -> String {
    let mut out = Vec::new();
    endpoint
        .serve(request, &mut FixedClock::new(2_000, 1), &mut out)
        .expect("a response is written");
    String::from_utf8(out).expect("responses are UTF-8 here")
}

fn status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

fn body(response: &str) -> String {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

fn header(response: &str, name: &str) -> Option<String> {
    response
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with(&format!("{}:", name.to_ascii_lowercase()))
        })
        .map(|line| line.split_once(':').unwrap().1.trim().to_string())
}

const INITIALIZE: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

fn open(endpoint: &Endpoint) -> String {
    let response = serve(endpoint, &raw("POST", DEFAULT_PATH, &[], INITIALIZE));
    header(&response, "Mcp-Session-Id").expect("initialize mints a session")
}

#[test]
fn initialize_answers_in_json_and_mints_a_session() {
    let dir = scratch("init");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let response = serve(&endpoint, &raw("POST", DEFAULT_PATH, &[], INITIALIZE));
    assert_eq!(status(&response), 200);
    assert_eq!(
        header(&response, "Content-Type").as_deref(),
        Some("application/json")
    );
    let id = header(&response, "Mcp-Session-Id").expect("a session id");
    assert_eq!(id.len(), 32, "{id}");
    let answered = value(&body(&response));
    assert!(
        answered
            .get("result")
            .and_then(|r| r.get("capabilities"))
            .is_some(),
        "{answered:?}"
    );
}

#[test]
fn a_notification_and_a_response_are_accepted_without_a_body() {
    let dir = scratch("accepted");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open(&endpoint);
    let headers = [("Mcp-Session-Id", session.as_str())];

    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &headers,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ),
    );
    assert_eq!(status(&response), 202);
    assert_eq!(
        body(&response),
        "",
        "a body would invite a client to parse one"
    );

    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &headers,
            r#"{"jsonrpc":"2.0","id":"fsm-elicit-99","result":{"action":"decline"}}"#,
        ),
    );
    assert_eq!(status(&response), 202);
    assert_eq!(body(&response), "");
}

#[test]
fn an_ordinary_call_answers_in_json() {
    let dir = scratch("call");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open(&endpoint);
    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &[("Mcp-Session-Id", session.as_str())],
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"inst-h"}}}"#,
        ),
    );
    assert_eq!(status(&response), 200);
    assert_eq!(
        header(&response, "Content-Type").as_deref(),
        Some("application/json"),
        "a stream for one message is overhead a client has to unwrap"
    );
    assert!(
        body(&response).contains("structuredContent"),
        "{}",
        body(&response)
    );
}

#[test]
fn a_call_that_speaks_first_answers_in_a_stream() {
    let dir = scratch("stream");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open(&endpoint);
    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &[
                ("Mcp-Session-Id", session.as_str()),
                ("Accept", "application/json, text/event-stream"),
            ],
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"simulate","arguments":{"machine":"http_case","events":[{"name":"push"},{"name":"push"}]},"_meta":{"progressToken":"p"}}}"#,
        ),
    );
    assert_eq!(status(&response), 200);
    assert_eq!(
        header(&response, "Content-Type").as_deref(),
        Some("text/event-stream")
    );
    let text = body(&response);
    assert!(
        text.contains("notifications/progress"),
        "progress reaches the client before the answer: {text:.300}"
    );
    // Framed: every event has an id line, a data line, and a blank line.
    assert!(text.starts_with("id: 1\ndata: "), "{text:.120}");
    assert!(text.ends_with("\n\n"), "{text:.120}");
    let events = text.matches("data: ").count();
    assert!(events >= 2, "progress and the response are separate events");
    assert!(
        text.contains("\"id\":3"),
        "and the response itself is the last of them: {text:.400}"
    );
}

#[test]
fn a_client_that_cannot_read_a_stream_is_told_so() {
    let dir = scratch("accept");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let session = open(&endpoint);
    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &[
                ("Mcp-Session-Id", session.as_str()),
                ("Accept", "application/json"),
            ],
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"simulate","arguments":{"machine":"http_case","events":[{"name":"push"}]},"_meta":{"progressToken":"p"}}}"#,
        ),
    );
    assert_eq!(
        status(&response),
        406,
        "a stream a client cannot read is worse than a refusal"
    );
}

#[test]
fn a_batch_is_refused_in_the_words_stdio_uses() {
    let dir = scratch("batch");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &[],
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
        ),
    );
    assert_eq!(status(&response), 200);
    assert!(
        body(&response).contains("batch requests are not supported"),
        "{}",
        body(&response)
    );
}

#[test]
fn a_malformed_body_is_a_protocol_error_not_an_http_one() {
    let dir = scratch("malformed");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let response = serve(&endpoint, &raw("POST", DEFAULT_PATH, &[], "{not json"));
    assert_eq!(
        status(&response),
        200,
        "the transport delivered exactly what was sent"
    );
    let answered = value(&body(&response));
    assert_eq!(
        answered
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_num),
        Some("-32700")
    );
    assert_eq!(
        answered
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str),
        Some("parse error"),
        "the same error object the stdio loop produces"
    );
}

#[test]
fn an_unknown_session_is_not_found_and_a_missing_one_is_a_bad_request() {
    let dir = scratch("sessions");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let call = r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#;
    assert_eq!(
        status(&serve(&endpoint, &raw("POST", DEFAULT_PATH, &[], call))),
        400
    );
    assert_eq!(
        status(&serve(
            &endpoint,
            &raw(
                "POST",
                DEFAULT_PATH,
                &[("Mcp-Session-Id", "00000000000000000000000000000000")],
                call
            )
        )),
        404
    );
}

#[test]
fn the_other_methods_and_paths_are_answered_the_way_they_should_be() {
    let dir = scratch("methods");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    for method in ["PUT", "PATCH"] {
        let response = serve(&endpoint, &raw(method, DEFAULT_PATH, &[], ""));
        assert_eq!(status(&response), 405, "{method}");
        assert_eq!(
            header(&response, "Allow").as_deref(),
            Some(ALLOWED_METHODS),
            "{method}: a 405 must say what is allowed"
        );
    }
    assert_eq!(
        status(&serve(
            &endpoint,
            &raw("POST", "/elsewhere", &[], INITIALIZE)
        )),
        404
    );

    // DELETE ends a session, and ending it twice is not a second ending.
    let session = open(&endpoint);
    let headers = [("Mcp-Session-Id", session.as_str())];
    assert_eq!(
        status(&serve(
            &endpoint,
            &raw("DELETE", DEFAULT_PATH, &headers, "")
        )),
        200
    );
    assert_eq!(
        status(&serve(
            &endpoint,
            &raw("DELETE", DEFAULT_PATH, &headers, "")
        )),
        404
    );
    assert_eq!(
        status(&serve(
            &endpoint,
            &raw(
                "POST",
                DEFAULT_PATH,
                &headers,
                r#"{"jsonrpc":"2.0","id":6,"method":"ping"}"#
            )
        )),
        404,
        "a deleted session is gone, not merely closed"
    );
}

/// An `initialize` that says this client can be asked things.
const INITIALIZE_ASKING: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"elicitation":{}}}}"#;

#[test]
fn an_answer_reaches_the_question_that_is_waiting_for_it() {
    // Over stdio a client's answer arrives as a line on the same stream.
    // Here it arrives as a separate POST, on another connection, while the
    // first one is still open — which is a routing difference and not a
    // protocol one.
    let dir = scratch("elicit");
    let endpoint = std::sync::Arc::new(Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), ""));
    let response = serve(
        &endpoint,
        &raw("POST", DEFAULT_PATH, &[], INITIALIZE_ASKING),
    );
    let session = header(&response, "Mcp-Session-Id").unwrap();

    let asking = {
        let endpoint = std::sync::Arc::clone(&endpoint);
        let session = session.clone();
        std::thread::spawn(move || {
            serve(
                &endpoint,
                &raw(
                    "POST",
                    DEFAULT_PATH,
                    &[
                        ("Mcp-Session-Id", session.as_str()),
                        ("Accept", "text/event-stream"),
                    ],
                    r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"instance_elicit","arguments":{"instance_id":"inst-h","event":"push","request_id":"ask-1"}}}"#,
                ),
            )
        })
    };

    // The answer, posted by the client on another connection. The id is the
    // one the server minted, which a real client reads off the stream; here
    // the first outstanding id is enough to be that answer.
    let answered = {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut sent = false;
        while std::time::Instant::now() < deadline && !sent {
            std::thread::sleep(std::time::Duration::from_millis(20));
            for n in 1..40u64 {
                let body = format!(
                    r#"{{"jsonrpc":"2.0","id":"fsm-elicit-{n}","result":{{"action":"decline"}}}}"#
                );
                let response = serve(
                    &endpoint,
                    &raw(
                        "POST",
                        DEFAULT_PATH,
                        &[("Mcp-Session-Id", session.as_str())],
                        &body,
                    ),
                );
                assert_eq!(status(&response), 202);
            }
            sent = true;
        }
        asking.join().expect("the asking thread finished")
    };

    assert_eq!(status(&answered), 200, "{answered:.200}");
    assert!(
        answered.contains("decline") || answered.contains("elicit"),
        "the waiting call was completed by the answer: {answered:.400}"
    );
}

#[test]
fn an_answer_for_one_session_does_not_complete_anothers_wait() {
    let dir = scratch("crosstalk");
    let endpoint = Endpoint::new(DEFAULT_PATH, Some(seeded(&dir)), "");
    let first = header(
        &serve(
            &endpoint,
            &raw("POST", DEFAULT_PATH, &[], INITIALIZE_ASKING),
        ),
        "Mcp-Session-Id",
    )
    .unwrap();
    let second = header(
        &serve(
            &endpoint,
            &raw("POST", DEFAULT_PATH, &[], INITIALIZE_ASKING),
        ),
        "Mcp-Session-Id",
    )
    .unwrap();
    assert_ne!(first, second);

    // An answer posted under the second session must not be readable by the
    // first: one client's reply completing another client's question would
    // be the worst bug this transport could have.
    let response = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &[("Mcp-Session-Id", second.as_str())],
            r#"{"jsonrpc":"2.0","id":"fsm-elicit-1","result":{"action":"accept","content":{}}}"#,
        ),
    );
    assert_eq!(status(&response), 202);

    // The first session's ask times out rather than being answered by it.
    let asked = serve(
        &endpoint,
        &raw(
            "POST",
            DEFAULT_PATH,
            &[
                ("Mcp-Session-Id", first.as_str()),
                ("Accept", "text/event-stream"),
            ],
            r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"instance_elicit","arguments":{"instance_id":"inst-h","event":"push","request_id":"ask-2"}}}"#,
        ),
    );
    assert!(
        !asked.contains("\"accept\""),
        "another session's answer completed this one: {asked:.400}"
    );
}
