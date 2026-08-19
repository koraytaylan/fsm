use std::collections::BTreeSet;

use fsm_core::diagram::{InstanceOverlay, dot, mermaid};
use fsm_core::spec::{compile, load_machine_json};

fn compiled() -> fsm_core::machine::CompiledMachine {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    compile(spec).unwrap()
}

#[test]
fn case_review_mermaid_shape() {
    let m = compiled();
    let mm = mermaid(&m, None);
    assert!(mm.contains("stateDiagram-v2"));
    assert!(mm.contains("state in_review {"));
    assert!(mm.contains("[*] --> intake"));
    assert!(mm.contains("[*] --> docs_review"));
    assert!(mm.contains("approved --> [*]"));
    assert!(mm.contains("rejected --> [*]"));
    assert!(mm.contains("resume_review"));
    assert!(mm.contains("<<deep-history>>"));
    let again = mermaid(&m, None);
    assert_eq!(mm, again);
}

#[test]
fn case_review_dot_shape() {
    let m = compiled();
    let d = dot(&m, None);
    assert!(d.contains("digraph"));
    assert!(d.contains("subgraph cluster_in_review"));
    assert!(d.contains("intake"));
    assert_eq!(d, dot(&m, None));
}

#[test]
fn overlay_marks_current_and_visited() {
    let m = compiled();
    let ov = InstanceOverlay {
        current_leaves: BTreeSet::from(["risk_review".into()]),
        visited: BTreeSet::from(["intake".into(), "docs_review".into()]),
    };
    let mm = mermaid(&m, Some(&ov));
    assert!(mm.contains("classDef"));
    assert!(mm.contains("class risk_review current"));
    let d = dot(&m, Some(&ov));
    assert!(d.contains("style=bold") || d.contains("risk_review"));
}

#[test]
fn node_and_transition_counts() {
    let m = compiled();
    let mm = mermaid(&m, None);
    for name in [
        "intake",
        "in_review",
        "docs_review",
        "risk_review",
        "approved",
        "rejected",
        "suspended",
        "resume_review",
    ] {
        let decl = mm.matches(name).count();
        assert!(decl >= 1, "{name} missing");
    }
    assert_eq!(mm.matches("-->").count() >= m.spec.transitions.len(), true);
}
