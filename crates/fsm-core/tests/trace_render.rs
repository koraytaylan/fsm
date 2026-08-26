use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{domain_hash, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

#[test]
fn traces_and_hashes() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    let mut b = Budget::new(4096);
    let a = match step(
        &m,
        &t,
        &st,
        "docs_ok",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut b,
    ) {
        Outcome::Applied(a) => a,
        o => panic!("{o:?}"),
    };
    let v = a.trace.to_value();
    let bytes = canon_bytes(&v);
    assert!(!bytes.is_empty());
    assert!(std::str::from_utf8(&bytes).unwrap().contains("candidates"));
    // not_considered grouping exists on scored from risk after two docs_ok
    st.configuration = a.configuration_after;
    st.ctx = a.ctx_after;
    st.history = a.history_after;
    st.deadlines = a.deadlines_after;
    let mut b = Budget::new(4096);
    let a = match step(
        &m,
        &t,
        &st,
        "docs_ok",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut b,
    ) {
        Outcome::Applied(a) => a,
        o => panic!("{o:?}"),
    };
    st.configuration = a.configuration_after;
    st.ctx = a.ctx_after;
    st.deadlines = a.deadlines_after;
    let mut payload = BTreeMap::new();
    payload.insert("score".into(), Value::Str("800".into()));
    let mut b = Budget::new(4096);
    let a = match step(&m, &t, &st, "scored", &Value::Obj(payload), 0, &mut b) {
        Outcome::Applied(a) => a,
        o => panic!("{o:?}"),
    };
    let rendered = std::str::from_utf8(&canon_bytes(&a.trace.to_value()))
        .unwrap()
        .to_string();
    assert!(rendered.contains("not_considered") || rendered.contains("candidates"));

    let mut st1 = InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: "a".into() },
        ctx: BTreeMap::from([("x".into(), Val::Int(1))]),
        history: BTreeMap::from([("c".into(), "l".into())]),
        deadlines: BTreeMap::new(),
        pending: vec!["p1".into()],
    };
    let h1 = state_hash("mid", "iid", 1, &st1);
    st1.configuration = ActiveConfiguration::Sequential { leaf: "b".into() };
    let h2 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h2);
    st1.configuration = ActiveConfiguration::Sequential { leaf: "a".into() };
    st1.ctx.insert("x".into(), Val::Int(2));
    let h3 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h3);
    st1.ctx.insert("x".into(), Val::Int(1));
    st1.history.insert("c".into(), "z".into());
    let h4 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h4);
    st1.history.insert("c".into(), "l".into());
    st1.pending = vec!["p2".into()];
    let h5 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h5);
    st1.pending = vec!["p1".into()];
    let h6 = state_hash("mid", "iid", 2, &st1);
    assert_ne!(h1, h6);
    assert_eq!(
        h1,
        state_hash(
            "mid",
            "iid",
            1,
            &InstanceState {
                status: Status::Running,
                configuration: ActiveConfiguration::Sequential { leaf: "a".into() },
                ctx: BTreeMap::from([("x".into(), Val::Int(1))]),
                history: BTreeMap::from([("c".into(), "l".into())]),
                deadlines: BTreeMap::new(),
                pending: vec!["p1".into()],
            }
        )
    );
    let v = parse(b"{}", &JsonLimits::DEFAULT).unwrap();
    assert_ne!(
        h1,
        format!(
            "sha256:{}",
            fsm_core::sha256::to_hex(&domain_hash("fsm:machine:1", &v))
        )
    );
}

// ---- Plan 0009 task 4701: the trace of a macrostep.

/// `go` raises `ping`, which nothing handles, and lands in a two-step
/// eventless cascade whose guards and blocks give every reaction candidates
/// and a pipeline of its own; the invariant bound decides whether the
/// macrostep applies or is rejected at quiescence.
fn cascade_machine(bound: i64) -> (fsm_core::machine::CompiledMachine, Tree) {
    let src = format!(
        r#"{{"format":"fsm.machine/1","name":"cascade","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}},{{"name":"ping","fields":[],"internal":true}}],"states":[{{"name":"a"}},{{"name":"b"}},{{"name":"c"}},{{"name":"d"}}],"initial":"a","invariants":[{{"name":"bounded","expr":"ctx.x < {bound}"}}],"transitions":[{{"from":"a","on":"go","to":"b","raise":[{{"event":"ping"}}]}},{{"from":"b","if":"ctx.x >= 0","to":"c","do":[{{"target":"x","value":"ctx.x + 1"}}]}},{{"from":"c","if":"ctx.x > 0","to":"d","do":[{{"target":"x","value":"ctx.x + 1"}}]}}]}}"#
    );
    let spec = fsm_core::spec::parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn go(m: &fsm_core::machine::CompiledMachine, t: &Tree) -> Outcome {
    let c = create(m, t, &BTreeMap::new(), 0).unwrap();
    let state = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    let mut budget = Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
    step(
        m,
        t,
        &state,
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    )
}

fn rendered(v: &Value) -> String {
    String::from_utf8(canon_bytes(v)).unwrap()
}

#[test]
fn a_non_reactive_trace_emits_no_reaction_keys() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    let mut budget = Budget::new(4096);
    let Outcome::Applied(a) = step(
        &m,
        &t,
        &state,
        "docs_ok",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) else {
        panic!("docs_ok applies");
    };
    assert!(a.trace.microsteps.is_empty());
    assert!(a.trace.internal_unhandled.is_empty());
    let text = rendered(&a.trace.to_value());
    assert!(
        !text.contains("\"microsteps\"") && !text.contains("\"internal_unhandled\""),
        "{text}"
    );
}

