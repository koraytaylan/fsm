//! Internal events: declared with `internal: true`, typed like any event,
//! legal as a transition's `on`, and refused from the external send path.
//!
//! Plan 0009 task 4401.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::spec::{Finding, compile, compile_accepted, parse_machine};
use fsm_core::step::{Outcome, create, step, validate_event};
use fsm_core::tree::Tree;

fn parsed(src: &str) -> Result<fsm_core::spec::MachineSpec, Vec<Finding>> {
    parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
}

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let m = compile(parsed(src).unwrap()).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn with_events(events: &str, transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}},{{"name":"b"}}],"initial":"a","context":[{{"name":"total","ty":{{"decimal":"2"}},"init":"0.00"}}],"events":{events},"transitions":{transitions}}}"#
    )
}

const INTERNAL: &str = r#"[{"name":"settle","fields":[{"name":"amount","ty":{"decimal":"2"}}],"internal":true},{"name":"go","fields":[]}]"#;

fn state_after_create(m: &CompiledMachine, t: &Tree) -> InstanceState {
    let created = create(m, t, &BTreeMap::new(), 0).unwrap();
    InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

#[test]
fn an_internal_event_parses_typechecks_and_may_be_a_transition_trigger() {
    let (m, _) = machine(&with_events(
        INTERNAL,
        r#"[{"from":"a","on":"settle","if":"evt.amount > 0.00","to":"b","do":[{"target":"total","value":"evt.amount"}]}]"#,
    ));
    assert!(m.spec.events[0].internal);
    assert!(!m.spec.events[1].internal);
    assert_eq!(m.spec.transitions[0].on.as_deref(), Some("settle"));
    let bad_type = parsed(&with_events(
        INTERNAL,
        r#"[{"from":"a","on":"settle","do":[{"target":"total","value":"evt.amount + 1"}]}]"#,
    ))
    .unwrap();
    let errs = compile(bad_type).unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "expr/mixed_class"),
        "fields of an internal event are typed like any other: {errs:?}"
    );
}

#[test]
fn sending_an_internal_event_from_outside_is_req_event_internal() {
    let (m, t) = machine(&with_events(
        INTERNAL,
        r#"[{"from":"a","on":"settle","to":"b"},{"from":"a","on":"go","to":"b"}]"#,
    ));
    let state = state_after_create(&m, &t);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let payload = Value::Obj(BTreeMap::from([(
        "amount".to_string(),
        Value::Str("1.00".into()),
    )]));
    match step(&m, &t, &state, "settle", &payload, 0, &mut budget) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "req/event_internal");
            assert!(r.message.contains("internal"), "{}", r.message);
            assert!(
                r.hint.contains("go"),
                "the hint lists the sendable events: {}",
                r.hint
            );
            assert!(r.hint.contains("settle"), "and names the event: {}", r.hint);
        }
        other => panic!("{other:?}"),
    }
    let rejection = validate_event(&m, "settle", &payload).unwrap_err();
    assert_eq!(rejection.code, "req/event_internal");
}

#[test]
fn a_generated_event_name_is_refused_as_internal_even_when_nothing_generates_it() {
    let (m, t) = machine(&with_events(
        r#"[{"name":"go","fields":[]}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    ));
    let state = state_after_create(&m, &t);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match step(
        &m,
        &t,
        &state,
        "$done.state.anything",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "req/event_internal");
            assert!(r.hint.contains("go"), "{}", r.hint);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_undeclared_ordinary_event_is_still_req_event_unknown_with_its_suggestion() {
    let (m, t) = machine(&with_events(
        r#"[{"name":"go","fields":[]}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    ));
    let state = state_after_create(&m, &t);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match step(
        &m,
        &t,
        &state,
        "gо_",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        Outcome::Rejected(r) => assert_eq!(r.code, "req/event_unknown"),
        other => panic!("{other:?}"),
    }
    let rejection = validate_event(&m, "goo", &Value::Obj(BTreeMap::new())).unwrap_err();
    assert_eq!(rejection.code, "req/event_unknown");
    assert!(
        rejection.hint.contains("did you mean `go`"),
        "{}",
        rejection.hint
    );
}

#[test]
fn a_non_boolean_internal_flag_is_def_shape_at_the_pointer() {
    let errs = parsed(&with_events(
        r#"[{"name":"tick","fields":[],"internal":"yes"}]"#,
        "[]",
    ))
    .unwrap_err();
    let finding = errs
        .iter()
        .find(|f| f.code == "def/shape")
        .expect("def/shape");
    assert_eq!(finding.path, "/events/0/internal");
    let effects = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[],"effects":[{"name":"fx","fields":[],"internal":true}],"transitions":[]}"#;
    let errs = parsed(effects).unwrap_err();
    assert!(
        errs.iter()
            .any(|f| f.code == "def/unknown_key" && f.path == "/effects/0/internal"),
        "effects do not take the flag: {errs:?}"
    );
}

#[test]
fn identity_moves_only_when_the_flag_is_declared() {
    let plain = with_events(
        r#"[{"name":"go","fields":[]}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    );
    let explicit_false = with_events(
        r#"[{"name":"go","fields":[],"internal":false}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    );
    let marked = with_events(
        r#"[{"name":"go","fields":[],"internal":true}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    );
    let id = |src: &str| {
        compile_accepted(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
            .unwrap()
            .machine_id
    };
    let rendered = parsed(&plain).unwrap().to_value();
    let events = rendered.get("events").and_then(Value::as_arr).unwrap();
    assert!(
        events[0].get("internal").is_none(),
        "false is omitted on serialization"
    );
    let rendered = parsed(&marked).unwrap().to_value();
    let events = rendered.get("events").and_then(Value::as_arr).unwrap();
    assert_eq!(
        events[0].get("internal").and_then(Value::as_bool),
        Some(true)
    );
    assert_ne!(
        id(&plain),
        id(&marked),
        "a machine with an internal event is a different machine"
    );
    // The identity is the accepted source document, so an explicit false is
    // a different document too — and a round trip through the model drops it.
    assert_ne!(id(&plain), id(&explicit_false));
    let round_tripped = parsed(&explicit_false).unwrap().to_value();
    assert!(
        round_tripped.get("events").and_then(Value::as_arr).unwrap()[0]
            .get("internal")
            .is_none()
    );
}

#[test]
fn internal_events_count_against_the_same_ceilings() {
    let fields: Vec<String> = (0..33)
        .map(|i| format!(r#"{{"name":"f{i}","ty":"int"}}"#))
        .collect();
    let too_many_fields = with_events(
        &format!(
            r#"[{{"name":"tick","fields":[{}],"internal":true}}]"#,
            fields.join(",")
        ),
        "[]",
    );
    let errs = fsm_core::spec::validate(&parsed(&too_many_fields).unwrap()).unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "def/limit_fields"),
        "{errs:?}"
    );
    let events: Vec<String> = (0..129)
        .map(|i| {
            format!(
                r#"{{"name":"e{i}","fields":[]{}}}"#,
                if i % 2 == 0 {
                    r#","internal":true"#
                } else {
                    ""
                }
            )
        })
        .collect();
    let too_many_events = with_events(&format!("[{}]", events.join(",")), "[]");
    let errs = fsm_core::spec::validate(&parsed(&too_many_events).unwrap()).unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "def/limit_events"),
        "{errs:?}"
    );
}
