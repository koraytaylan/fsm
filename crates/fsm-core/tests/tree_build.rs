use fsm_core::spec::load_machine_json;
use fsm_core::tree::{NodeKind, Tree};

#[test]
fn case_review_tables() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let t = Tree::for_machine(&spec);
    let rows: &[(&str, Option<&str>, u8, &str, Option<&str>)] = &[
        ("intake", None, 1, "leaf", None),
        ("in_review", None, 1, "compound", Some("docs_review")),
        ("resume_review", Some("in_review"), 2, "history", None),
        ("docs_review", Some("in_review"), 2, "leaf", None),
        ("risk_review", Some("in_review"), 2, "leaf", None),
        ("suspended", None, 1, "leaf", None),
        ("approved", None, 1, "leaf", None),
        ("rejected", None, 1, "leaf", None),
    ];
    assert_eq!(t.names.len(), 8);
    for (i, (name, parent, depth, kind, init)) in rows.iter().enumerate() {
        assert_eq!(t.names[i], *name, "name {i}");
        assert_eq!(t.parent[i].map(|p| t.names[p as usize].as_str()), *parent);
        assert_eq!(t.depth[i], *depth);
        let k = match &t.kind[i] {
            NodeKind::Leaf => "leaf",
            NodeKind::Compound => "compound",
            NodeKind::History(_) => "history",
        };
        assert_eq!(k, *kind);
        assert_eq!(
            t.initial_child[i].map(|c| t.names[c as usize].as_str()),
            *init
        );
        assert_eq!(t.index[*name] as usize, i);
    }
    assert_eq!(
        t.chain(t.id("docs_review").unwrap())
            .into_iter()
            .map(|i| t.names[i as usize].as_str())
            .collect::<Vec<_>>(),
        ["docs_review", "in_review"]
    );
    assert_eq!(
        t.chain(t.id("risk_review").unwrap())
            .into_iter()
            .map(|i| t.names[i as usize].as_str())
            .collect::<Vec<_>>(),
        ["risk_review", "in_review"]
    );
    assert_eq!(
        t.chain(t.id("intake").unwrap())
            .into_iter()
            .map(|i| t.names[i as usize].as_str())
            .collect::<Vec<_>>(),
        ["intake"]
    );
    let t2 = Tree::for_machine(&spec);
    assert_eq!(t, t2);
}

#[test]
fn depth4_document_order() {
    let spec = fsm_core::spec::parse_machine(
        &fsm_core::json::parse(
            br#"{"format":"fsm.machine/1","name":"d","states":[
                {"name":"c1","initial":"c2","states":[{"name":"c2","initial":"c3","states":[{"name":"c3","initial":"leaf","states":[{"name":"leaf"}]}]}]},
                {"name":"sib"}
            ],"initial":"c1","context":[],"events":[],"transitions":[]}"#,
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .unwrap();
    let t = Tree::for_machine(&spec);
    assert_eq!(t.names, ["c1", "c2", "c3", "leaf", "sib"]);
    assert_eq!(t.depth, [1, 2, 3, 4, 1]);
    assert_eq!(t.parent[4], None);
}
