//! Three-way refold identity. Instant is allowed in tests (not in fsm-core/src).

#[path = "../../fsm-core/tests/proputil.rs"]
mod proputil;

use fsm_cli::clock;
use fsm_cli::journal_io::{load_records, verify};
use fsm_cli::store::Store;
use fsm_core::hashes::state_hash;
use fsm_core::json::Value;
use fsm_core::replay::{NopSink, fold_with};
use std::collections::BTreeMap;
use std::time::Instant;

fn tmp(seed: u64) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("fsm-det-{}-{seed}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn three_way_refold() {
    let mut snaps = 0;
    let mut saw_reject = false;
    for seed in 1u64..=50 {
        clock::reset_injected();
        clock::force_ms(1_000);
        clock::set_step(1);
        let dir = tmp(seed);
        let mut g = proputil::Gen(seed);
        let m = proputil::gen_machine(&mut g, 4);
        let evs = proputil::gen_events(&mut g, &m, 8);
        let mut store = Store::open(&dir).unwrap();
        store
            .define_machine(m, false, false)
            .unwrap_or_else(|e| panic!("seed {seed} {e:?}"));
        let name = store
            .state
            .machines
            .values()
            .next()
            .unwrap()
            .compiled
            .spec
            .name
            .clone();
        store
            .create_instance(&name, "i", "c", None)
            .unwrap_or_else(|e| panic!("seed {seed} create {e:?}"));
        for (i, ev) in evs.iter().enumerate() {
            let n = ev.get("name").and_then(Value::as_str).unwrap_or("go");
            let _ = store.send_event("i", n, Value::Obj(BTreeMap::new()), &format!("e{i}"), None);
        }
        if seed % 2 == 0 {
            store.shutdown_snapshot().ok();
            snaps += 1;
        }
        let live: BTreeMap<_, _> = store
            .state
            .instances
            .iter()
            .map(|(id, st)| {
                let mid = store
                    .state
                    .instance_machines
                    .get(id)
                    .cloned()
                    .unwrap_or_default();
                (id.clone(), state_hash(&mid, id, store.journal.last_seq, st))
            })
            .collect();
        if store
            .records
            .iter()
            .any(|r| matches!(r.kind, fsm_core::record::RecordKind::EventRejected))
        {
            saw_reject = true;
        }
        drop(store);
        assert!(matches!(
            verify(&dir).health,
            fsm_cli::journal_io::JournalHealth::Ok
        ));
        let recs = load_records(&dir).unwrap();
        let folded = fold_with(recs, &mut NopSink).unwrap_or_else(|e| panic!("seed {seed} {e:?}"));
        let store2 = Store::open(&dir).unwrap();
        for (id, h) in &live {
            let st = folded.instances.get(id).unwrap();
            let mid = folded.instance_machines.get(id).unwrap();
            let hf = state_hash(mid, id, folded.last_seq, st);
            assert_eq!(&hf, h, "seed {seed} fold");
            let st2 = store2.state.instances.get(id).unwrap();
            let mid2 = store2.state.instance_machines.get(id).unwrap();
            let hr = state_hash(mid2, id, store2.journal.last_seq, st2);
            assert_eq!(&hr, h, "seed {seed} reopen");
        }
    }
    assert!(snaps >= 25, "{snaps}");
    assert!(saw_reject);
}

#[test]
fn perf_smoke() {
    clock::reset_injected();
    let dir = tmp(99);
    let mut store = Store::open(&dir).unwrap();
    let spec = parse_case();
    store.define_machine(spec, false, false).unwrap();
    store
        .create_instance("case_review", "i", "c", None)
        .unwrap();
    let mut times = Vec::new();
    for i in 0..10 {
        let t = Instant::now();
        let _ = store.send_event(
            "i",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            &format!("p{i}"),
            None,
        );
        times.push(t.elapsed());
    }
    let mean = times.iter().sum::<std::time::Duration>() / times.len() as u32;
    assert!(mean.as_millis() < 250, "mean {}ms", mean.as_millis());
}

fn parse_case() -> Value {
    fsm_core::json::parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap()
}
