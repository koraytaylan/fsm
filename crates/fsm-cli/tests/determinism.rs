//! Three-way refold identity. Instant is allowed in tests (not in fsm-core/src).

#[path = "../../fsm-core/tests/proputil.rs"]
mod proputil;

use fsm_cli::clock::FixedClock;
use fsm_cli::journal_io::{load_records, verify};
use fsm_cli::snapshot::{open_state, store_states_eq};
use fsm_cli::store::Store;
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
        let mut clock = FixedClock::new(1_000 + seed as i64 * 100, 1);
        let dir = tmp(seed);
        let mut g = proputil::Gen(seed);
        let m = proputil::gen_machine(&mut g, 4);
        let evs = proputil::gen_events(&mut g, &m, 8);
        let mut store = Store::open(&dir).unwrap();
        store
            .define_machine_on(&mut clock, m, false, false)
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
            .create_instance_ctx_on(&mut clock, &name, "i", "c", None, &BTreeMap::new(), &[])
            .unwrap_or_else(|e| panic!("seed {seed} create {e:?}"));
        let split = if seed % 2 == 0 {
            evs.len() / 2
        } else {
            evs.len()
        };
        for (i, ev) in evs.iter().take(split).enumerate() {
            let n = ev.get("name").and_then(Value::as_str).unwrap_or("go");
            let mut payload = Value::Obj(BTreeMap::new());
            let _ = store.send_event_stamp_on(
                &mut clock,
                "i",
                n,
                &mut payload,
                &format!("e{i}"),
                None,
                &[],
            );
        }
        let mut mid_seq = None;
        if seed % 2 == 0 {
            let before_checkpoint = store.state.last_seq;
            store
                .shutdown_snapshot_on(&mut clock)
                .unwrap_or_else(|e| panic!("seed {seed} snapshot {e:?}"));
            let seq = store.state.last_seq;
            assert!(
                seq > before_checkpoint,
                "seed {seed}: checkpoint not committed"
            );
            assert!(
                fsm_cli::snapshot::listed_snaps(&dir)
                    .iter()
                    .any(|(snap_seq, _)| *snap_seq == seq),
                "seed {seed}: snapshot was not written at seq {seq}"
            );
            mid_seq = Some(seq);
            snaps += 1;

            // A declared event always reaches the durable outcome path, even
            // when the generated instance is already terminal.
            let mut payload = Value::Obj(BTreeMap::new());
            let _ = store.send_event_stamp_on(
                &mut clock,
                "i",
                "go",
                &mut payload,
                "snapshot-tail",
                None,
                &[],
            );
            assert!(
                store.state.last_seq > seq,
                "seed {seed}: snapshot must have a nonempty journal tail"
            );
        }
        for (i, ev) in evs.iter().enumerate().skip(split) {
            let n = ev.get("name").and_then(Value::as_str).unwrap_or("go");
            let mut payload = Value::Obj(BTreeMap::new());
            let _ = store.send_event_stamp_on(
                &mut clock,
                "i",
                n,
                &mut payload,
                &format!("e{i}"),
                None,
                &[],
            );
        }
        let live = store.state.clone();
        if let Some(seq) = mid_seq {
            let (snap_tail, path) = open_state(&dir, store.records.clone(), &mut NopSink)
                .unwrap_or_else(|e| panic!("seed {seed} snapshot+tail {e:?}"));
            assert!(
                path.used_snapshot,
                "seed {seed}: snapshot path not selected"
            );
            assert_eq!(path.snapshot_seq, Some(seq), "seed {seed}");
            assert!(path.replayed_records > 0, "seed {seed}: empty tail");
            assert!(store_states_eq(&snap_tail, &live), "seed {seed} snapshot");
        }
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
        assert!(store_states_eq(&live, &folded), "seed {seed} fold");
        assert!(store_states_eq(&live, &store2.state), "seed {seed} reopen");
    }
    assert!(snaps >= 25, "{snaps}");
    assert!(saw_reject);
}

#[test]
fn generator_twice_byte_identical() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("tools/gen_decimal_vectors.py");
    let committed = root.join("crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl");
    let original = std::fs::read(&committed).expect("snapshot committed fixture first");
    let a = std::env::temp_dir().join(format!("dec-a-{}.jsonl", std::process::id()));
    let b = std::env::temp_dir().join(format!("dec-b-{}.jsonl", std::process::id()));
    for dest in [&a, &b] {
        let out = std::process::Command::new("python3")
            .arg(&script)
            .arg(dest)
            .output()
            .expect("python3 tools/gen_decimal_vectors.py <dest>");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let still = std::fs::read(&committed).unwrap();
    assert_eq!(
        still, original,
        "generator must not overwrite the committed fixture"
    );
    let ba = std::fs::read(&a).unwrap();
    let bb = std::fs::read(&b).unwrap();
    assert_eq!(ba, original);
    assert_eq!(bb, original);
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
}

