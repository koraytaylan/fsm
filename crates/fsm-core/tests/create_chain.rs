use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{compile, load_machine_json, parse_machine};
use fsm_core::step::create;
use fsm_core::tree::Tree;

#[test]
fn case_review_create() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let a = create(&m, &t, &BTreeMap::new()).unwrap();
    assert_eq!(a.leaf_after, "intake");
    assert_eq!(a.ctx_after.get("visits").unwrap().canonical_string(), "0");
    assert!(a.effects.is_empty());
    assert!(a.history_after.is_empty());
    assert_eq!(a.status_after, fsm_core::machine::Status::Running);
    let b = create(&m, &t, &BTreeMap::new()).unwrap();
    assert_eq!(a.leaf_after, b.leaf_after);
    assert_eq!(a.ctx_after, b.ctx_after);
}

#[test]
fn compound_entry_order() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"leaf","entry":{"do":[{"target":"n","value":"ctx.n + 1"}],"emit":[{"effect":"fx","args":{}}]},"states":[{"name":"leaf","entry":{"do":[{"target":"n","value":"ctx.n + 10"}]}}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"effects":[{"name":"fx","fields":[]}],"transitions":[]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let a = create(&m, &t, &BTreeMap::new()).unwrap();
    assert_eq!(a.entered, ["c", "leaf"]);
    assert_eq!(a.ctx_after.get("n").unwrap().canonical_string(), "11");
    assert_eq!(a.effects[0].k, 0);
    assert!(a.history_after.is_empty());
}

#[test]
fn overrides() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let mut ov = BTreeMap::new();
    ov.insert("score".into(), Val::Int(5));
    let a = create(&m, &t, &ov).unwrap();
    assert_eq!(a.ctx_after.get("score").unwrap().canonical_string(), "5");
    let mut bad = BTreeMap::new();
    bad.insert("nope".into(), Val::Int(1));
    assert_eq!(create(&m, &t, &bad).unwrap_err().code, "req/field_unknown");
    let mut typ = BTreeMap::new();
    typ.insert("score".into(), Val::Bool(true));
    assert_eq!(create(&m, &t, &typ).unwrap_err().code, "req/field_type");
}

#[test]
fn create_failed_pure() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"leaf","entry":{"do":[{"target":"n","value":"9223372036854775807 + 1"}]},"states":[{"name":"leaf"}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let e1 = create(&m, &t, &BTreeMap::new()).unwrap_err();
    let e2 = create(&m, &t, &BTreeMap::new()).unwrap_err();
    assert_eq!(e1.code, "run/create_failed");
    assert_eq!(e1.code, e2.code);
    assert_eq!(e1.message, e2.message);
}

#[test]
fn create_inner_failure_keeps_discarded_outer_trace() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"leaf","entry":{"do":[{"target":"y","value":"1"}]},"states":[{"name":"leaf","entry":{"do":[{"target":"x","value":"ctx.x + 1"}]}}]}],"initial":"c","context":[{"name":"x","ty":"int","init":"9223372036854775807"},{"name":"y","ty":"int","init":"0"}],"events":[],"transitions":[]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let e = create(&m, &t, &BTreeMap::new()).unwrap_err();
    assert_eq!(e.code, "run/create_failed");
    assert_eq!(e.block.as_deref(), Some("entry(leaf)"));
    assert!(e.span.is_some());
    assert!(e.trace.pipeline.iter().all(|p| p.discarded));
    let outer = e
        .trace
        .pipeline
        .iter()
        .find(|p| matches!(p.block, fsm_core::trace::BlockKind::Entry(ref n) if n == "c"))
        .expect("outer entry");
    assert_eq!(outer.sets[0].target, "y");
    assert_eq!(outer.sets[0].after, "1");
    assert!(
        e.trace
            .pipeline
            .iter()
            .any(|p| matches!(p.block, fsm_core::trace::BlockKind::Entry(ref n) if n == "leaf"))
    );
}

#[test]
fn create_first_block_failure_has_failing_trace() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"c","initial":"leaf","entry":{"do":[{"target":"n","value":"9223372036854775807 + 1"}]},"states":[{"name":"leaf"}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let e = create(&m, &t, &BTreeMap::new()).unwrap_err();
    assert_eq!(e.code, "run/create_failed");
    assert_eq!(e.block.as_deref(), Some("entry(c)"));
    assert_eq!(e.trace.pipeline.len(), 1);
    assert!(e.trace.pipeline[0].discarded);
    assert!(!e.trace.pipeline[0].sets.is_empty());
}

#[test]
fn create_invariant_eval_error_has_identity() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[],"transitions":[],"invariants":[{"name":"pos","expr":"ctx.x + 1 > 0","mode":"enforce"}]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let e = create(&m, &t, &BTreeMap::new()).unwrap_err();
    assert_eq!(e.code, "run/create_failed");
    assert_eq!(e.block.as_deref(), Some("invariant(pos)"));
    assert!(e.span.is_some());
    assert!(e.message.contains("pos"));
    let inv = e
        .trace
        .invariants
        .iter()
        .find(|i| i.name == "pos")
        .expect("pos");
    assert!(!inv.passed);
    let err = inv.error.as_ref().unwrap();
    assert_eq!(err.code, "run/overflow");
    let node = err.expr.as_ref().expect("invariant eval node");
    fn has_input(n: &fsm_core::expr::eval::TraceNode, want: &str) -> bool {
        let here = match &n.outcome {
            fsm_core::expr::eval::TraceOutcome::Error { inputs, .. } => {
                inputs.iter().any(|s| s.contains(want))
            }
            fsm_core::expr::eval::TraceOutcome::Value(v) => v.contains(want),
            _ => false,
        };
        here || n.children.iter().any(|c| has_input(c, want))
    }
    assert!(
        has_input(node, "9223372036854775807") || has_input(node, "1"),
        "{node:?}"
    );
}
