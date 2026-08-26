use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::ActiveConfiguration;
use fsm_core::record::{RecordError, RecordKind, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, ReplayError, fold_with};

#[test]
fn chain_and_tamper() {
    let def = parse(
        include_bytes!("fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let mut prev = zeros();
    let mut recs = Vec::new();
    let g = {
        let mut b = BTreeMap::new();
        b.insert("format".into(), Value::Str("fsm.journal/1".into()));
        b.insert("created_ts".into(), Value::Num("0".into()));
        b.insert("limits".into(), fsm_core::record::limits_value());
        seal(0, 0, RecordKind::Genesis, Value::Obj(b), &prev)
    };
    prev = g.hash.clone();
    recs.push(g);
    let mid = fsm_core::hashes::machine_id(&def);
    let compiled = fsm_core::spec::compile_accepted(&def).unwrap();
    let tree = fsm_core::tree::Tree::for_machine(&compiled.spec);
    let created_state =
        fsm_core::step::create(&compiled, &tree, &std::collections::BTreeMap::new(), 0).unwrap();
    let inst0 = fsm_core::machine::InstanceState {
        status: created_state.status_after,
        configuration: created_state.configuration_after.clone(),
        ctx: created_state.ctx_after.clone(),
        history: created_state.history_after.clone(),
        deadlines: created_state.deadlines_after.clone(),
        pending: vec![],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let created_hash = fsm_core::hashes::state_hash(&mid, "i1", 2, &inst0);
    let defn = {
        let mut b = BTreeMap::new();
        b.insert("machine_id".into(), Value::Str(mid.clone()));
        b.insert("def".into(), def);
        seal(1, 1, RecordKind::MachineDefined, Value::Obj(b), &prev)
    };
    prev = defn.hash.clone();
    recs.push(defn);
    let created = {
        let mut b = BTreeMap::new();
        b.insert("instance_id".into(), Value::Str("i1".into()));
        b.insert("machine_id".into(), Value::Str(mid));
        b.insert("request_id".into(), Value::Str("r1".into()));
        b.insert(
            "state_format".into(),
            Value::Str(fsm_core::hashes::STATE_FORMAT.into()),
        );
        b.insert(
            "configuration".into(),
            fsm_core::hashes::configuration_value(&inst0.configuration),
        );
        b.insert("state_hash".into(), Value::Str(created_hash));
        b.insert("overrides".into(), Value::Obj(BTreeMap::new()));
        seal(2, 2, RecordKind::InstanceCreated, Value::Obj(b), &prev)
    };
    recs.push(created);

    let mut expect_prev = zeros();
    for (i, r) in recs.iter().enumerate() {
        let line = r.to_line();
        verify_line(&line, i as u64, &expect_prev).unwrap();
        expect_prev = r.hash.clone();
    }
    let st = fold_with(recs.clone(), &mut NopSink).unwrap();
    assert_eq!(st.machines.len(), 1);
    assert_eq!(st.instances.len(), 1);
    assert!(matches!(
        st.instances.get("i1").unwrap().configuration,
        ActiveConfiguration::Sequential { ref leaf } if leaf == "intake"
    ));
    assert!(st.dedup.contains_key("r1"));

    // tampered hash
    let mut bad = recs[1].to_line();
    if let Some(i) = bad.iter().position(|&b| b == b'a') {
        bad[i] = b'b';
    }
    assert!(matches!(
        verify_line(&bad, 1, &recs[0].hash),
        Err(RecordError::HashMismatch { .. })
            | Err(RecordError::NonCanonical { .. })
            | Err(RecordError::Parse { .. })
    ));

    // seq gap
    assert!(matches!(
        verify_line(&recs[2].to_line(), 9, &recs[1].hash),
        Err(RecordError::SeqGap { .. })
    ));

    // non-canonical
    let mut spaced = recs[1].to_line();
    spaced.insert(1, b' ');
    assert!(matches!(
        verify_line(&spaced, 1, &recs[0].hash),
        Err(RecordError::NonCanonical { .. })
    ));

    // field mismatch: reseal applied with wrong exited
    let mut body = BTreeMap::new();
    body.insert("instance_id".into(), Value::Str("i1".into()));
    body.insert("event".into(), Value::Str("docs_ok".into()));
    body.insert("payload".into(), Value::Obj(BTreeMap::new()));
    body.insert("request_id".into(), Value::Str("r2".into()));
    body.insert(
        "state_hash".into(),
        Value::Str(format!("sha256:{}", "ab".repeat(32))),
    );
    body.insert("exited".into(), Value::Arr(vec![Value::Str("nope".into())]));
    body.insert("entered".into(), Value::Arr(vec![]));
    body.insert("source_state".into(), Value::Str("intake".into()));
    let tampered = seal(
        3,
        3,
        RecordKind::EventApplied,
        Value::Obj(body),
        &recs[2].hash,
    );
    let mut chain = recs[..3].to_vec();
    chain.push(tampered);
    match fold_with(chain, &mut NopSink) {
        Err(ReplayError::FieldMismatch { field, .. }) => assert_eq!(field, "exited"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn genesis_limits_must_match_table() {
    let mut b = BTreeMap::new();
    b.insert("format".into(), Value::Str("fsm.journal/1".into()));
    b.insert("created_ts".into(), Value::Num("0".into()));
    b.insert("limits".into(), Value::Null);
    let g = seal(0, 0, RecordKind::Genesis, Value::Obj(b), &zeros());
    assert!(matches!(
        verify_line(&g.to_line(), 0, &zeros()),
        Err(RecordError::BodyInvalid { .. })
    ));
}

#[test]
fn genesis_limits_bind_new_definition_ceilings_but_accept_the_exact_legacy_table() {
    let Value::Obj(current) = fsm_core::record::limits_value() else {
        panic!("limits must be an object")
    };
    assert_eq!(
        current.get("max_regions").and_then(Value::as_num),
        Some("8")
    );
    assert_eq!(
        current.get("max_deadlines").and_then(Value::as_num),
        Some("128")
    );
    assert_eq!(
        current.get("max_eval_ticks").and_then(Value::as_num),
        Some("4096")
    );

    let mut legacy = current.clone();
    legacy.remove("max_regions");
    legacy.remove("max_deadlines");
    legacy.remove("max_eval_ticks");
    let legacy_record = seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), Value::Obj(legacy.clone())),
        ])),
        &zeros(),
    );
    verify_line(&legacy_record.to_line(), 0, &zeros()).unwrap();

    legacy.insert("max_regions".into(), Value::Num("8".into()));
    let mixed_record = seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), Value::Obj(legacy)),
        ])),
        &zeros(),
    );
    assert!(matches!(
        verify_line(&mixed_record.to_line(), 0, &zeros()),
        Err(RecordError::BodyInvalid { seq: 0 })
    ));
}

#[test]
fn event_rejected_requires_details() {
    let mut b = BTreeMap::new();
    b.insert("instance_id".into(), Value::Str("i1".into()));
    b.insert("request_id".into(), Value::Str("r".into()));
    b.insert("event".into(), Value::Str("go".into()));
    b.insert("payload".into(), Value::Obj(BTreeMap::new()));
    b.insert(
        "state_hash".into(),
        Value::Str(format!("sha256:{}", "ab".repeat(32))),
    );
    b.insert("code".into(), Value::Str("run/unhandled".into()));
    b.insert("message".into(), Value::Str("no".into()));
    b.insert("hint".into(), Value::Str("h".into()));
    let rec = seal(1, 1, RecordKind::EventRejected, Value::Obj(b), &zeros());
    assert!(matches!(
        verify_line(&rec.to_line(), 1, &zeros()),
        Err(RecordError::BodyInvalid { .. })
    ));
}
