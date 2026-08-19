//! Seeded chaos suite. The ~30-line xorshift64* is duplicated with proputil on purpose.

use fsm_cli::journal_io::verify;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::{NopSink, fold_with};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(seed: u64) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-cli-chaos-{}-{seed}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
    let directory = TestDirectory::create(seed);
    let mut g = Gen(seed);
    let mut store =
        Store::open(directory.path()).unwrap_or_else(|e| panic!("seed {seed} open {e:?}"));
    let mut kinds = BTreeMap::new();
    let mut log = Vec::new();
    let n = g.range(30, 80);
    let mut iid = 1u32;
    #[derive(Clone, Debug)]
    struct MutTuple {
        kind: &'static str,
        instance: Option<String>,
        event: Option<String>,
        payload: Option<Value>,
        request_id: String,
        expect_seq: Option<u64>,
        effect_id: Option<String>,
        ok: bool,
    }
    let mut tuples: Vec<MutTuple> = Vec::new();
    fn note(tuples: &mut Vec<MutTuple>, t: MutTuple) {
        tuples.push(t);
    }
    let defined = push_result(
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
    note(
        &mut tuples,
        MutTuple {
            kind: "define",
            instance: None,
            event: None,
            payload: Some(case()),
            request_id: String::new(),
            expect_seq: None,
            effect_id: None,
            ok: defined,
        },
    );
    let tid = format!("i{iid}");
    let crid = format!("c{iid}");
    let cok = push_result(
        &mut log,
        "create",
        &mut kinds,
        store.create_instance("case_review", &tid, &crid, None),
    );
    note(
        &mut tuples,
        MutTuple {
            kind: "create",
            instance: Some(tid.clone()),
            event: None,
            payload: None,
            request_id: crid,
            expect_seq: None,
            effect_id: None,
            ok: cok,
        },
    );
    let first_send = store.send_event(
        &tid,
        "docs_ok",
        Value::Obj(BTreeMap::new()),
        "typed-d1",
        None,
    );
    let first_send_response = first_send.as_ref().ok().cloned();
    let sok = push_result(&mut log, "send", &mut kinds, first_send);
    note(
        &mut tuples,
        MutTuple {
            kind: "send",
            instance: Some(tid.clone()),
            event: Some("docs_ok".into()),
            payload: Some(Value::Obj(BTreeMap::new())),
            request_id: "typed-d1".into(),
            expect_seq: None,
            effect_id: None,
            ok: sok,
        },
    );
    let mut last_rid = String::from("typed-d1");
    #[derive(Clone, Debug)]
    struct SendTuple {
        id: String,
        event: String,
        payload: Value,
        rid: String,
        expect_seq: Option<u64>,
    }
    let mut last_ok_send = if sok {
        Some(SendTuple {
            id: tid.clone(),
            event: "docs_ok".into(),
            payload: Value::Obj(BTreeMap::new()),
            rid: last_rid.clone(),
            expect_seq: None,
        })
    } else {
        None
    };
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
    note(
        &mut tuples,
        MutTuple {
            kind: "typed_err",
            instance: Some(tid.clone()),
            event: Some("scored".into()),
            payload: Some(Value::Obj(BTreeMap::from([(
                "score".into(),
                Value::Bool(true),
            )]))),
            request_id: "typed-bad".into(),
            expect_seq: None,
            effect_id: None,
            ok: false,
        },
    );
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
    note(
        &mut tuples,
        MutTuple {
            kind: "stale",
            instance: Some(tid.clone()),
            event: Some("docs_ok".into()),
            payload: Some(Value::Obj(BTreeMap::new())),
            request_id: "stale-1".into(),
            expect_seq: Some(0),
            effect_id: None,
            ok: false,
        },
    );
    let unk = store.ack_effect(&tid, "none", "ack-unknown");
    assert_eq!(
        unk.as_ref().err().map(|e| e.code.as_str()),
        Some("req/field_unknown")
    );
    push_result(&mut log, "unknown_ack", &mut kinds, unk);
    note(
        &mut tuples,
        MutTuple {
            kind: "unknown_ack",
            instance: Some(tid.clone()),
            event: None,
            payload: None,
            request_id: "ack-unknown".into(),
            expect_seq: None,
            effect_id: Some("none".into()),
            ok: false,
        },
    );
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
    if let (Some(mut expected), Ok(actual)) = (first_send_response, &dup0) {
        if let Value::Obj(o) = &mut expected {
            o.insert("duplicate".into(), Value::Bool(true));
        }
        assert_eq!(
            actual, &expected,
            "exact duplicate must preserve full outcome"
        );
    }
    let dup_ok = push_result(&mut log, "dup", &mut kinds, dup0);
    note(
        &mut tuples,
        MutTuple {
            kind: "dup",
            instance: Some(tid.clone()),
            event: Some("docs_ok".into()),
            payload: Some(Value::Obj(BTreeMap::new())),
            request_id: "typed-d1".into(),
            expect_seq: None,
            effect_id: None,
            ok: dup_ok,
        },
    );
    let exp_live = Some(store.journal.last_seq);
    let eok = push_result(
        &mut log,
        "expect_seq",
        &mut kinds,
        store.send_event(
            &tid,
            "note_added",
            Value::Obj(BTreeMap::from([("text".into(), Value::Str("n".into()))])),
            "expect-live",
            exp_live,
        ),
    );
    note(
        &mut tuples,
        MutTuple {
            kind: "expect_seq",
            instance: Some(tid.clone()),
            event: Some("note_added".into()),
            payload: Some(Value::Obj(BTreeMap::from([(
                "text".into(),
                Value::Str("n".into()),
            )]))),
            request_id: "expect-live".into(),
            expect_seq: exp_live,
            effect_id: None,
            ok: eok,
        },
    );
    let s2 = push_result(
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
    note(
        &mut tuples,
        MutTuple {
            kind: "send",
            instance: Some(tid.clone()),
            event: Some("docs_ok".into()),
            payload: Some(Value::Obj(BTreeMap::new())),
            request_id: "typed-d2".into(),
            expect_seq: None,
            effect_id: None,
            ok: s2,
        },
    );
    let tok = push_result(
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
    note(
        &mut tuples,
        MutTuple {
            kind: "typed",
            instance: Some(tid.clone()),
            event: Some("scored".into()),
            payload: Some(Value::Obj(BTreeMap::from([(
                "score".into(),
                Value::Str("800".into()),
            )]))),
            request_id: "typed-ok".into(),
            expect_seq: None,
            effect_id: None,
            ok: tok,
        },
    );
    for _ in 0..n {
        match g.range(0, 10) {
            0 => {
                let ok = push_result(
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
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "define",
                        instance: None,
                        event: None,
                        payload: Some(case()),
                        request_id: String::new(),
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
                );
            }
            1 => {
                iid += 1;
                let id = format!("i{iid}");
                let rid = format!("c{iid}");
                let ok = push_result(
                    &mut log,
                    "create",
                    &mut kinds,
                    store.create_instance("case_review", &id, &rid, None),
                );
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "create",
                        instance: Some(id),
                        event: None,
                        payload: None,
                        request_id: rid,
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
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
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "send",
                        instance: Some(id.clone()),
                        event: Some(ev.into()),
                        payload: Some(Value::Obj(BTreeMap::new())),
                        request_id: last_rid.clone(),
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
                );
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
                    let rid = format!("a{}", g.next());
                    let ok = push_result(
                        &mut log,
                        "ack_ok",
                        &mut kinds,
                        store.ack_effect(&id, &eid, &rid),
                    );
                    if ok {
                        *kinds.entry("ack").or_insert(0) += 1;
                    }
                    note(
                        &mut tuples,
                        MutTuple {
                            kind: "ack",
                            instance: Some(id),
                            event: None,
                            payload: None,
                            request_id: rid,
                            expect_seq: None,
                            effect_id: Some(eid),
                            ok,
                        },
                    );
                } else {
                    let rid = format!("a{}", g.next());
                    let ok = push_result(
                        &mut log,
                        "ack",
                        &mut kinds,
                        store.ack_effect(&id, "none", &rid),
                    );
                    note(
                        &mut tuples,
                        MutTuple {
                            kind: "ack",
                            instance: Some(id),
                            event: None,
                            payload: None,
                            request_id: rid,
                            expect_seq: None,
                            effect_id: Some("none".into()),
                            ok,
                        },
                    );
                }
            }
            4 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let rid = format!("k{}", g.next());
                let ok = push_result(
                    &mut log,
                    "cancel",
                    &mut kinds,
                    store.cancel_instance(&id, &rid),
                );
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "cancel",
                        instance: Some(id),
                        event: None,
                        payload: None,
                        request_id: rid,
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
                );
            }
            5 => {
                drop(store);
                store = Store::open(directory.path())
                    .unwrap_or_else(|e| panic!("seed {seed} reopen {e:?}"));
                *kinds.entry("reopen").or_insert(0) += 1;
            }
            6 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                last_rid = format!("t{}", g.next());
                let payload = Value::Obj(BTreeMap::from([("score".into(), Value::Bool(true))]));
                let ok = push_result(
                    &mut log,
                    "typed",
                    &mut kinds,
                    store.send_event(&id, "scored", payload.clone(), &last_rid, None),
                );
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "typed_random",
                        instance: Some(id.clone()),
                        event: Some("scored".into()),
                        payload: Some(payload.clone()),
                        request_id: last_rid.clone(),
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
                );
                if ok {
                    last_ok_send = Some(SendTuple {
                        id,
                        event: "scored".into(),
                        payload,
                        rid: last_rid.clone(),
                        expect_seq: None,
                    });
                }
            }
            7 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let payload = Value::Obj(BTreeMap::new());
                let ok = push_result(
                    &mut log,
                    "dup",
                    &mut kinds,
                    store.send_event(&id, "docs_ok", payload.clone(), &last_rid, None),
                );
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "dup_random",
                        instance: Some(id.clone()),
                        event: Some("docs_ok".into()),
                        payload: Some(payload.clone()),
                        request_id: last_rid.clone(),
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
                );
                if ok {
                    last_ok_send = Some(SendTuple {
                        id,
                        event: "docs_ok".into(),
                        payload,
                        rid: last_rid.clone(),
                        expect_seq: None,
                    });
                }
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
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "expect_seq",
                        instance: Some(id.clone()),
                        event: Some("docs_ok".into()),
                        payload: Some(Value::Obj(BTreeMap::new())),
                        request_id: last_rid.clone(),
                        expect_seq: exp,
                        effect_id: None,
                        ok,
                    },
                );
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
                let bad = parse(br#"{"format":"fsm.machine/1"}"#, &JsonLimits::DEFAULT).unwrap();
                let ok = push_result(
                    &mut log,
                    "define",
                    &mut kinds,
                    store.define_machine(bad.clone(), false, false).map(|o| {
                        Value::Obj(BTreeMap::from([(
                            "machine_id".into(),
                            Value::Str(o.machine_id),
                        )]))
                    }),
                );
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "define_invalid",
                        instance: None,
                        event: None,
                        payload: Some(bad),
                        request_id: String::new(),
                        expect_seq: None,
                        effect_id: None,
                        ok,
                    },
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
                let rid = format!("forced-{}", g.next());
                let ok = push_result(
                    &mut log,
                    "ack_ok",
                    &mut kinds,
                    store.ack_effect(&id, &eid, &rid),
                );
                assert!(ok, "forced pending ack failed");
                *kinds.entry("ack").or_insert(0) += 1;
                note(
                    &mut tuples,
                    MutTuple {
                        kind: "ack",
                        instance: Some(id),
                        event: None,
                        payload: None,
                        request_id: rid,
                        expect_seq: None,
                        effect_id: Some(eid),
                        ok,
                    },
                );
                break;
            }
        }
    }
    let recs = fsm_cli::journal_io::load_records(directory.path()).unwrap();
    let folded = fold_with(recs, &mut NopSink).unwrap_or_else(|e| panic!("seed {seed} fold {e:?}"));
    assert!(
        fsm_cli::snapshot::store_states_eq(&store.state, &folded),
        "seed {seed} complete StoreState differs after refold"
    );
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
        assert_eq!(
            st.configuration, o.configuration,
            "seed {seed} {id} configuration"
        );
        assert_eq!(st.status, o.status, "seed {seed} {id} status");
        assert_eq!(st.ctx, o.ctx, "seed {seed} {id} ctx");
        assert_eq!(st.history, o.history, "seed {seed} {id} hist");
        assert_eq!(st.deadlines, o.deadlines, "seed {seed} {id} deadlines");
        assert_eq!(st.pending, o.pending, "seed {seed} {id} pend");
    }
    assert!(
        tuples.iter().any(|t| t.kind == "typed_err" && !t.ok),
        "seed {seed} missing typed_err tuple"
    );
    assert!(
        tuples
            .iter()
            .any(|t| t.kind == "stale" && t.expect_seq == Some(0) && !t.ok),
        "seed {seed} missing stale tuple"
    );
    assert!(
        tuples
            .iter()
            .any(|t| t.kind == "unknown_ack" && t.effect_id.as_deref() == Some("none") && !t.ok),
        "seed {seed} missing unknown_ack tuple"
    );
    assert!(
        tuples.iter().any(|t| t.kind == "dup" && t.ok),
        "seed {seed} missing duplicate tuple"
    );
    assert!(
        tuples
            .iter()
            .any(|t| { t.instance.is_some() && t.event.is_some() && t.payload.is_some() && t.ok }),
        "seed {seed} missing complete successful send tuple"
    );
    let success_ids: Vec<String> = tuples
        .iter()
        .filter(|t| t.ok && !t.request_id.is_empty())
        .map(|t| t.request_id.clone())
        .collect();
    assert!(
        !success_ids.is_empty(),
        "seed {seed} no successful mutations"
    );
    for rid in &success_ids {
        assert!(
            folded.dedup.contains_key(rid),
            "seed {seed} success {rid} missing from refold; tuples={tuples:?}"
        );
    }
    drop(store);
    let v = verify(directory.path());
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
