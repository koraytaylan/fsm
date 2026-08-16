use fsm_core::hashes::{ResolveError, domain_hash, machine_id, resolve_machine_ref};
use fsm_core::json::{JsonLimits, Value, parse};

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap()
}

#[test]
fn identity_jsonl() {
    let text = include_str!("fixtures/hashes/identity.jsonl");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hashes");
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let rec =
            parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap_or_else(|e| panic!("{idx}: {e:?}"));
        let v = if let Some(f) = rec.get("file").and_then(Value::as_str) {
            let bytes = std::fs::read(root.join(f)).unwrap();
            parse(&bytes, &JsonLimits::DEFAULT).unwrap()
        } else {
            rec.get("inline").unwrap().clone()
        };
        assert_eq!(machine_id(&v), s(&rec, "id"), "line {idx}");
    }
}

#[test]
fn resolve_cases() {
    let ids = [
        "a@sha256:aaaaaaaaaaaa1111111111111111111111111111111111111111111111111111",
        "a@sha256:aaaaaaaaaaaa2222222222222222222222222222222222222222222222222222",
        "b@sha256:bbbbbbbbbbbb3333333333333333333333333333333333333333333333333333",
    ];
    assert_eq!(
        resolve_machine_ref(ids.iter().copied(), ids[2]).unwrap(),
        ids[2]
    );
    assert_eq!(
        resolve_machine_ref(ids.iter().copied(), "b@sha256:bbbbbbbbbbbb").unwrap(),
        ids[2]
    );
    assert!(matches!(
        resolve_machine_ref(ids.iter().copied(), "b@sha256:bbbbbbbbbbb"),
        Err(ResolveError::TooShort)
    ));
    assert!(matches!(
        resolve_machine_ref(ids.iter().copied(), "a@sha256:aaaaaaaaaaaa"),
        Err(ResolveError::Ambiguous(_))
    ));
    assert_eq!(
        resolve_machine_ref(ids.iter().copied(), "b").unwrap(),
        ids[2]
    );
    assert!(matches!(
        resolve_machine_ref(ids.iter().copied(), "a"),
        Err(ResolveError::Ambiguous(_))
    ));
    assert!(matches!(
        resolve_machine_ref(ids.iter().copied(), "zzz"),
        Err(ResolveError::NotFound)
    ));
}

#[test]
fn description_changes_id() {
    let a = parse(
        br#"{"format":"fsm.machine/1","name":"n","description":"x","states":[],"initial":"z"}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let b = parse(
        br#"{"format":"fsm.machine/1","name":"n","description":"y","states":[],"initial":"z"}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_ne!(machine_id(&a), machine_id(&b));
}

#[test]
fn domain_hash_differs_by_tag() {
    let v = parse(b"{}", &JsonLimits::DEFAULT).unwrap();
    assert_ne!(domain_hash("fsm:machine:1", &v), domain_hash("other", &v));
}