#[test]
fn perf_smoke() {
    // tmp seeds are a process-wide namespace shared with three_way_refold's
    // 1..=50 loop; this seed must stay outside that range.
    let dir = tmp(1_012);
    let mut clock = FixedClock::new(12_000, 1);
    let mut store = Store::open(&dir).unwrap();
    let spec = legal_limit_spec();
    assert_eq!(
        fsm_core::canon::canon_bytes(&spec).len(),
        fsm_core::limits::MAX_DEF_BYTES,
        "fixture must be the largest legal definition"
    );
    store
        .define_machine_on(&mut clock, spec, false, false)
        .unwrap();
    let stored = store.state.machines.values().next().unwrap();
    assert_eq!(
        count_nodes(&stored.compiled.spec.states),
        fsm_core::limits::MAX_STATES
    );
    assert_eq!(
        stored.compiled.spec.events.len(),
        fsm_core::limits::MAX_EVENTS
    );
    assert_eq!(
        stored.compiled.spec.context.len(),
        fsm_core::limits::MAX_CTX_VARS
    );
    assert_eq!(
        stored.compiled.spec.invariants.len(),
        fsm_core::limits::MAX_INVARIANTS
    );
    assert_eq!(
        stored.compiled.spec.invariants[0].expr.len(),
        fsm_core::expr::lexer::SOURCE_CAP
    );
    assert_full_spine(&stored.compiled.spec.states[0]);
    assert_full_spine(&stored.compiled.spec.states[1]);
    for transition in &stored.compiled.spec.transitions {
        assert_eq!(transition.sets.len(), fsm_core::limits::MAX_SETS_PER_BLOCK);
        assert_eq!(
            transition.emits.len(),
            fsm_core::limits::MAX_EMITS_PER_BLOCK
        );
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            "limitperf",
            "deep",
            "c",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    assert_eq!(store.state.instances.get("deep").unwrap().leaf, "a11");
    let mut times = Vec::new();
    for i in 0..10 {
        if i == 5 {
            store.shutdown_snapshot_on(&mut clock).unwrap();
            assert!(
                std::fs::read_dir(dir.join("snapshots"))
                    .unwrap()
                    .next()
                    .is_some()
            );
        }
        let t = Instant::now();
        let mut payload = Value::Obj(BTreeMap::new());
        let r = store
            .send_event_stamp_on(
                &mut clock,
                "deep",
                "go",
                &mut payload,
                &format!("cross{i}"),
                None,
                &[],
            )
            .unwrap();
        times.push(t.elapsed());
        assert_eq!(
            r.get("applied").and_then(Value::as_bool),
            Some(true),
            "{r:?}"
        );
    }
    let mean = times.iter().sum::<std::time::Duration>() / times.len() as u32;
    assert!(mean.as_millis() < 250, "limit mean {}", mean.as_millis());
    let mid_seq = {
        let snaps = fsm_cli::snapshot::listed_snaps(&dir);
        assert!(!snaps.is_empty(), "midstream snapshot written");
        snaps[0].0
    };
    let mut payload = Value::Obj(BTreeMap::new());
    store
        .send_event_stamp_on(&mut clock, "deep", "go", &mut payload, "tail", None, &[])
        .unwrap();
    let recs = store.records.clone();
    let last = recs.last().unwrap().seq;
    assert!(last > mid_seq, "nonempty tail after mid-stream snapshot");
    let (live, path) = open_state(&dir, recs.clone(), &mut NopSink).unwrap();
    assert!(path.used_snapshot, "snapshot fast path must be selected");
    assert_eq!(path.snapshot_seq, Some(mid_seq));
    assert!(
        path.replayed_records > 0,
        "snapshot must have a nonempty tail"
    );
    let folded = fsm_core::replay::fold_with(recs.clone(), &mut fsm_core::replay::NopSink).unwrap();
    assert!(
        fsm_cli::snapshot::store_states_eq(&live, &folded),
        "complete StoreState mismatch live vs fold"
    );
    drop(store);
}

fn vobj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}

fn inc(target: &str) -> Value {
    vobj(&[
        ("target", Value::Str(target.into())),
        ("value", Value::Str(format!("ctx.{target} + 1"))),
    ])
}

fn block_do(sets: Vec<Value>, emits: Vec<Value>) -> Value {
    let mut o = BTreeMap::new();
    o.insert("do".into(), Value::Arr(sets));
    if !emits.is_empty() {
        o.insert("emit".into(), Value::Arr(emits));
    }
    Value::Obj(o)
}

fn full_sets() -> Vec<Value> {
    (0..fsm_core::limits::MAX_SETS_PER_BLOCK)
        .map(|i| inc(&format!("n{i}")))
        .collect()
}

fn full_emits() -> Vec<Value> {
    (0..fsm_core::limits::MAX_EMITS_PER_BLOCK)
        .map(|_| {
            vobj(&[
                ("effect", Value::Str("tick".into())),
                ("args", vobj(&[("k", Value::Str("ctx.n0".into()))])),
            ])
        })
        .collect()
}

