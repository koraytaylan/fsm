use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{RecordError, RecordKind, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, ReplayError, fold_with};
use fsm_core::spec::{compile, load_machine_json};

#[test]
fn chain_and_tamper() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let compiled = compile(spec).unwrap();
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
        b.insert("limits".into(), fsm_core::record::limits_value());
        seal(0, 0, RecordKind::Genesis, Value::Obj(b), &prev)
    };
    prev = g.hash.clone();
    recs.push(g);
    let mid = compiled.machine_id.clone();
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
        b.insert("leaf".into(), Value::Str("intake".into()));
        seal(2, 2, RecordKind::InstanceCreated, Value::Obj(b), &prev)
    };
    prev = created.hash.clone();
    recs.push(created);
    let applied = {
        let mut b = BTreeMap::new();
        b.insert("instance_id".into(), Value::Str("i1".into()));
        b.insert("event".into(), Value::Str("docs_ok".into()));
        b.insert("payload".into(), Value::Obj(BTreeMap::new()));
        b.insert("request_id".into(), Value::Str("r2".into()));
        b.insert("state_hash".into(), Value::Str("s".into()));
        seal(3, 3, RecordKind::EventApplied, Value::Obj(b), &prev)
    };
    prev = applied.hash.clone();
    recs.push(applied);
    let rejected = {
        let mut b = BTreeMap::new();
        b.insert("instance_id".into(), Value::Str("i1".into()));
        b.insert("state_hash".into(), Value::Str("x".into()));
        b.insert("request_id".into(), Value::Str("r3".into()));
        seal(4, 4, RecordKind::EventRejected, Value::Obj(b), &prev)
    };
    prev = rejected.hash.clone();
    recs.push(rejected);
    let ack = {
        let mut b = BTreeMap::new();
        b.insert("instance_id".into(), Value::Str("i1".into()));
        b.insert("effect_id".into(), Value::Str("none".into()));
        b.insert("request_id".into(), Value::Str("r4".into()));
        seal(5, 5, RecordKind::EffectAcked, Value::Obj(b), &prev)
    };
    recs.push(ack);

    let mut expect_prev = zeros();
    for (i, r) in recs.iter().enumerate() {
        let line = r.to_line();
        verify_line(&line, i as u64, &expect_prev).unwrap();
        expect_prev = r.hash.clone();
    }
    let st = fold_with(recs.clone(), &mut NopSink).unwrap();
    assert_eq!(st.machines.len(), 1);
    assert_eq!(st.instances.len(), 1);
    assert_eq!(st.instances.get("i1").unwrap().leaf, "docs_review");
    assert!(st.dedup.contains_key("r1"));
    assert!(st.dedup.contains_key("r2"));

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
    body.insert("exited".into(), Value::Arr(vec![Value::Str("nope".into())]));
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
