//! Isolated fuzz: never touches the default store; fixed clock; hash recompute.

use fsm_cli::clock;
use fsm_cli::journal_io::load_records;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::Record;

fn tmp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-fuzz-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn fuzz_inputs_use_temp_store_and_fixed_clock() {
    clock::reset_injected();
    clock::force_ms(9_000);
    clock::set_step(1);
    let dir = tmp();
    assert!(!dir.starts_with(fsm_cli::args::default_data_dir()));
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut seed = 0xC0FFEE_u64;
    for i in 0..32 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let ev = if seed & 1 == 0 {
            "docs_ok"
        } else {
            "note_added"
        };
        let rid = format!("f{i}");
        if i == 0 {
            let _ = store.create_instance("case_review", "fz", "fc", None);
        }
        let _ = store.send_event("fz", ev, Value::Obj(Default::default()), &rid, None);
    }
    let recs = load_records(&dir).unwrap();
    for rec in &recs {
        assert!(recompute_hash(rec), "hash mismatch {}", rec.seq);
        if rec.kind == fsm_core::record::RecordKind::EventRejected {
            if let Some(code) = rec.body.get("code").and_then(Value::as_str) {
                assert!(!code.is_empty());
            }
        }
    }
    for rec in recs.windows(2) {
        assert_eq!(rec[1].prev, rec[0].hash);
    }
}

fn recompute_hash(rec: &Record) -> bool {
    let sealed = fsm_core::record::seal(rec.seq, rec.ts, rec.kind, rec.body.clone(), &rec.prev);
    sealed.hash == rec.hash
}
