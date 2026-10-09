//! Independent draft fixtures exercise host evidence without a clock or writer.

use super::*;
use fsm_core::json::{JsonLimits, parse};

fn draft() -> Value {
    parse(
        include_bytes!("../../../../../tests/fixtures/contract/draft.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn arguments(spec: Value) -> Value {
    Value::Obj(BTreeMap::from([("spec".into(), spec)]))
}

fn table() -> HandlerTable {
    HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{
        "effect":"work","argv":["/PRIVATE_EXECUTABLE","PRIVATE_ARGUMENT","{value}"],"timeout_ms":1000
    }]}"#).unwrap()
}

#[test]
fn embedded_draft_matches_independent_analyzer_without_mutation() {
    let store = Store::open_memory().unwrap();
    let records = store.records.clone();
    let table = table();
    let spec = draft();
    let expected = analyze_contract(
        &compile_accepted(&spec).unwrap(),
        &BTreeMap::new(),
        &table,
        Limits::default(),
    )
    .unwrap()
    .to_value();
    let result = check(
        Some(&store),
        &arguments(spec),
        ExecutionContext::Embedded(&table),
    )
    .unwrap();
    assert_eq!(result, expected);
    assert_eq!(
        result.get("status").and_then(Value::as_str),
        Some("compatible")
    );
    assert_eq!(store.records, records);
    assert!(store.state.machines.is_empty());
    let bytes = String::from_utf8(canon_bytes(&result)).unwrap();
    assert!(!bytes.contains("PRIVATE_EXECUTABLE"));
    assert!(!bytes.contains("PRIVATE_ARGUMENT"));
}

#[test]
fn unavailable_tables_preserve_sites_without_inventing_policy() {
    let store = Store::open_memory().unwrap();
    for (context, mode) in [
        (ExecutionContext::Writer, "writer"),
        (ExecutionContext::ReadOnly, "read-only"),
        (ExecutionContext::Degraded, "degraded"),
    ] {
        let result = check(Some(&store), &arguments(draft()), context).unwrap();
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("unknown")
        );
        assert_eq!(result.get("contract_id"), Some(&Value::Null));
        let scope = result.get("scope").unwrap();
        assert_eq!(scope.get("effects_checked"), Some(&Value::Bool(false)));
        assert_eq!(scope.get("outcomes_checked"), Some(&Value::Bool(false)));
        let effects = result.get("effects").unwrap().as_arr().unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(
            effects[0].get("disposition").and_then(Value::as_str),
            Some("missing")
        );
        assert_eq!(
            effects[0].get("required_args"),
            Some(&Value::Arr(Vec::new()))
        );
        assert_eq!(
            effects[0].get("outcomes"),
            Some(&Value::Obj(BTreeMap::new()))
        );
        assert_eq!(
            effects[0]
                .get("arguments")
                .unwrap()
                .get("value")
                .and_then(Value::as_str),
            Some("int")
        );
        let findings = result.get("findings").unwrap().as_arr().unwrap();
        assert!(
            findings
                .iter()
                .all(|finding| finding.get("severity").and_then(Value::as_str) == Some("unknown"))
        );
        let provenance = findings
            .iter()
            .find_map(|finding| finding.get("cause").filter(|cause| cause.is_obj()))
            .unwrap();
        assert_eq!(
            provenance,
            &Value::Obj(BTreeMap::from([
                ("mode".into(), Value::Str(mode.into())),
                ("table".into(), Value::Str("unavailable".into())),
            ]))
        );
    }
}

