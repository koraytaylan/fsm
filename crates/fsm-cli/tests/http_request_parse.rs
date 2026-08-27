//! The parser, driven by raw bytes rather than by constructed structs.
//!
//! Every case here is what a socket would actually deliver, because a
//! parser tested through its own types is a parser tested against itself.
//!
//! Plan 0015 task 6902.

use std::io::{BufReader, Cursor};

use fsm_cli::http::request::{
    MAX_BODY_BYTES, MAX_HEADER_BYTES, MAX_HEADERS, MAX_HEADERS_BYTES, MAX_REQUEST_LINE, Refusal,
    Request, read_request,
};

fn parse(raw: &str) -> Result<Request, Refusal> {
    parse_bytes(raw.as_bytes())
}

fn parse_bytes(raw: &[u8]) -> Result<Request, Refusal> {
    let mut input = BufReader::new(Cursor::new(raw.to_vec()));
    read_request(&mut input)
}

fn status(raw: &str) -> u16 {
    parse(raw).expect_err("this request is refused").status
}

#[test]
fn a_well_formed_request_parses_into_its_parts() {
    let request = parse(
        "POST /mcp?session=7 HTTP/1.1\r\nHost: localhost:9000\r\nContent-Type: application/json\r\nContent-Length: 17\r\n\r\n{\"jsonrpc\":\"2.0\"}",
    )
    .expect("well formed");
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/mcp");
    assert_eq!(request.query, "session=7");
    assert_eq!(request.body, b"{\"jsonrpc\":\"2.0\"}");
    assert_eq!(request.header("content-type"), Some("application/json"));
}

#[test]
fn header_names_are_matched_case_insensitively_and_values_kept_as_sent() {
    let request =
        parse("GET /mcp HTTP/1.1\r\nHost: localhost\r\nX-Odd-Case: Two  Spaces Inside\r\n\r\n")
            .expect("well formed");
    assert_eq!(request.header("x-odd-case"), Some("Two  Spaces Inside"));
    assert_eq!(request.header("X-ODD-CASE"), Some("Two  Spaces Inside"));
    assert_eq!(
        request.header("x-odd-case"),
        Some("Two  Spaces Inside"),
        "internal spacing is a value's own business"
    );
}

#[test]
fn each_bound_produces_its_documented_status() {
    // Request line: 414.
    let long_path = "x".repeat(MAX_REQUEST_LINE);
    assert_eq!(
        status(&format!("GET /{long_path} HTTP/1.1\r\nHost: h\r\n\r\n")),
        414
    );

    // One header over its own ceiling: 431.
    let long_value = "v".repeat(MAX_HEADER_BYTES);
    assert_eq!(
        status(&format!("GET / HTTP/1.1\r\nX-Long: {long_value}\r\n\r\n")),
        431
    );

    // Too many headers: 431.
    let many: String = (0..=MAX_HEADERS).map(|n| format!("X-{n}: v\r\n")).collect();
    assert_eq!(status(&format!("GET / HTTP/1.1\r\n{many}\r\n")), 431);

    // Headers that are individually fine and collectively are not: 431.
    let chunk = "h".repeat(1_000);
    let bulky: String = (0..(MAX_HEADERS_BYTES / 1_000) + 2)
        .map(|n| format!("X-{n}: {chunk}\r\n"))
        .collect();
    assert_eq!(status(&format!("GET / HTTP/1.1\r\n{bulky}\r\n")), 431);

    // A body over the limit: 413, and refused on the *claim* rather than
    // after reading it.
    assert_eq!(
        status(&format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        )),
        413
    );

    // A body shorter than declared: 408, and it does not wait forever.
    assert_eq!(
        status("POST / HTTP/1.1\r\nContent-Length: 100\r\n\r\nshort"),
        408
    );
}

#[test]
fn the_two_smuggling_shapes_are_refused_rather_than_reconciled() {
    assert_eq!(
        status("POST / HTTP/1.1\r\nContent-Length: 3\r\nTransfer-Encoding: chunked\r\n\r\nabc"),
        400,
        "two answers to one question are not reconciled"
    );
    assert_eq!(
        status("POST / HTTP/1.1\r\nContent-Length: 3\r\nContent-Length: 4\r\n\r\nabc"),
        400
    );
    assert_eq!(status("GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n"), 400);
}

#[test]
fn chunked_is_refused_with_the_status_that_names_the_limitation() {
    let refusal = parse("POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n")
        .expect_err("not accepted");
    assert_eq!(refusal.status, 411);
    assert!(
        refusal.message.contains("Content-Length"),
        "a caller must be told what this server does read: {}",
        refusal.message
    );
}

#[test]
fn obsolete_folding_is_refused_rather_than_unfolded() {
    assert_eq!(
        status("GET / HTTP/1.1\r\nX-Folded: first\r\n  continued\r\n\r\n"),
        400
    );
    assert_eq!(
        status("GET / HTTP/1.1\r\nX-Folded: first\r\n\tcontinued\r\n\r\n"),
        400
    );
}