#[test]
fn a_reactive_trace_carries_each_reaction_with_its_own_candidates_and_pipeline() {
    let (m, t) = cascade_machine(3);
    let Outcome::Applied(a) = go(&m, &t) else {
        panic!("the cascade applies under a bound of 3");
    };
    let indices: Vec<u32> = a.trace.microsteps.iter().map(|m| m.index).collect();
    assert_eq!(indices, [1, 2]);
    for microstep in &a.trace.microsteps {
        assert_eq!(
            microstep.trigger,
            fsm_core::trace::MicrostepTrigger::Eventless
        );
        assert!(!microstep.candidates.is_empty(), "the guard was scored");
        assert!(!microstep.pipeline.is_empty(), "the block ran");
    }
    assert_eq!(a.trace.microsteps[0].exited, ["b"]);
    assert_eq!(a.trace.microsteps[0].entered, ["c"]);
    assert_eq!(a.trace.microsteps[1].exited, ["c"]);
    assert_eq!(a.trace.microsteps[1].entered, ["d"]);
    // The value form nests each reaction as its own section.
    let value = a.trace.to_value();
    let sections = value.get("microsteps").and_then(Value::as_arr).unwrap();
    assert_eq!(sections.len(), 2);
    for section in sections {
        assert!(section.get("candidates").and_then(Value::as_arr).is_some());
        assert!(section.get("pipeline").and_then(Value::as_arr).is_some());
        assert_eq!(
            section.get("trigger").and_then(Value::as_str),
            Some("eventless")
        );
    }
}

#[test]
fn a_discarded_internal_event_is_traced_as_unhandled() {
    let (m, t) = cascade_machine(3);
    let Outcome::Applied(a) = go(&m, &t) else {
        panic!("the cascade applies under a bound of 3");
    };
    assert_eq!(a.trace.internal_unhandled.len(), 1);
    assert_eq!(a.trace.internal_unhandled[0].event, "ping");
    assert_eq!(a.trace.internal_unhandled[0].after_microstep, 2);
    let value = a.trace.to_value();
    let unhandled = value
        .get("internal_unhandled")
        .and_then(Value::as_arr)
        .unwrap();
    assert_eq!(
        unhandled[0].get("event").and_then(Value::as_str),
        Some("ping")
    );
}

#[test]
fn a_rejected_macrostep_keeps_the_microsteps_that_ran_in_order() {
    let (m, t) = cascade_machine(2);
    let Outcome::Rejected(r) = go(&m, &t) else {
        panic!("x reaches 2 and the invariant refuses at quiescence");
    };
    assert_eq!(r.code, "run/invariant");
    let indices: Vec<u32> = r.trace.microsteps.iter().map(|m| m.index).collect();
    assert_eq!(indices, [1, 2], "both reactions ran before the failure");
    assert_eq!(r.trace.microsteps[1].entered, ["d"]);
    assert_eq!(r.trace.internal_unhandled.len(), 1);
    let value = r.trace.to_value();
    assert_eq!(
        value
            .get("microsteps")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        Some(2)
    );
}

#[test]
fn trace_values_round_trip_through_canonical_bytes() {
    let (m, t) = cascade_machine(3);
    let Outcome::Applied(a) = go(&m, &t) else {
        panic!("the cascade applies under a bound of 3");
    };
    let (m, t) = cascade_machine(2);
    let Outcome::Rejected(r) = go(&m, &t) else {
        panic!("the invariant refuses under a bound of 2");
    };
    for value in [a.trace.to_value(), r.trace.to_value()] {
        let bytes = canon_bytes(&value);
        let reparsed = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        assert_eq!(canon_bytes(&reparsed), bytes);
        assert_eq!(reparsed, value);
    }
}
