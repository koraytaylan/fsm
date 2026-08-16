//! Golden fixtures and round-trip properties for FSM-CJSON.

use std::fs;
use std::path::Path;

use fsm_core::canon::{canon_bytes, is_canonical};
use fsm_core::json::{JsonLimits, parse, write_canonical};

#[test]
fn golden_pairs() {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/canon"));
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("txt") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let (input, expected) = text
            .split_once("\n===\n")
            .unwrap_or_else(|| panic!("{}: expected input\\n===\\ncanonical", path.display()));
        let v = parse(input.as_bytes(), &JsonLimits::DEFAULT)
            .unwrap_or_else(|e| panic!("{} parse: {e:?}", path.display()));
        let got = canon_bytes(&v);
        let want = expected.trim_end_matches('\n').as_bytes();
        assert_eq!(
            got,
            want,
            "{}:\n got {}\nwant {}",
            path.display(),
            String::from_utf8_lossy(&got),
            String::from_utf8_lossy(want)
        );
    }
}

#[test]
fn corpus_round_trip_and_idempotence() {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/json"));
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if !name.starts_with("y_") {
            continue;
        }
        let bytes = fs::read(&path).unwrap();
        let v1 = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        let mut canon = Vec::new();
        write_canonical(&v1, &mut canon);
        let v2 = parse(&canon, &JsonLimits::DEFAULT).unwrap();
        assert_eq!(v1, v2, "{name} parse∘write∘parse identity");
        let mut canon2 = Vec::new();
        write_canonical(&v2, &mut canon2);
        assert_eq!(canon, canon2, "{name} canonicalize idempotent");
    }
}

#[test]
fn is_canonical_cases() {
    let limits = JsonLimits::DEFAULT;
    let v = parse(br#"{"b":1,"a":2}"#, &limits).unwrap();
    let c = canon_bytes(&v);
    assert_eq!(is_canonical(&c, &limits).unwrap(), true);
    let mut spaced = c.clone();
    spaced.insert(1, b' ');
    assert_eq!(is_canonical(&spaced, &limits).unwrap(), false);
    // two keys swapped relative to canonical (canonical is a then b)
    assert_eq!(is_canonical(br#"{"b":1,"a":2}"#, &limits).unwrap(), false);
    assert!(is_canonical(b"{", &limits).is_err());
}