#[test]
fn invalid_bytes_are_refused_and_not_sanitised() {
    // A control character in a value.
    assert_eq!(
        parse_bytes(b"GET / HTTP/1.1\r\nX-Bad: a\x01b\r\n\r\n")
            .expect_err("refused")
            .status,
        400
    );
    // A space inside a header name.
    assert_eq!(status("GET / HTTP/1.1\r\nX Bad: value\r\n\r\n"), 400);
    // A name that is not a token at all.
    assert_eq!(status("GET / HTTP/1.1\r\n(bad): value\r\n\r\n"), 400);
    // And a method that is not one.
    assert_eq!(status("GE T / HTTP/1.1\r\n\r\n"), 400);
}

#[test]
fn a_request_target_must_be_a_path() {
    assert_eq!(status("GET http://elsewhere/ HTTP/1.1\r\n\r\n"), 400);
    assert_eq!(status("GET * HTTP/1.1\r\n\r\n"), 400);
}

#[test]
fn another_protocol_version_is_told_which_one_this_is() {
    assert_eq!(status("GET / HTTP/2.0\r\n\r\n"), 505);
}

#[test]
fn no_malformed_request_panics() {
    // Forty-odd shapes a stranger could send. Each must produce a status
    // rather than an unwind — that is the whole claim a hand-rolled parser
    // on a socket has to make.
    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"\r\n".to_vec(),
        b"\n".to_vec(),
        b"\r".to_vec(),
        b"GET".to_vec(),
        b"GET /".to_vec(),
        b"GET / ".to_vec(),
        b"GET / HTTP/1.1".to_vec(),
        b"GET / HTTP/1.1\r".to_vec(),
        b"GET / HTTP/1.1\r\n".to_vec(),
        b"GET / HTTP/1.1\r\nHost".to_vec(),
        b"GET / HTTP/1.1\r\nHost:".to_vec(),
        b"GET / HTTP/1.1\r\nHost:\r\n".to_vec(),
        b"GET / HTTP/1.1\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\n:\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\n: value\r\n\r\n".to_vec(),
        b"POST / HTTP/1.1\r\nContent-Length: \r\n\r\n".to_vec(),
        b"POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n".to_vec(),
        b"POST / HTTP/1.1\r\nContent-Length: 99999999999999999999\r\n\r\n".to_vec(),
        b"POST / HTTP/1.1\r\nContent-Length: 0x10\r\n\r\n".to_vec(),
        b"POST / HTTP/1.1\r\nContent-Length: 1 2\r\n\r\n".to_vec(),
        b"POST / HTTP/1.1\r\ncontent-length: 5\r\n\r\nab".to_vec(),
        b"POST / HTTP/1.1\r\nTransfer-Encoding: identity\r\n\r\n".to_vec(),
        b"\x00\x01\x02\x03".to_vec(),
        b"\xff\xfe\xfd".to_vec(),
        b"GET /\xff HTTP/1.1\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\nX-\xff: v\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\nX: \xff\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1\r\nX: v\n\r\n".to_vec(),
        b"GET / HTTP/1.1\nHost: h\n\n".to_vec(),
        b"   GET / HTTP/1.1\r\n\r\n".to_vec(),
        b"GET  / HTTP/1.1\r\n\r\n".to_vec(),
        b"GET / HTTP/1.1 extra\r\n\r\n".to_vec(),
        b"HEAD / HTTP/1.0\r\n\r\n".to_vec(),
        b"OPTIONS * HTTP/1.1\r\n\r\n".to_vec(),
        b"DELETE /mcp HTTP/1.1\r\nMcp-Session-Id: \r\n\r\n".to_vec(),
        b"POST /mcp HTTP/1.1\r\nContent-Length: 4\r\n\r\n".to_vec(),
        b"POST /mcp HTTP/1.1\r\nContent-Length: 1\r\n\r\n\x00".to_vec(),
        b"GET / HTTP/1.1\r\nX: \t v \t\r\n\r\n".to_vec(),
        b"GET /?a=1&b=2 HTTP/1.1\r\n\r\n".to_vec(),
        b"GET /?\r\n\r\n".to_vec(),
        b"gEt / HTTP/1.1\r\n\r\n".to_vec(),
        vec![b'G'; 100_000],
        b"GET / HTTP/1.1\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\nabc".to_vec(),
    ];
    assert!(cases.len() >= 40, "{} cases", cases.len());
    for (index, case) in cases.iter().enumerate() {
        match parse_bytes(case) {
            Ok(request) => {
                // A parse that succeeds must still be a parse: the parts are
                // there and nothing was invented.
                assert!(!request.method.is_empty(), "case {index}");
                assert!(request.path.starts_with('/'), "case {index}");
            }
            Err(refusal) => {
                assert!(
                    (400..=505).contains(&refusal.status),
                    "case {index} produced status {}",
                    refusal.status
                );
                assert!(!refusal.message.is_empty(), "case {index} says nothing");
            }
        }
    }
}
