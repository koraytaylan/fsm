use fsm_core::spec::load_machine_json;
use fsm_core::tree::Tree;

fn names(t: &Tree, ids: &[u16]) -> Vec<String> {
    ids.iter().map(|&i| t.names[i as usize].clone()).collect()
}

#[test]
fn case_review_dom_table() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let t = Tree::for_machine(&spec);
    let id = |n: &str| t.id(n).unwrap();
    // intake → in_review from intake
    let dom = t.proper_lca(id("intake"), id("in_review"));
    assert_eq!(dom, None);
    assert_eq!(names(&t, &t.exit_set(id("intake"), dom)), ["intake"]);
    assert_eq!(
        names(&t, &t.entry_path(dom, id("in_review"))),
        ["in_review"]
    );
    // docs_review → risk_review
    let dom = t.proper_lca(id("docs_review"), id("risk_review"));
    assert_eq!(dom, Some(id("in_review")));
    assert_eq!(
        names(&t, &t.exit_set(id("docs_review"), dom)),
        ["docs_review"]
    );
    assert_eq!(
        names(&t, &t.entry_path(dom, id("risk_review"))),
        ["risk_review"]
    );
    // risk_review → approved
    let dom = t.proper_lca(id("risk_review"), id("approved"));
    assert_eq!(dom, None);
    assert_eq!(
        names(&t, &t.exit_set(id("risk_review"), dom)),
        ["risk_review", "in_review"]
    );
    assert_eq!(names(&t, &t.entry_path(dom, id("approved"))), ["approved"]);
    // withdraw from docs_review to rejected
    let dom = t.proper_lca(id("in_review"), id("rejected"));
    assert_eq!(dom, None);
    assert_eq!(
        names(&t, &t.exit_set(id("docs_review"), dom)),
        ["docs_review", "in_review"]
    );
    assert_eq!(names(&t, &t.entry_path(dom, id("rejected"))), ["rejected"]);
    // suspend from risk_review
    let dom = t.proper_lca(id("in_review"), id("suspended"));
    assert_eq!(
        names(&t, &t.exit_set(id("risk_review"), dom)),
        ["risk_review", "in_review"]
    );
    assert_eq!(
        names(&t, &t.entry_path(dom, id("suspended"))),
        ["suspended"]
    );
    // resume → owner in_review
    let dom = t.proper_lca(id("suspended"), id("in_review"));
    assert_eq!(dom, None);
    assert_eq!(names(&t, &t.exit_set(id("suspended"), dom)), ["suspended"]);
    assert_eq!(
        names(&t, &t.entry_path(dom, id("in_review"))),
        ["in_review"]
    );
}

#[test]
fn lca_units() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let t = Tree::for_machine(&spec);
    let id = |n: &str| t.id(n).unwrap();
    assert_eq!(
        t.proper_lca(id("docs_review"), id("risk_review")),
        Some(id("in_review"))
    );
    assert_eq!(t.proper_lca(id("docs_review"), id("in_review")), None);
    assert_eq!(t.proper_lca(id("intake"), id("suspended")), None);
    assert_eq!(t.proper_lca(id("risk_review"), id("intake")), None);
}

#[test]
fn external_self() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let t = Tree::for_machine(&spec);
    let x = t.id("intake").unwrap();
    let dom = t.parent[x as usize];
    assert_eq!(names(&t, &t.exit_set(x, dom)), ["intake"]);
    assert_eq!(names(&t, &t.entry_path(dom, x)), ["intake"]);
}
