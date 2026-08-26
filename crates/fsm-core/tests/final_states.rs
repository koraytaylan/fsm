//! `final: true`: a leaf whose entry ends its parent compound's inner
//! workflow — distinct from, and orthogonal to, `terminal`.
//!
//! Plan 0009 task 4501. Generation of `$done.state.*` is task 4502's; this
//! task pins the shape and the five admission rules.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{CompiledMachine, InstanceState, Status};
use fsm_core::spec::{Finding, compile, compile_accepted, parse_machine, validate};
use fsm_core::step::{Outcome, create, step};
use fsm_core::trace::BlockKind;
use fsm_core::tree::Tree;

fn findings(src: &str) -> Vec<Finding> {
    match parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()) {
        Ok(spec) => validate(&spec).err().unwrap_or_default(),
        Err(findings) => findings,
    }
}

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn definition(states: &str, transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{states},"initial":"review","context":[{{"name":"decided","ty":"bool","init":"false"}}],"events":[{{"name":"approve","fields":[]}}],"transitions":{transitions}}}"#
    )
}

const REVIEW: &str = r#"[{"name":"review","initial":"pending","states":[{"name":"pending"},{"name":"approved","final":true,"entry":{"do":[{"target":"decided","value":"true"}]}}]},{"name":"settled","terminal":true}]"#;

#[test]
fn a_compound_with_one_final_leaf_validates_and_compiles() {
    let (m, _) = machine(&definition(
        REVIEW,
        r#"[{"from":"pending","on":"approve","to":"approved"}]"#,
    ));
    let review = m
        .spec
        .walk_states()
        .into_iter()
        .find(|(n, _)| n.name == "review")
        .unwrap()
        .0;
    assert!(review.states[1].final_state);
    assert!(!review.states[1].terminal);
}

fn only_finding(src: &str, code: &str) -> Finding {
    let found = findings(src);
    let matching: Vec<&Finding> = found.iter().filter(|f| f.code == code).collect();
    assert_eq!(matching.len(), 1, "{code} once: {found:?}");
    matching[0].clone()
}

#[test]
fn each_of_the_five_rules_fires_at_its_pointer() {
    let not_leaf = definition(
        r#"[{"name":"review","initial":"pending","states":[{"name":"pending"},{"name":"approved","final":true,"initial":"inner","states":[{"name":"inner"}]}]}]"#,
        "[]",
    );
    let f = only_finding(&not_leaf, "def/final_not_leaf");
    assert_eq!(f.path, "/states/approved");
    assert!(f.hint.contains("leaf"), "{}", f.hint);

    let at_root = definition(r#"[{"name":"review"},{"name":"done","final":true}]"#, "[]");
    let f = only_finding(&at_root, "def/final_at_root");
    assert_eq!(f.path, "/states/done");
    assert!(f.hint.contains("terminal"), "{}", f.hint);

    let region_root = r#"{"format":"fsm.machine/1","name":"m","regions":[{"name":"left","states":[{"name":"l0"},{"name":"lf","final":true}],"initial":"l0"},{"name":"right","states":[{"name":"r0"}],"initial":"r0"}],"context":[],"events":[],"transitions":[]}"#;
    let f = only_finding(region_root, "def/final_at_root");
    assert_eq!(f.path, "/states/lf");

    let both = definition(
        r#"[{"name":"review","initial":"pending","states":[{"name":"pending"},{"name":"approved","final":true,"terminal":true}]}]"#,
        "[]",
    );
    let f = only_finding(&both, "def/final_and_terminal");
    assert_eq!(f.path, "/states/approved");

    let from_final = definition(
        REVIEW,
        r#"[{"from":"approved","on":"approve","to":"pending"}]"#,
    );
    let f = only_finding(&from_final, "def/final_has_transitions");
    assert_eq!(f.path, "/transitions/0/from");
    let eventless_from_final = definition(REVIEW, r#"[{"from":"approved","to":"pending"}]"#);
    assert_eq!(
        only_finding(&eventless_from_final, "def/final_has_transitions").path,
        "/transitions/0/from"
    );
    let deadline_from_final = format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{REVIEW},"initial":"review","context":[],"events":[],"transitions":[],"deadlines":[{{"name":"late","from":"approved","after":"dur(1, s)","to":"pending"}}]}}"#
    );
    assert_eq!(
        only_finding(&deadline_from_final, "def/final_has_transitions").path,
        "/deadlines/0/from"
    );

    let is_initial = definition(
        r#"[{"name":"review","initial":"approved","states":[{"name":"pending"},{"name":"approved","final":true}]}]"#,
        "[]",
    );
    let f = only_finding(&is_initial, "def/final_is_initial");
    assert_eq!(f.path, "/states/review/initial");
}

