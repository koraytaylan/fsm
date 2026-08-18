//! Seeded chaos suite. The ~30-line xorshift64* is duplicated with proputil on purpose.

use fsm_cli::journal_io::verify;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::{NopSink, fold_with};
use std::collections::BTreeMap;

struct Gen(u64);
impl Gen {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() as u32) % (hi - lo + 1)
    }
}

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn tmp(seed: u64) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("fsm-chaos-{}-{seed}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn storm() {
    if let Ok(s) = std::env::var("CHAOS_SEED") {
        run_seed(s.parse().unwrap());
        return;
    }
    let mut kinds = BTreeMap::new();
    let mut reopens = 0;
    for i in 0..200u64 {
        let seed = 0xC0FFEE + i * 17;
        let (k, _log) = run_seed(seed);
        for (name, n) in &k {
            *kinds.entry(*name).or_insert(0) += *n;
        }
        if k.get("reopen").copied().unwrap_or(0) > 0 {
            reopens += 1;
        }
    }
    for need in [
        "define",
        "create",
        "send",
        "ack",
        "cancel",
        "typed_err",
        "stale",
        "unknown_ack",
        "dup",
        "expect_seq",
        "ack_ok",
    ] {
        assert!(kinds.get(need).copied().unwrap_or(0) > 0, "missing {need}");
    }
    assert!(reopens >= 100, "reopens {reopens}");
}

fn push_result(
    log: &mut Vec<u8>,
    kind: &'static str,
    kinds: &mut BTreeMap<&'static str, u32>,
    r: Result<Value, fsm_cli::store::ErrorObj>,
) -> bool {
    match r {
        Ok(v) => {
            *kinds.entry(kind).or_insert(0) += 1;
            log.extend(fsm_core::canon::canon_bytes(&v));
            true
        }
        Err(e) => {
            *kinds.entry("err").or_insert(0) += 1;
            *kinds.entry(kind).or_insert(0) += 1;
            assert!(!e.hint.is_empty(), "error {} missing hint", e.code);
            log.extend(fsm_core::canon::canon_bytes(&e.to_value()));
            false
        }
    }
}

