use fsm_core::analyze::{ancestor_shadowed, create_always_fails, shadowing_findings};
use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{compile, load_machine_json, parse_machine};
use fsm_core::tree::Tree;

fn comp(src: &str) -> (fsm_core::machine::CompiledMachine, Tree) {
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    (m, t)
}

#[test]
fn case_review_clean() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    assert!(shadowing_findings(&m).is_empty());
    assert!(ancestor_shadowed(&m, &t).is_empty());
    assert!(create_always_fails(&m, &t).is_empty());
}

#[test]
fn shadowed_and_dup() {
    let (m, _) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","to":"a"},{"from":"a","on":"e","if":"true","to":"a"}]}"#,
    );
    assert!(
        shadowing_findings(&m)
            .iter()
            .any(|f| f.code == "def/shadowed")
    );
}

#[test]
fn dup_guard() {
    let (m, _) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.x > 0","to":"a"},{"from":"a","on":"e","if":"ctx.x  >  0","to":"a"}]}"#,
    );
    assert!(
        shadowing_findings(&m)
            .iter()
            .any(|f| f.code == "def/duplicate_guard"),
        "{:?}",
        shadowing_findings(&m)
    );
}

#[test]
fn different_guard_ok() {
    let (m, _) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"ctx.x > 0","to":"a"},{"from":"a","on":"e","if":"ctx.x < 0","to":"a"}]}"#,
    );
    assert!(
        !shadowing_findings(&m)
            .iter()
            .any(|f| f.code == "def/duplicate_guard")
    );
}

#[test]
fn ancestor_quadrants() {
    // every leaf masked by guardless child
    let (m, t) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"l","states":[{"name":"l"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"l"},{"from":"c","on":"e","to":"l"}]}"#,
    );
    assert!(
        ancestor_shadowed(&m, &t)
            .iter()
            .any(|f| f.code == "def/ancestor_shadowed")
    );
    // live leaf: sibling without handler
    let (m, t) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"l","states":[{"name":"l"},{"name":"r"}]}],"initial":"c","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"l","on":"e","to":"l"},{"from":"c","on":"e","to":"l"}]}"#,
    );
    assert!(
        !ancestor_shadowed(&m, &t)
            .iter()
            .any(|f| f.code == "def/ancestor_shadowed")
    );
}

#[test]
fn create_always_fails_overflow() {
    let (m, t) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"l","entry":{"do":[{"target":"n","value":"9223372036854775807 + 1"}]},"states":[{"name":"l"}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[]}"#,
    );
    assert!(
        create_always_fails(&m, &t)
            .iter()
            .any(|f| f.code == "def/create_always_fails")
    );
}

#[test]
fn create_always_fails_skips_overrideable_init() {
    let (m, t) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"l","entry":{"do":[{"target":"n","value":"ctx.n + 1"}]},"states":[{"name":"l"}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"9223372036854775807"}],"events":[],"transitions":[]}"#,
    );
    assert!(
        create_always_fails(&m, &t).is_empty(),
        "declared MAX init overflows but default override 0 succeeds"
    );
}

#[test]
fn create_always_fails_skips_boolean_override() {
    let (m, t) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"b","ty":"bool","init":"false"}],"events":[],"transitions":[],"invariants":[{"name":"need","expr":"ctx.b","mode":"enforce"}]}"#,
    );
    assert!(
        create_always_fails(&m, &t).is_empty(),
        "ctx.b=false fails declared create but b=true succeeds"
    );
    let ok = fsm_core::step::create(
        &m,
        &t,
        &std::collections::BTreeMap::from([("b".into(), fsm_core::expr::eval::Val::Bool(true))]),
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn create_always_fails_skips_int_eq_override() {
    let (m, t) = comp(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"invariants":[{"name":"need","expr":"ctx.n == 7","mode":"enforce"}]}"#,
    );
    assert!(
        create_always_fails(&m, &t).is_empty(),
        "ctx.n==7 fails empty/default/alt but n=7 succeeds"
    );
    let ok = fsm_core::step::create(
        &m,
        &t,
        &std::collections::BTreeMap::from([("n".into(), fsm_core::expr::eval::Val::Int(7))]),
    );
    assert!(ok.is_ok(), "{ok:?}");
}