#[test]
fn a_final_state_is_an_ordinary_leaf_otherwise() {
    let (m, t) = machine(&definition(
        REVIEW,
        r#"[{"from":"pending","on":"approve","to":"approved"}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match step(
        &m,
        &t,
        &state,
        "approve",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        Outcome::Applied(out) => {
            assert_eq!(out.configuration_after.sequential_leaf(), Some("approved"));
            assert_eq!(
                out.ctx_after["decided"].canonical_string(),
                "true",
                "the entry block ran"
            );
            assert!(
                out.trace
                    .pipeline
                    .iter()
                    .any(|b| b.block == BlockKind::Entry("approved".into()))
            );
            assert_eq!(out.status_after, Status::Running, "final is not terminal");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_final_state_may_be_an_eventless_target_and_bind_history() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"review","initial":"pending","states":[{"name":"h","history":"deep"},{"name":"pending"},{"name":"approved","final":true}]},{"name":"aside"}],"initial":"review","context":[{"name":"ready","ty":"bool","init":"false"}],"events":[{"name":"step_out","fields":[]},{"name":"back","fields":[]}],"transitions":[{"from":"pending","if":"ctx.ready","to":"approved"},{"from":"review","on":"step_out","to":"aside"},{"from":"aside","on":"back","to":"h"}]}"#;
    assert!(findings(src).is_empty(), "{:?}", findings(src));
    let (m, _) = machine(src);
    assert_eq!(m.spec.transitions[0].to.as_deref(), Some("approved"));
}

#[test]
fn a_history_pseudostate_cannot_be_final() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"p","initial":"a","states":[{"name":"h","history":"deep","final":true},{"name":"a"}]}],"initial":"p","context":[],"events":[],"transitions":[]}"#;
    let found = findings(src);
    assert!(
        found
            .iter()
            .any(|f| f.code == "def/shape" && f.path == "/states/h" && f.message.contains("final")),
        "{found:?}"
    );
}

#[test]
fn a_non_boolean_final_is_def_shape_at_the_pointer() {
    let src = definition(
        r#"[{"name":"review","initial":"pending","states":[{"name":"pending"},{"name":"approved","final":"yes"}]}]"#,
        "[]",
    );
    let found = findings(&src);
    assert!(
        found
            .iter()
            .any(|f| f.code == "def/shape" && f.path == "/states/0/states/1/final"),
        "{found:?}"
    );
}

#[test]
fn final_states_without_reactive_transitions_still_validate() {
    let src = definition(
        REVIEW,
        r#"[{"from":"pending","on":"approve","to":"approved"}]"#,
    );
    assert!(findings(&src).is_empty());
}

#[test]
fn identity_moves_only_when_final_is_declared() {
    let plain = definition(
        r#"[{"name":"review","initial":"pending","states":[{"name":"pending"},{"name":"approved"}]},{"name":"settled","terminal":true}]"#,
        r#"[{"from":"pending","on":"approve","to":"approved"}]"#,
    );
    let marked = definition(
        REVIEW,
        r#"[{"from":"pending","on":"approve","to":"approved"}]"#,
    );
    let rendered = parse_machine(&parse(plain.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap()
        .to_value();
    let review = &rendered.get("states").and_then(Value::as_arr).unwrap()[0];
    let approved = &review.get("states").and_then(Value::as_arr).unwrap()[1];
    assert!(approved.get("final").is_none(), "false is omitted");
    let rendered = parse_machine(&parse(marked.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap()
        .to_value();
    let review = &rendered.get("states").and_then(Value::as_arr).unwrap()[0];
    let approved = &review.get("states").and_then(Value::as_arr).unwrap()[1];
    assert_eq!(approved.get("final").and_then(Value::as_bool), Some(true));
    let id = |src: &str| {
        compile_accepted(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
            .unwrap()
            .machine_id
    };
    assert_ne!(id(&plain), id(&marked));
}
