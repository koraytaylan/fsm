use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{
    accepted_identity, compile, compile_accepted, load_machine_json, parse_machine,
};

#[test]
fn compile_and_compile_accepted_share_accepted_identity() {
    let bytes = include_bytes!("fixtures/machines/case_review.json");
    let v = parse(bytes, &JsonLimits::DEFAULT).unwrap();
    let from_source = compile_accepted(&v).unwrap();
    let (canon_src, mid_src) = accepted_identity(&v);
    assert_eq!(from_source.machine_id, mid_src);
    assert_eq!(from_source.canonical, canon_src);
    let spec = parse_machine(&v).unwrap();
    let from_spec = compile(spec.clone()).unwrap();
    let (canon_spec, mid_spec) = accepted_identity(&spec.to_value());
    assert_eq!(from_spec.machine_id, mid_spec);
    assert_eq!(from_spec.canonical, canon_spec);
}

#[test]
fn omitted_defaults_share_one_identity_path() {
    let src = parse(
        br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert!(src.get("on_unhandled").is_none());
    assert!(src.get("effects").is_none());
    let via_accepted = compile_accepted(&src).unwrap();
    let via_compile = compile(parse_machine(&src).unwrap()).unwrap();
    assert_eq!(via_accepted.machine_id, via_compile.machine_id);
    assert_eq!(via_accepted.canonical, via_compile.canonical);
    let (canon, mid) = accepted_identity(&parse_machine(&src).unwrap().to_value());
    assert_eq!(via_accepted.machine_id, mid);
    assert_eq!(via_accepted.canonical, canon);
}

fn compile_s(s: &str) -> Result<fsm_core::machine::CompiledMachine, Vec<fsm_core::spec::Finding>> {
    let v = parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let spec = parse_machine(&v).map_err(|e| e)?;
    compile(spec)
}

#[test]
fn case_review_index() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let want: &[((&str, &str), &[usize])] = &[
        (("intake", "docs_ok"), &[0]),
        (("docs_review", "docs_ok"), &[1]),
        (("risk_review", "scored"), &[2, 3]),
        (("in_review", "note_added"), &[4]),
        (("in_review", "withdraw"), &[5]),
        (("in_review", "suspend"), &[6]),
        (("suspended", "resume"), &[7]),
    ];
    assert_eq!(m.transitions_by.len(), want.len());
    for ((from, on), idxs) in want {
        assert_eq!(
            m.transitions_by
                .get(&((*from).into(), (*on).into()))
                .map(Vec::as_slice),
            Some(*idxs)
        );
    }
    assert!(
        m.compiled_exprs
            .values()
            .any(|e| e.source == "evt.score >= 700" && e.ty.to_string() == "bool")
    );
}

#[test]
fn binding_errors() {
    let assign = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"1.000"}]}]}"#;
    let errs = compile_s(assign).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "def/assign_type"),
        "{:?}",
        errs
    );

    let dup = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"1"},{"target":"x","value":"2"}]}]}"#;
    let errs = compile_s(dup).unwrap_err();
    assert!(errs.iter().any(|e| e.code == "def/dup_set"), "{:?}", errs);

    let evb = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a","entry":{"do":[{"target":"x","value":"evt.y"}]}}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"y","ty":"int"}]}],"transitions":[]}"#;
    let errs = compile_s(evb).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/evt_in_block"),
        "{:?}",
        errs
    );

    let invi = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"y","ty":"int"}]}],"transitions":[],"invariants":[{"name":"i","expr":"evt.y > 0","mode":"enforce"}]}"#;
    let errs = compile_s(invi).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/evt_in_invariant"),
        "{:?}",
        errs
    );

    let emit = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","emit":[{"effect":"fx","args":{"n":"true"}}]}]}"#;
    let errs = compile_s(emit).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/type_mismatch"),
        "{:?}",
        errs
    );

    let field = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]},{"name":"f","fields":[{"name":"z","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.z > 0"}]}"#;
    let errs = compile_s(field).unwrap_err();
    assert!(
        errs.iter().any(|e| e.code == "expr/unknown_field"),
        "{:?}",
        errs
    );
}

#[test]
fn scope_ok() {
    let s = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a","entry":{"do":[{"target":"x","value":"ctx.x + 1"}]}}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[{"name":"n","ty":"int"}]}],"transitions":[{"from":"a","on":"e","if":"evt.n > 0"}],"invariants":[{"name":"i","expr":"ctx.x >= 0","mode":"enforce"}]}"#;
    compile_s(s).unwrap();
}
