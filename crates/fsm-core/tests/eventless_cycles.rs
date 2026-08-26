//! The eventless cycle analysis: a machine that provably cannot quiesce is
//! refused at admission; a cycle a guard must break is a warning; a cascade
//! that approaches the shared microstep ceiling is a warning.
//!
//! Plan 0009 task 4304.

use std::collections::BTreeMap;

use fsm_core::analyze::{analyze_all, eventless_cycle_findings};
use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::CompiledMachine;
use fsm_core::spec::{Finding, Severity, compile, parse_machine};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn compiled(src: &str) -> Result<CompiledMachine, Vec<Finding>> {
    compile(parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap())
}

fn analysis(src: &str) -> Vec<Finding> {
    let m = compiled(src).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    analyze_all(&m, &t)
}

fn refused(src: &str) -> Vec<Finding> {
    compiled(src).err().expect("the definition is refused")
}

fn sequential(states: &str, transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{states},"initial":"a","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":{transitions}}}"#
    )
}

fn chain(states: usize, back_edge: bool, guarded: bool) -> String {
    let names: Vec<String> = (0..states)
        .map(|i| format!(r#"{{"name":"s{i}"}}"#))
        .collect();
    let mut transitions: Vec<String> = (0..states - 1)
        .map(|i| format!(r#"{{"from":"s{i}","to":"s{}"}}"#, i + 1))
        .collect();
    if back_edge {
        let guard = if guarded { r#""if":"ctx.x > 0","# } else { "" };
        transitions.push(format!(r#"{{"from":"s{}",{guard}"to":"s0"}}"#, states - 1));
    }
    format!(
        r#"{{"format":"fsm.machine/1","name":"chain","states":[{}],"initial":"s0","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{}]}}"#,
        names.join(","),
        transitions.join(",")
    )
}

#[test]
fn a_guardless_two_state_cycle_is_refused() {
    let findings = refused(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","to":"b"},{"from":"b","to":"a"}]"#,
    ));
    let cycle = findings
        .iter()
        .find(|f| f.code == "def/eventless_cycle")
        .expect("def/eventless_cycle");
    assert_eq!(cycle.severity, Severity::Error);
    assert_eq!(cycle.path, "/transitions/0");
    assert!(cycle.message.contains("a, b"), "{}", cycle.message);
    assert!(cycle.message.contains("0, 1"), "{}", cycle.message);
}

#[test]
fn a_guard_on_one_edge_makes_the_cycle_a_warning_and_the_machine_accepted() {
    let src = sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","to":"b"},{"from":"b","if":"ctx.x > 0","to":"a"}]"#,
    );
    let findings = analysis(&src);
    let warning = findings
        .iter()
        .find(|f| f.code == "def/eventless_cycle_guarded")
        .expect("def/eventless_cycle_guarded");
    assert_eq!(warning.severity, Severity::Warning);
    assert!(warning.message.contains("a, b"), "{}", warning.message);
    assert!(
        warning.hint.contains("run/microstep_limit"),
        "{}",
        warning.hint
    );
    assert!(!findings.iter().any(|f| f.code == "def/eventless_cycle"));
}

#[test]
fn a_guardless_external_self_transition_is_a_certain_cycle() {
    let findings = refused(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","to":"b"}]"#,
    ));
    let cycle = findings
        .iter()
        .find(|f| f.code == "def/eventless_cycle")
        .expect("def/eventless_cycle");
    assert_eq!(cycle.path, "/transitions/1");
    assert!(cycle.message.contains("through b "), "{}", cycle.message);
}

#[test]
fn a_guardless_internal_eventless_transition_is_a_certain_self_loop() {
    // The configuration never changes, so the same guardless candidate is
    // selected forever: an internal eventless transition is a self-edge.
    let findings = refused(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","do":[{"target":"x","value":"ctx.x + 1"}]}]"#,
    ));
    assert!(
        findings.iter().any(|f| f.code == "def/eventless_cycle"),
        "{findings:?}"
    );
}

#[test]
fn a_guarded_internal_eventless_transition_is_both_a_guarded_cycle_and_a_noop_warning() {
    let findings = analysis(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.x > 0"}]"#,
    ));
    let codes: Vec<&str> = findings.iter().map(|f| f.code).collect();
    assert!(codes.contains(&"def/eventless_internal_noop"), "{codes:?}");
    assert!(codes.contains(&"def/eventless_cycle_guarded"), "{codes:?}");
    assert!(!codes.contains(&"def/eventless_cycle"), "{codes:?}");
}

#[test]
fn a_three_state_guardless_cycle_is_one_finding_naming_all_three() {
    let findings = refused(&sequential(
        r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
        r#"[{"from":"a","to":"b"},{"from":"b","to":"c"},{"from":"c","to":"a"}]"#,
    ));
    let cycles: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == "def/eventless_cycle")
        .collect();
    assert_eq!(cycles.len(), 1);
    assert!(
        cycles[0].message.contains("a, b, c"),
        "{}",
        cycles[0].message
    );
    assert!(
        cycles[0].message.contains("0, 1, 2"),
        "{}",
        cycles[0].message
    );
}

#[test]
fn an_acyclic_chain_reports_nothing() {
    let findings = analysis(&chain(6, false, false));
    assert!(
        !findings
            .iter()
            .any(|f| f.code.starts_with("def/eventless_")),
        "{findings:?}"
    );
}

#[test]
fn a_cycle_through_a_history_target_resolves_through_the_owner_initial() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"p","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"}]},{"name":"out"}],"initial":"p","context":[],"events":[],"transitions":[{"from":"l","to":"out"},{"from":"out","to":"h"}]}"#;
    let findings = refused(src);
    let cycle = findings
        .iter()
        .find(|f| f.code == "def/eventless_cycle")
        .expect("def/eventless_cycle");
    assert!(cycle.message.contains("l, out"), "{}", cycle.message);
}

