//! Seeded machine generator. Consumed via `#[path = "proputil.rs"] mod proputil;`.
//! Duplication with the chaos suite is deliberate — test crates cannot share helpers.

use fsm_core::json::Value;
use fsm_core::spec::{compile, parse_machine};
use std::collections::BTreeMap;

pub struct Gen(pub u64);

impl Gen {
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next_u64() as u32) % (hi - lo + 1)
    }
    #[allow(dead_code)]
    pub fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.range(0, xs.len() as u32 - 1) as usize]
    }
}

pub fn gen_machine(g: &mut Gen, _size: u32) -> Value {
    let n = g.range(2, 6) as usize;
    let mut states = Vec::new();
    for i in 0..n {
        let name = format!("s{i}");
        if i == n - 1 {
            states.push(obj(&[
                ("name", Value::Str(name)),
                ("terminal", Value::Bool(true)),
            ]));
        } else {
            states.push(obj(&[("name", Value::Str(name))]));
        }
    }
    let has_compound = g.range(0, 2) != 0 && n >= 3;
    if has_compound {
        let child = obj(&[("name", Value::Str("inner".into()))]);
        let mut comp = BTreeMap::new();
        comp.insert("name".into(), Value::Str("comp".into()));
        comp.insert("initial".into(), Value::Str("inner".into()));
        let mut kids = vec![child];
        if g.range(0, 1) == 0 {
            kids.push(obj(&[
                ("name", Value::Str("hist".into())),
                ("history", Value::Str("deep".into())),
            ]));
        }
        comp.insert("states".into(), Value::Arr(kids));
        states.insert(1, Value::Obj(comp));
    }
    let evs = ["go", "tick", "stop"];
    let count = g.range(1, 3) as usize;
    let mut events = Vec::new();
    for e in evs.iter().take(count) {
        events.push(obj(&[
            ("name", Value::Str((*e).into())),
            ("fields", Value::Arr(vec![])),
        ]));
    }
    let mut transitions = Vec::new();
    transitions.push(obj(&[
        ("from", Value::Str("s0".into())),
        ("on", Value::Str("go".into())),
        ("to", Value::Str(format!("s{}", (n - 1).min(1)))),
    ]));
    if count >= 2 && g.range(0, 3) != 0 {
        transitions.push(obj(&[
            ("from", Value::Str("s0".into())),
            ("on", Value::Str("tick".into())),
            ("if", Value::Str("ctx.flag".into())),
            ("to", Value::Str("s0".into())),
        ]));
    } else if g.range(0, 4) != 0 {
        transitions.push(obj(&[
            ("from", Value::Str("s0".into())),
            ("on", Value::Str("go".into())),
            ("if", Value::Str("ctx.flag".into())),
            ("to", Value::Str("s0".into())),
        ]));
    }
    let mut ctx = vec![
        obj(&[
            ("name", Value::Str("flag".into())),
            ("ty", Value::Str("bool".into())),
            ("init", Value::Str("true".into())),
        ]),
        obj(&[
            ("name", Value::Str("count".into())),
            ("ty", Value::Str("int".into())),
            ("init", Value::Str("0".into())),
        ]),
    ];
    if g.range(0, 2) == 0 {
        ctx.push(obj(&[
            ("name", Value::Str("total".into())),
            (
                "ty",
                Value::Obj(BTreeMap::from([("decimal".into(), Value::Str("2".into()))])),
            ),
            ("init", Value::Str("0.00".into())),
        ]));
    }
    let mut inv = Vec::new();
    if g.range(0, 2) != 2 {
        inv.push(obj(&[
            ("name", Value::Str("ok".into())),
            ("expr", Value::Str("ctx.count >= 0".into())),
            ("mode", Value::Str("enforce".into())),
        ]));
    }
    let mut m = BTreeMap::new();
    m.insert("format".into(), Value::Str("fsm.machine/1".into()));
    m.insert("name".into(), Value::Str(format!("g{}", g.0 % 10_000)));
    m.insert("context".into(), Value::Arr(ctx));
    m.insert("events".into(), Value::Arr(events));
    m.insert("states".into(), Value::Arr(states));
    m.insert("initial".into(), Value::Str("s0".into()));
    m.insert("transitions".into(), Value::Arr(transitions));
    if !inv.is_empty() {
        m.insert("invariants".into(), Value::Arr(inv));
    }
    Value::Obj(m)
}

pub fn gen_events(g: &mut Gen, machine: &Value, len: u32) -> Vec<Value> {
    let evs = machine
        .get("events")
        .and_then(Value::as_arr)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("name").and_then(Value::as_str).map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for _ in 0..len {
        let name = if evs.is_empty() {
            "go".into()
        } else {
            evs[g.range(0, evs.len() as u32 - 1) as usize].clone()
        };
        let wrong = g.range(0, 9) == 0;
        let evname = if wrong { "nope".into() } else { name };
        let mut o = BTreeMap::new();
        o.insert("name".into(), Value::Str(evname));
        o.insert("wrong".into(), Value::Bool(wrong));
        out.push(Value::Obj(o));
    }
    out
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}

#[test]
fn generator_sanity() {
    let mut compound = 0;
    let mut history = 0;
    let mut guarded = 0;
    let mut invariant = 0;
    for seed in 1u64..=100 {
        let mut g = Gen(seed);
        let m = gen_machine(&mut g, 4);
        let spec = parse_machine(&m).unwrap_or_else(|e| panic!("seed {seed} {e:?}"));
        compile(spec).unwrap_or_else(|e| panic!("seed {seed} {e:?}"));
        let evs = gen_events(&mut g, &m, 5);
        for e in &evs {
            let n = e.get("name").and_then(Value::as_str).unwrap();
            let wrong = e.get("wrong").and_then(Value::as_bool).unwrap();
            let declared = m
                .get("events")
                .and_then(Value::as_arr)
                .unwrap()
                .iter()
                .any(|x| x.get("name").and_then(Value::as_str) == Some(n));
            if wrong {
                assert!(!declared, "seed {seed} tag lied");
            } else {
                assert!(declared, "seed {seed} untagged not declared");
            }
        }
        if format!("{m:?}").contains("comp") {
            compound += 1;
        }
        if format!("{m:?}").contains("history") {
            history += 1;
        }
        if format!("{m:?}").contains("if") {
            guarded += 1;
        }
        if format!("{m:?}").contains("invariants") {
            invariant += 1;
        }
        let mut g2 = Gen(seed);
        assert_eq!(
            gen_machine(&mut g2, 4),
            gen_machine(&mut Gen(seed), 4),
            "seed {seed}"
        );
    }
    assert!(compound >= 30, "{compound}");
    assert!(history >= 15, "{history}");
    assert!(guarded >= 50, "{guarded}");
    assert!(invariant >= 20, "{invariant}");
}
