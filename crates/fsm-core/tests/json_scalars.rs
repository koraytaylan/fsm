//! Verdict vectors for string unescaping and number-token validation.

use fsm_core::json::{check_number_token, unescape_string};

fn decode_spec(spec: &str) -> Vec<u8> {
    if let Some(hex) = spec.strip_prefix("hex:") {
        assert!(hex.len() % 2 == 0, "odd hex: {hex}");
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
            .collect()
    } else if let Some(lit) = spec.strip_prefix("lit:") {
        lit.as_bytes().to_vec()
    } else {
        panic!("spec must start with lit: or hex: ({spec})");
    }
}

#[test]
fn strings_txt() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/json-scalars/strings.txt"
    );
    let text = std::fs::read_to_string(path).unwrap();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let input_spec = parts.next().expect("input");
        let verdict = parts.next().expect("verdict");
        let raw_bytes = decode_spec(input_spec);
        let raw = std::str::from_utf8(&raw_bytes).expect("utf8 input");
        let got = unescape_string(raw);
        if verdict == "ERR" {
            assert!(got.is_err(), "line {} expected ERR for {raw:?}", idx + 1);
        } else {
            let want = decode_spec(verdict);
            let want_s = std::str::from_utf8(&want).expect("utf8 want");
            assert_eq!(got.unwrap(), want_s, "line {}", idx + 1);
        }
    }
}

#[test]
fn numbers_txt() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/json-scalars/numbers.txt"
    );
    let text = std::fs::read_to_string(path).unwrap();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let tok = parts.next().expect("token");
        let verdict = parts.next().expect("verdict");
        let ok = check_number_token(tok);
        match verdict {
            "OK" => assert!(ok, "line {} token {tok} should be OK", idx + 1),
            "ERR" => assert!(!ok, "line {} token {tok} should be ERR", idx + 1),
            other => panic!("bad verdict {other}"),
        }
    }
}
