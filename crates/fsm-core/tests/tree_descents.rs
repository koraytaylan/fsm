use fsm_core::spec::load_machine_json;
use fsm_core::tree::Tree;

fn names(t: &Tree, ids: &[u16]) -> Vec<String> {
    ids.iter().map(|&i| t.names[i as usize].clone()).collect()
}

#[test]
fn initial_and_history() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let t = Tree::for_machine(&spec);
    let ir = t.id("in_review").unwrap();
    assert_eq!(names(&t, &t.initial_descent(ir)), ["docs_review"]);
    assert!(t.initial_descent(t.id("intake").unwrap()).is_empty());
    let h = t.id("resume_review").unwrap();
    assert_eq!(
        names(&t, &t.history_descent(h, Some("risk_review"))),
        ["risk_review"]
    );
    assert_eq!(names(&t, &t.history_descent(h, None)), ["docs_review"]);
    // last is leaf, owner not included
    let d = t.history_descent(h, Some("risk_review"));
    assert_ne!(d[0], ir);
    assert!(matches!(
        t.kind[*d.last().unwrap() as usize],
        fsm_core::tree::NodeKind::Leaf
    ));
}