fn run_seed(seed: u64) -> (BTreeMap<&'static str, u32>, Vec<u8>) {
    let dir = tmp(seed);
    let mut g = Gen(seed);
    let mut store = Store::open(&dir).unwrap_or_else(|e| panic!("seed {seed} open {e:?}"));
    let mut kinds = BTreeMap::new();
    let mut log = Vec::new();
    push_result(
        &mut log,
        "define",
        &mut kinds,
        store.define_machine(case(), false, false).map(|o| {
            Value::Obj(BTreeMap::from([(
                "machine_id".into(),
                Value::Str(o.machine_id),
            )]))
        }),
    );
    let n = g.range(30, 80);
    let mut iid = 1u32;
    let mut ledger: Vec<(String, bool)> = Vec::new();
    let tid = format!("i{iid}");
    push_result(
        &mut log,
        "create",
        &mut kinds,
        store.create_instance("case_review", &tid, &format!("c{iid}"), None),
    );
    push_result(
        &mut log,
        "send",
        &mut kinds,
        store.send_event(
            &tid,
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "typed-d1",
            None,
        ),
    );
    let mut last_rid = String::from("typed-d1");
    #[derive(Clone)]
    struct SendTuple {
        id: String,
        event: String,
        payload: Value,
        rid: String,
        expect_seq: Option<u64>,
    }
    let mut last_ok_send = Some(SendTuple {
        id: tid.clone(),
        event: "docs_ok".into(),
        payload: Value::Obj(BTreeMap::new()),
        rid: last_rid.clone(),
        expect_seq: None,
    });
    let mut tuples: Vec<String> = Vec::new();
    ledger.push((last_rid.clone(), true));
    let typed_err = store.send_event(
        &tid,
        "scored",
        Value::Obj(BTreeMap::from([("score".into(), Value::Bool(true))])),
        "typed-bad",
        None,
    );
    assert_eq!(
        typed_err.as_ref().err().map(|e| e.code.as_str()),
        Some("req/field_type")
    );
    push_result(&mut log, "typed_err", &mut kinds, typed_err);
    tuples.push("typed_err scored bool".into());
    let stale = store.send_event(
        &tid,
        "docs_ok",
        Value::Obj(BTreeMap::new()),
        "stale-1",
        Some(0),
    );
    assert_eq!(
        stale.as_ref().err().map(|e| e.code.as_str()),
        Some("req/seq_mismatch")
    );
    push_result(&mut log, "stale", &mut kinds, stale);
    tuples.push("stale expect_seq=0".into());
    let unk = store.ack_effect(&tid, "none", "ack-unknown");
    assert_eq!(
        unk.as_ref().err().map(|e| e.code.as_str()),
        Some("req/field_unknown")
    );
    push_result(&mut log, "unknown_ack", &mut kinds, unk);
    tuples.push("unknown_ack none".into());
    let dup0 = store.send_event(
        &tid,
        "docs_ok",
        Value::Obj(BTreeMap::new()),
        "typed-d1",
        None,
    );
    assert_eq!(
        dup0.as_ref()
            .ok()
            .and_then(|v| v.get("duplicate").and_then(Value::as_bool)),
        Some(true)
    );
    push_result(&mut log, "dup", &mut kinds, dup0);
    push_result(
        &mut log,
        "expect_seq",
        &mut kinds,
        store.send_event(
            &tid,
            "note_added",
            Value::Obj(BTreeMap::from([("text".into(), Value::Str("n".into()))])),
            "expect-live",
            Some(store.journal.last_seq),
        ),
    );
    push_result(
        &mut log,
        "send",
        &mut kinds,
        store.send_event(
            &tid,
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "typed-d2",
            None,
        ),
    );
    push_result(
        &mut log,
        "typed",
        &mut kinds,
        store.send_event(
            &tid,
            "scored",
            Value::Obj(BTreeMap::from([("score".into(), Value::Str("800".into()))])),
            "typed-ok",
            None,
        ),
    );
    for _ in 0..n {
        match g.range(0, 10) {
            0 => {
                push_result(
                    &mut log,
                    "define",
                    &mut kinds,
                    store.define_machine(case(), false, false).map(|o| {
                        Value::Obj(BTreeMap::from([(
                            "machine_id".into(),
                            Value::Str(o.machine_id),
                        )]))
                    }),
                );
            }
            1 => {
                iid += 1;
                let id = format!("i{iid}");
                push_result(
                    &mut log,
                    "create",
                    &mut kinds,
                    store.create_instance("case_review", &id, &format!("c{iid}"), None),
                );
            }
            2 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let ev = if g.range(0, 3) == 0 {
                    "nope"
                } else {
                    "docs_ok"
                };
                last_rid = format!("s{}", g.next());
                let ok = push_result(
                    &mut log,
                    "send",
                    &mut kinds,
                    store.send_event(&id, ev, Value::Obj(BTreeMap::new()), &last_rid, None),
                );
                ledger.push((last_rid.clone(), ok));
                if ok {
                    last_ok_send = Some(SendTuple {
                        id: id.clone(),
                        event: ev.into(),
                        payload: Value::Obj(BTreeMap::new()),
                        rid: last_rid.clone(),
                        expect_seq: None,
                    });
                }
            }
            3 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let pending = store
                    .state
                    .instances
                    .get(&id)
                    .and_then(|st| st.pending.first().cloned());
                if let Some(eid) = pending {
                    if push_result(
                        &mut log,
                        "ack_ok",
                        &mut kinds,
                        store.ack_effect(&id, &eid, &format!("a{}", g.next())),
                    ) {
                        *kinds.entry("ack").or_insert(0) += 1;
                    }
                } else {
                    push_result(
                        &mut log,
                        "ack",
                        &mut kinds,
                        store.ack_effect(&id, "none", &format!("a{}", g.next())),
                    );
                }
            }
            4 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                push_result(
                    &mut log,
                    "cancel",
                    &mut kinds,
                    store.cancel_instance(&id, &format!("k{}", g.next())),
                );
            }
            5 => {
                drop(store);
                store = Store::open(&dir).unwrap_or_else(|e| panic!("seed {seed} reopen {e:?}"));
                *kinds.entry("reopen").or_insert(0) += 1;
            }
            6 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                last_rid = format!("t{}", g.next());
                push_result(
                    &mut log,
                    "typed",
                    &mut kinds,
                    store.send_event(
                        &id,
                        "scored",
                        Value::Obj(BTreeMap::from([("score".into(), Value::Bool(true))])),
                        &last_rid,
                        None,
                    ),
                );
            }
            7 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                push_result(
                    &mut log,
                    "dup",
                    &mut kinds,
                    store.send_event(&id, "docs_ok", Value::Obj(BTreeMap::new()), &last_rid, None),
                );
            }
            8 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                last_rid = format!("e{}", g.next());
                let exp = if g.range(0, 1) == 0 {
                    Some(0)
                } else {
                    Some(store.journal.last_seq)
                };
                let ok = push_result(
                    &mut log,
                    "expect_seq",
                    &mut kinds,
                    store.send_event(&id, "docs_ok", Value::Obj(BTreeMap::new()), &last_rid, exp),
                );
                ledger.push((last_rid.clone(), ok));
                if ok {
                    last_ok_send = Some(SendTuple {
                        id: id.clone(),
                        event: "docs_ok".into(),
                        payload: Value::Obj(BTreeMap::new()),
                        rid: last_rid.clone(),
                        expect_seq: exp,
                    });
                }
            }
            _ => {
                push_result(
                    &mut log,
                    "define",
                    &mut kinds,
                    store
                        .define_machine(
                            parse(br#"{"format":"fsm.machine/1"}"#, &JsonLimits::DEFAULT).unwrap(),
                            false,
                            false,
                        )
                        .map(|o| {
                            Value::Obj(BTreeMap::from([(
                                "machine_id".into(),
                                Value::Str(o.machine_id),
                            )]))
                        }),
                );
            }
        }
    }
    if let Some(t) = last_ok_send {
        let again = store.send_event(&t.id, &t.event, t.payload.clone(), &t.rid, t.expect_seq);
        assert!(again.is_ok(), "identical accepted retry {again:?}");
        let v = again.unwrap();
        assert_eq!(v.get("duplicate").and_then(Value::as_bool), Some(true));
    }
    assert!(!tuples.is_empty());
    if kinds.get("ack_ok").copied().unwrap_or(0) == 0 {
        for (id, inst) in store.state.instances.clone() {
            if let Some(eid) = inst.pending.first().cloned() {
                let ok = push_result(
                    &mut log,
                    "ack_ok",
                    &mut kinds,
                    store.ack_effect(&id, &eid, &format!("forced-{}", g.next())),
                );
                assert!(ok, "forced pending ack failed");
                *kinds.entry("ack").or_insert(0) += 1;
                break;
            }
        }
    }
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let folded = fold_with(recs, &mut NopSink).unwrap_or_else(|e| panic!("seed {seed} fold {e:?}"));
    assert_eq!(store.state.last_seq, folded.last_seq, "seed {seed} seq");
    if store.state.last_seq > 0 {
        assert_eq!(store.state.last_hash, folded.last_hash, "seed {seed} hash");
    }
    assert_eq!(store.state.dedup, folded.dedup, "seed {seed} dedup");
    assert_eq!(
        store.state.machines.len(),
        folded.machines.len(),
        "seed {seed} machines"
    );
    for (id, m) in &store.state.machines {
        let o = folded.machines.get(id).expect(id);
        assert_eq!(
            m.compiled.machine_id, o.compiled.machine_id,
            "seed {seed} machine {id}"
        );
    }
    assert_eq!(
        store.state.instance_machines, folded.instance_machines,
        "seed {seed} instance_machines"
    );
    assert_eq!(
        store.state.instances.len(),
        folded.instances.len(),
        "seed {seed} inst"
    );
    for (id, st) in &store.state.instances {
        let o = folded.instances.get(id).expect(id);
        assert_eq!(st.leaf, o.leaf, "seed {seed} {id} leaf");
        assert_eq!(st.status, o.status, "seed {seed} {id} status");
        assert_eq!(st.ctx, o.ctx, "seed {seed} {id} ctx");
        assert_eq!(st.history, o.history, "seed {seed} {id} hist");
        assert_eq!(st.pending, o.pending, "seed {seed} {id} pend");
    }
    for (rid, ok) in &ledger {
        if *ok {
            assert!(
                folded.dedup.contains_key(rid),
                "seed {seed} success {rid} missing from refold"
            );
        }
    }
    drop(store);
    let v = verify(&dir);
    assert!(
        matches!(v.health, fsm_cli::journal_io::JournalHealth::Ok),
        "seed {seed} health {:?}",
        v.health
    );
    (kinds, log)
}

#[test]
fn seed_replay_stable() {
    let (ka, la) = run_seed(42);
    let (kb, lb) = run_seed(42);
    assert_eq!(ka, kb);
    assert_eq!(la, lb);
}
