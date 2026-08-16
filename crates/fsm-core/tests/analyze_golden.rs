use fsm_core::analyze::{completeness_matrix, reachability_findings};
use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{compile, load_machine_json, parse_machine};
use fsm_core::tree::Tree;

#[test]
fn case_review_reachability_and_matrix() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let f = reachability_findings(&m, &t);
    assert!(f.is_empty(), "{f:?}");
    let mat = completeness_matrix(&m, &t);
    assert_eq!(
        mat.get(&("docs_review".into(), "docs_ok".into())).unwrap(),
        "handled@docs_review"
    );
    assert_eq!(
        mat.get(&("docs_review".into(), "note_added".into()))
            .unwrap(),
        "handled@in_review"
    );
    assert_eq!(
        mat.get(&("docs_review".into(), "scored".into())).unwrap(),
        "unhandled(reject)"
    );
    assert_eq!(
        mat.get(&("risk_review".into(), "scored".into())).unwrap(),
        "handled@risk_review"
    );
    assert_eq!(
        mat.get(&("intake".into(), "resume".into())).unwrap(),
        "unhandled(reject)"
    );
    assert_eq!(
        mat.get(&("suspended".into(), "resume".into())).unwrap(),
        "handled@suspended"
    );
    for ev in [
        "docs_ok",
        "scored",
        "note_added",
        "withdraw",
        "suspend",
        "resume",
    ] {
        assert_eq!(
            mat.get(&("approved".into(), ev.into())).unwrap(),
            "unhandled(reject)"
        );
        assert_eq!(
            mat.get(&("rejected".into(), ev.into())).unwrap(),
            "unhandled(reject)"
        );
    }
}

#[test]
fn unreachable_state() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"ghost"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let f = reachability_findings(&m, &t);
    assert!(
        f.iter()
            .any(|x| x.code == "def/unreachable_state" && x.message.contains("ghost")),
        "{f:?}"
    );
    assert_eq!(f.len(), 1);
}

#[test]
fn ignore_policy_matrix() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","on_unhandled":"ignore","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let mat = completeness_matrix(&m, &t);
    assert_eq!(
        mat.get(&("a".into(), "e".into())).unwrap(),
        "unhandled(ignore)"
    );
}