fn spine(prefix: char, depth: usize) -> Value {
    let last = depth - 1;
    let mut node = vobj(&[
        ("name", Value::Str(format!("{prefix}{last}"))),
        ("entry", block_do(full_sets(), full_emits())),
        ("exit", block_do(full_sets(), full_emits())),
    ]);
    for i in (0..last).rev() {
        node = vobj(&[
            ("name", Value::Str(format!("{prefix}{i}"))),
            ("initial", Value::Str(format!("{prefix}{}", i + 1))),
            ("entry", block_do(full_sets(), full_emits())),
            ("exit", block_do(full_sets(), full_emits())),
            ("states", Value::Arr(vec![node])),
        ]);
    }
    node
}

fn assert_full_spine(node: &fsm_core::spec::StateNode) {
    let entry = node.entry.as_ref().expect("entry block");
    let exit = node.exit.as_ref().expect("exit block");
    assert_eq!(entry.sets.len(), fsm_core::limits::MAX_SETS_PER_BLOCK);
    assert_eq!(entry.emits.len(), fsm_core::limits::MAX_EMITS_PER_BLOCK);
    assert_eq!(exit.sets.len(), fsm_core::limits::MAX_SETS_PER_BLOCK);
    assert_eq!(exit.emits.len(), fsm_core::limits::MAX_EMITS_PER_BLOCK);
    if let Some(child) = node.states.first() {
        assert_full_spine(child);
    }
}

fn count_nodes(nodes: &[fsm_core::spec::StateNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.states)).sum()
}

fn legal_limit_spec() -> Value {
    use fsm_core::limits::{MAX_CTX_VARS, MAX_EVENTS, MAX_INVARIANTS, MAX_NESTING, MAX_STATES};
    let depth = MAX_NESTING as usize;
    let a = spine('a', depth);
    let b = spine('b', depth);
    let used = 2 * depth;
    let mut states = vec![a, b];
    for i in 0..(MAX_STATES - used) {
        states.push(vobj(&[("name", Value::Str(format!("p{i}")))]));
    }
    let mut events = vec![vobj(&[
        ("name", Value::Str("go".into())),
        ("fields", Value::Arr(vec![])),
    ])];
    for i in 1..MAX_EVENTS {
        events.push(vobj(&[
            ("name", Value::Str(format!("e{i}"))),
            ("fields", Value::Arr(vec![])),
        ]));
    }
    let context: Vec<Value> = (0..MAX_CTX_VARS)
        .map(|i| {
            vobj(&[
                ("name", Value::Str(format!("n{i}"))),
                ("ty", Value::Str("int".into())),
                ("init", Value::Str("0".into())),
            ])
        })
        .collect();
    let max_expr = format!(
        "{}ctx.n0 >= 0",
        " ".repeat(fsm_core::expr::lexer::SOURCE_CAP - "ctx.n0 >= 0".len())
    );
    let invariants: Vec<Value> = (0..MAX_INVARIANTS)
        .map(|i| {
            vobj(&[
                ("name", Value::Str(format!("i{i}"))),
                (
                    "expr",
                    Value::Str(if i == 0 {
                        max_expr.clone()
                    } else {
                        format!("ctx.n{i} >= 0")
                    }),
                ),
                ("mode", Value::Str("monitor".into())),
            ])
        })
        .collect();
    let transitions = vec![
        vobj(&[
            ("from", Value::Str("a11".into())),
            ("on", Value::Str("go".into())),
            ("to", Value::Str("b11".into())),
            ("if", Value::Str("ctx.n0 >= 0".into())),
            ("do", Value::Arr(full_sets())),
            ("emit", Value::Arr(full_emits())),
        ]),
        vobj(&[
            ("from", Value::Str("b11".into())),
            ("on", Value::Str("go".into())),
            ("to", Value::Str("a11".into())),
            ("if", Value::Str("ctx.n0 >= 0".into())),
            ("do", Value::Arr(full_sets())),
            ("emit", Value::Arr(full_emits())),
        ]),
    ];
    let mut spec = vobj(&[
        ("format", Value::Str("fsm.machine/1".into())),
        ("name", Value::Str("limitperf".into())),
        ("states", Value::Arr(states)),
        ("initial", Value::Str("a0".into())),
        ("context", Value::Arr(context)),
        ("events", Value::Arr(events)),
        (
            "effects",
            Value::Arr(vec![vobj(&[
                ("name", Value::Str("tick".into())),
                (
                    "fields",
                    Value::Arr(vec![vobj(&[
                        ("name", Value::Str("k".into())),
                        ("ty", Value::Str("int".into())),
                    ])]),
                ),
            ])]),
        ),
        ("transitions", Value::Arr(transitions)),
        ("invariants", Value::Arr(invariants)),
    ]);
    if let Value::Obj(obj) = &mut spec {
        obj.insert("description".into(), Value::Str(String::new()));
    }
    let base = fsm_core::canon::canon_bytes(&spec).len();
    assert!(base <= fsm_core::limits::MAX_DEF_BYTES, "fixture too large");
    if let Value::Obj(obj) = &mut spec {
        obj.insert(
            "description".into(),
            Value::Str("x".repeat(fsm_core::limits::MAX_DEF_BYTES - base)),
        );
    }
    spec
}
