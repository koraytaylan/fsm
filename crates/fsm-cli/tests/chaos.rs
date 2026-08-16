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
        let k = run_seed(seed);
        for (name, n) in &k {
            *kinds.entry(*name).or_insert(0) += *n;
        }
        if k.get("reopen").copied().unwrap_or(0) > 0 {
            reopens += 1;
        }
    }
    for need in ["define", "create", "send", "ack", "cancel"] {
        assert!(kinds.get(need).copied().unwrap_or(0) > 0, "missing {need}");
    }
    assert!(reopens >= 100, "reopens {reopens}");
}

fn run_seed(seed: u64) -> BTreeMap<&'static str, u32> {
    let dir = tmp(seed);
    let mut g = Gen(seed);
    let mut store = Store::open(&dir).unwrap_or_else(|e| panic!("seed {seed} open {e:?}"));
    let _ = store.define_machine(case(), false, false);
    let mut kinds = BTreeMap::new();
    let n = g.range(30, 80);
    let mut iid = 0u32;
    for _ in 0..n {
        match g.range(0, 6) {
            0 => {
                let _ = store.define_machine(case(), false, false);
                *kinds.entry("define").or_insert(0) += 1;
            }
            1 => {
                iid += 1;
                let id = format!("i{iid}");
                let _ = store.create_instance("case_review", &id, &format!("c{iid}"), None);
                *kinds.entry("create").or_insert(0) += 1;
            }
            2 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let ev = if g.range(0, 3) == 0 {
                    "nope"
                } else {
                    "docs_ok"
                };
                let _ = store.send_event(
                    &id,
                    ev,
                    Value::Obj(BTreeMap::new()),
                    &format!("s{}", g.next()),
                    None,
                );
                *kinds.entry("send").or_insert(0) += 1;
            }
            3 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let _ = store.ack_effect(&id, "none", &format!("a{}", g.next()));
                *kinds.entry("ack").or_insert(0) += 1;
            }
            4 => {
                let id = format!("i{}", g.range(1, iid.max(1)));
                let _ = store.cancel_instance(&id, &format!("k{}", g.next()));
                *kinds.entry("cancel").or_insert(0) += 1;
            }
            5 => {
                drop(store);
                store = Store::open(&dir).unwrap_or_else(|e| panic!("seed {seed} reopen {e:?}"));
                *kinds.entry("reopen").or_insert(0) += 1;
            }
            _ => {
                let _ = store.define_machine(
                    parse(br#"{"format":"fsm.machine/1"}"#, &JsonLimits::DEFAULT).unwrap(),
                    false,
                    false,
                );
                *kinds.entry("define").or_insert(0) += 1;
            }
        }
    }
    drop(store);
    let v = verify(&dir);
    assert!(
        matches!(v.health, fsm_cli::journal_io::JournalHealth::Ok),
        "seed {seed} health {:?}",
        v.health
    );
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    fold_with(recs, &mut NopSink).unwrap_or_else(|e| panic!("seed {seed} fold {e:?}"));
    kinds
}

#[test]
fn seed_replay_stable() {
    let a = format!("{:?}", run_seed(42));
    let b = format!("{:?}", run_seed(42));
    assert_eq!(a, b);
}
