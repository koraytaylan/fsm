//! Verdict corpus for the structural JSON parser.

use std::fs;
use std::path::Path;

use fsm_core::json::{JsonErrorKind, JsonLimits, parse};

fn kind_from_name(name: &str) -> JsonErrorKind {
    let rest = name.strip_prefix("n_").expect("n_ prefix");
    let key = rest.split("__").next().unwrap();
    match key {
        "max_depth" => JsonErrorKind::MaxDepth,
        "duplicate_key" => JsonErrorKind::DuplicateKey,
        "trailing_garbage" => JsonErrorKind::TrailingGarbage,
        "bom" => JsonErrorKind::Bom,
        "control_char" => JsonErrorKind::ControlChar,
        "lone_surrogate" => JsonErrorKind::LoneSurrogate,
        "truncated" => JsonErrorKind::Truncated,
        "invalid_literal" => JsonErrorKind::InvalidLiteral,
        "invalid_number" => JsonErrorKind::InvalidNumber,
        other => panic!("unknown n_ kind in filename: {other}"),
    }
}

#[test]
fn corpus_verdicts() {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/json"));
    let mut seen = 0u32;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&path).unwrap();
        if name.starts_with("y_") {
            let v = parse(&bytes, &JsonLimits::DEFAULT)
                .unwrap_or_else(|e| panic!("{name} should parse: {e:?}"));
            if name.contains("number_1e309") {
                assert_eq!(v.as_num(), Some("1e309"));
            }
            if name.contains("number_40digit") {
                assert_eq!(v.as_num(), Some("1234567890123456789012345678901234567890"));
            }
            if name.contains("surrogate") {
                assert_eq!(v.as_str(), Some("😀"));
            }
            if name.contains("unicode_keys") {
                let keys: Vec<_> = v.as_obj().unwrap().keys().cloned().collect();
                let mut sorted = keys.clone();
                sorted.sort();
                assert_eq!(keys, sorted);
            }
            seen += 1;
        } else if name.starts_with("n_") {
            let err =
                parse(&bytes, &JsonLimits::DEFAULT).expect_err(&format!("{name} should reject"));
            let want = kind_from_name(&name);
            assert_eq!(err.kind, want, "{name}");
            seen += 1;
        } else {
            panic!("fixture {name} matches neither y_ nor n_ prefix");
        }
    }
    assert!(seen > 0, "no fixtures found");
}