#[test]
fn an_escape_the_scan_could_take_keeps_a_cycle_from_being_certain() {
    // a → b guardless, b → a guardless, but b's leaf-level guarded exit is
    // scanned first and may leave the cycle: not provably non-terminating.
    let src = sequential(
        r#"[{"name":"a"},{"name":"p","initial":"b","states":[{"name":"b"}]},{"name":"exit"}]"#,
        r#"[{"from":"a","to":"p"},{"from":"p","to":"a"},{"from":"b","if":"ctx.x > 0","to":"exit"}]"#,
    );
    let findings = analysis(&src);
    assert!(
        findings
            .iter()
            .any(|f| f.code == "def/eventless_cycle_guarded"),
        "{findings:?}"
    );
}

#[test]
fn a_shadowed_guardless_exit_does_not_count_as_an_escape() {
    // b's guardless leaf transition wins the scan every time, so p's exit is
    // never selected and the cycle through b is certain.
    let src = sequential(
        r#"[{"name":"a"},{"name":"p","initial":"b","states":[{"name":"b"}]},{"name":"exit"}]"#,
        r#"[{"from":"a","to":"p"},{"from":"b","to":"a"},{"from":"p","if":"ctx.x > 0","to":"exit"}]"#,
    );
    assert!(
        refused(&src)
            .iter()
            .any(|f| f.code == "def/eventless_cycle")
    );
}

#[test]
fn a_two_hundred_state_cycle_completes_without_a_deep_stack() {
    let findings = refused(&chain(200, true, false));
    let cycles: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == "def/eventless_cycle")
        .collect();
    assert_eq!(cycles.len(), 1);
}

#[test]
fn a_deep_cascade_warns_and_a_short_one_does_not() {
    let deep = analysis(&chain(41, false, false));
    let warning = deep
        .iter()
        .find(|f| f.code == "def/eventless_depth")
        .expect("def/eventless_depth");
    assert_eq!(warning.severity, Severity::Warning);
    assert!(
        warning.message.contains("40 microsteps"),
        "{}",
        warning.message
    );
    let shallow = analysis(&chain(5, false, false));
    assert!(!shallow.iter().any(|f| f.code == "def/eventless_depth"));
}

#[test]
fn regions_share_the_ceiling_so_depth_multiplies_by_their_count() {
    let regions: Vec<String> = (0..8)
        .map(|r| {
            let states: Vec<String> = (0..6)
                .map(|i| format!(r#"{{"name":"r{r}s{i}"}}"#))
                .collect();
            let transitions: Vec<String> = (0..5)
                .map(|i| format!(r#"{{"from":"r{r}s{i}","to":"r{r}s{}"}}"#, i + 1))
                .collect();
            (states.join(","), transitions.join(","), r)
        })
        .map(|(states, _, r)| {
            format!(r#"{{"name":"region{r}","states":[{states}],"initial":"r{r}s0"}}"#)
        })
        .collect();
    let transitions: Vec<String> = (0..8)
        .flat_map(|r| {
            (0..5).map(move |i| format!(r#"{{"from":"r{r}s{i}","to":"r{r}s{}"}}"#, i + 1))
        })
        .collect();
    let src = format!(
        r#"{{"format":"fsm.machine/1","name":"wide","regions":[{}],"context":[],"events":[],"transitions":[{}]}}"#,
        regions.join(","),
        transitions.join(",")
    );
    let findings = analysis(&src);
    let warning = findings
        .iter()
        .find(|f| f.code == "def/eventless_depth")
        .expect("8 regions × 5 = 40 reactions reaches the shared ceiling");
    assert!(
        warning.message.contains("5 microsteps"),
        "{}",
        warning.message
    );
    assert!(
        warning.message.contains("40 of the 64"),
        "{}",
        warning.message
    );
}

#[test]
fn the_depth_warning_still_admits_creates_and_steps_the_machine() {
    let src = chain(41, false, false);
    let m = compiled(&src).expect("a warning is not a refusal");
    let t = Tree::for_machine(&m.spec);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert_eq!(
        created.trace.microsteps.len(),
        40,
        "the whole cascade runs at creation"
    );
    assert_eq!(created.configuration_after.sequential_leaf(), Some("s40"));
    let state = fsm_core::machine::InstanceState {
        status: created.status_after,
        configuration: created.configuration_after.clone(),
        ctx: created.ctx_after.clone(),
        history: created.history_after.clone(),
        deadlines: created.deadlines_after.clone(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match step(
        &m,
        &t,
        &state,
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/unhandled"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_direct_analysis_entry_point_is_empty_for_a_non_reactive_machine() {
    let m = compiled(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
    ))
    .unwrap();
    let t = Tree::for_machine(&m.spec);
    assert!(eventless_cycle_findings(&m, &t).is_empty());
}
