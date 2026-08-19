//! Ordering goldens authored from SPEC §Semantics.

use std::collections::BTreeMap;
use std::fs;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::InstanceState;
use fsm_core::spec::{compile, load_machine_json, parse_machine};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

#[test]
fn scenario_goldens() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scenarios");
    let mut n = 0;
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            panic!("unrecognized {}", path.display());
        }
        n += 1;
        let bytes = fs::read(&path).unwrap();
        let rec = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        let machine = rec.get("machine").and_then(Value::as_str).unwrap();
        let (m, t) = if machine == "case_review" {
            let spec =
                load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
            let m = compile(spec).unwrap();
            let t = Tree::for_machine(&m.spec);
            (m, t)
        } else {
            let src = rec.get("src").and_then(Value::as_str).unwrap();
            let spec =
                parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
            let m = compile(spec).unwrap();
            let t = Tree::for_machine(&m.spec);
            (m, t)
        };
        let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
        if rec.get("check").and_then(Value::as_str) == Some("create") {
            let entered: Vec<_> = c.entered.iter().map(|s| Value::Str(s.clone())).collect();
            let want = rec.get("entered").and_then(Value::as_arr).unwrap();
            assert_eq!(entered, want, "{}", path.display());
            continue;
        }
        let mut st = InstanceState {
            status: c.status_after,
            configuration: c.configuration_after,
            ctx: c.ctx_after,
            history: c.history_after,
            deadlines: c.deadlines_after,
            pending: vec![],
        };
        let events = rec.get("events").and_then(Value::as_arr).unwrap();
        let mut last = None;
        for ev in events {
            let on = ev.get("on").and_then(Value::as_str).unwrap();
            let payload = ev
                .get("payload")
                .cloned()
                .unwrap_or(Value::Obj(BTreeMap::new()));
            let mut b = Budget::new(4096);
            match step(&m, &t, &st, on, &payload, 0, &mut b) {
                Outcome::Applied(a) => {
                    st.configuration = a.configuration_after.clone();
                    st.ctx = a.ctx_after.clone();
                    st.history = a.history_after.clone();
                    st.deadlines = a.deadlines_after.clone();
                    st.status = a.status_after;
                    last = Some(a);
                }
                o => panic!("{} {on} {o:?}", path.display()),
            }
        }
        let a = last.unwrap();
        if let Some(ex) = rec.get("exited").and_then(Value::as_arr) {
            let got: Vec<_> = a.exited.iter().map(|s| Value::Str(s.clone())).collect();
            assert_eq!(got, ex, "exited {}", path.display());
        }
        if let Some(en) = rec.get("entered").and_then(Value::as_arr) {
            let got: Vec<_> = a.entered.iter().map(|s| Value::Str(s.clone())).collect();
            assert_eq!(got, en, "entered {}", path.display());
        }
        if let Some(internal) = rec.get("internal").and_then(Value::as_bool) {
            assert_eq!(a.internal, internal, "{}", path.display());
        }
    }
    assert!(n >= 9, "expected 9 scenario goldens, got {n}");
}
