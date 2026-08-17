use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{RecordError, RecordKind, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, ReplayError, fold_with};
use fsm_core::spec::load_machine_json;

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
        b.insert("limits".into(), fsm_core::record::limits_value());
        seal(0, 0, RecordKind::Genesis, Value::Obj(b), &prev)
    };
    prev = g.hash.clone();
    recs.push(g);
    let mid = fsm_core::hashes::machine_id(&def);
    let compiled = fsm_core::spec::compile_accepted(&def).unwrap();
    let tree = fsm_core::tree::Tree::build(&compiled.spec.states);
    let created_state =
        fsm_core::step::create(&compiled, &tree, &std::collections::BTreeMap::new()).unwrap();
    let inst0 = fsm_core::machine::InstanceState {
        status: created_state.status_after,
        leaf: created_state.leaf_after.clone(),
        ctx: created_state.ctx_after.clone(),
        history: created_state.history_after.clone(),
        pending: vec![],
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
        b.insert("leaf".into(), Value::Str("intake".into()));
        b.insert("state_hash".into(), Value::Str(created_hash));
        seal(2, 2, RecordKind::InstanceCreated, Value::Obj(b), &prev)
    };
    prev = created.hash.clone();
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
    assert_eq!(st.instances.get("i1").unwrap().leaf, "intake");
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
