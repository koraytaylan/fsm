#![no_main]
use libfuzzer_sys::fuzz_target;
use fsm_cli::http::request::{MAX_BODY_BYTES, read_request};
use std::io::{BufReader, Cursor};

// The workspace's first network-facing parser, given the treatment the
// others got: whatever a stranger sends, this either parses into a request
// or refuses it with a status. It never panics, and it never claims a body
// it did not read.
fuzz_target!(|data: &[u8]| {
    let mut input = BufReader::new(Cursor::new(data.to_vec()));
    match read_request(&mut input) {
        Ok(request) => {
            assert!(!request.method.is_empty(), "a parsed request has a method");
            assert!(request.path.starts_with('/'), "and an origin-form path");
            assert!(request.body.len() <= MAX_BODY_BYTES, "and a bounded body");
            assert!(request.body.len() <= data.len(), "read from the input given");
            for (name, _) in &request.headers {
                assert_eq!(
                    *name,
                    name.to_ascii_lowercase(),
                    "header names are normalised"
                );
            }
        }
        Err(refusal) => {
            assert!(
                (400..=505).contains(&refusal.status),
                "a refusal carries an HTTP status"
            );
            assert!(!refusal.message.is_empty(), "and says why");
        }
    }
});