#[test]
fn effect_free_draft_still_requires_host_evidence() {
    let store = Store::open_memory().unwrap();
    let mut spec = draft();
    let Value::Obj(fields) = &mut spec else {
        panic!("fixture must be an object")
    };
    fields.insert(
        "states".into(),
        Value::Arr(vec![Value::Obj(BTreeMap::from([(
            "name".into(),
            Value::Str("idle".into()),
        )]))]),
    );
    let result = check(Some(&store), &arguments(spec), ExecutionContext::Writer).unwrap();
    assert_eq!(
        result.get("status").and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(result.get("effects"), Some(&Value::Arr(Vec::new())));
    assert_eq!(result.get("findings").unwrap().as_arr().unwrap().len(), 1);
}

#[test]
fn draft_errors_remain_invalid_without_store_or_table() {
    let mut spec = draft();
    let Value::Obj(fields) = &mut spec else {
        panic!("fixture must be an object")
    };
    fields.insert("initial".into(), Value::Str("absent".into()));
    let result = check(None, &arguments(spec), ExecutionContext::Writer).unwrap();
    assert_eq!(
        result.get("status").and_then(Value::as_str),
        Some("invalid")
    );
    assert_eq!(result.get("machine_id"), Some(&Value::Null));
    assert!(
        result
            .get("findings")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .any(|finding| finding.get("severity").and_then(Value::as_str) == Some("error"))
    );
}

#[test]
fn poisoned_store_discards_embedded_table_authority() {
    let mut store = Store::open_memory().unwrap();
    store.journal.poisoned = true;
    let table = table();
    let result = check(
        Some(&store),
        &arguments(draft()),
        ExecutionContext::Embedded(&table),
    )
    .unwrap();
    assert_eq!(
        result.get("status").and_then(Value::as_str),
        Some("unknown")
    );
    assert_eq!(result.get("contract_id"), Some(&Value::Null));
    assert!(
        result
            .get("findings")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .any(|finding| finding
                .get("cause")
                .and_then(|cause| cause.get("mode"))
                .and_then(Value::as_str)
                == Some("degraded"))
    );
}

#[test]
fn read_only_snapshot_discards_table_and_resolves_stored_selectors_under_writer() {
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch(
        std::env::temp_dir().join(format!("fsm-draft-check-{}-{unique}", std::process::id())),
    );
    let mut writer = Store::open(&scratch.0).unwrap();
    let created = writer
        .define_machine_on(
            &mut crate::clock::FixedClock::new(2000, 0),
            draft(),
            false,
            false,
        )
        .unwrap();
    let snapshot = Store::open_read_only(&scratch.0).unwrap();
    let records = snapshot.records.clone();
    let table = table();
    let by_draft = check(
        Some(&snapshot),
        &arguments(draft()),
        ExecutionContext::Embedded(&table),
    )
    .unwrap();
    for identity in ["simple", &created.machine_id] {
        let result = check(
            Some(&snapshot),
            &Value::Obj(BTreeMap::from([(
                "machine".into(),
                Value::Str(identity.into()),
            )])),
            ExecutionContext::Embedded(&table),
        )
        .unwrap();
        assert_eq!(result, by_draft);
        assert_eq!(
            result.get("status").and_then(Value::as_str),
            Some("unknown")
        );
        assert_eq!(result.get("contract_id"), Some(&Value::Null));
        assert!(
            result
                .get("findings")
                .unwrap()
                .as_arr()
                .unwrap()
                .iter()
                .any(|finding| finding
                    .get("cause")
                    .and_then(|cause| cause.get("mode"))
                    .and_then(Value::as_str)
                    == Some("read-only"))
        );
    }
    assert_eq!(snapshot.records, records);
    assert_eq!(writer.records, records);
    assert_eq!(Store::open_read_only(&scratch.0).unwrap().records, records);
}

#[test]
fn unavailable_stored_definition_is_unknown_without_opening_a_store() {
    let result = check(
        None,
        &Value::Obj(BTreeMap::from([(
            "machine".into(),
            Value::Str("missing".into()),
        )])),
        ExecutionContext::Writer,
    )
    .unwrap();
    assert_eq!(
        result.get("status").and_then(Value::as_str),
        Some("unknown")
    );
    assert!(
        result
            .get("findings")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .any(|finding| finding.get("code").and_then(Value::as_str)
                == Some("exec/contract_definition_unknown"))
    );
}

#[test]
fn closed_selectors_refuse_overrides_without_echoing_private_literals() {
    let store = Store::open_memory().unwrap();
    for fields in [
        BTreeMap::new(),
        BTreeMap::from([
            ("spec".into(), draft()),
            ("machine".into(), Value::Str("simple".into())),
        ]),
        BTreeMap::from([
            ("spec".into(), draft()),
            ("handlers".into(), Value::Str("PRIVATE_OVERRIDE".into())),
        ]),
        BTreeMap::from([("machine".into(), Value::Str(String::new()))]),
    ] {
        let error = check(Some(&store), &Value::Obj(fields), ExecutionContext::Writer).unwrap_err();
        assert_eq!(error.code, "req/args_invalid");
        assert!(
            !String::from_utf8(canon_bytes(&error.to_value()))
                .unwrap()
                .contains("PRIVATE_OVERRIDE")
        );
    }
}
