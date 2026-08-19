//! Parse `fsm.machine/1` and malformed variants.

use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{Topology, parse_machine};

fn load_case() -> fsm_core::json::Value {
    let bytes = include_bytes!("fixtures/machines/case_review.json");
    parse(bytes, &JsonLimits::DEFAULT).unwrap()
}

#[test]
fn case_review_shape() {
    let spec = parse_machine(&load_case()).unwrap();
    let states = match &spec.topology {
        Topology::Sequential { states, .. } => states,
        Topology::Parallel { .. } => panic!("case_review must be sequential"),
    };
    let names: Vec<_> = states.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["intake", "in_review", "suspended", "approved", "rejected"]
    );
    let ir = states.iter().find(|s| s.name == "in_review").unwrap();
    let kids: Vec<_> = ir
        .states
        .iter()
        .map(|s| (s.name.as_str(), s.history))
        .collect();
    assert_eq!(kids[0].0, "resume_review");
    assert!(matches!(kids[0].1, Some(fsm_core::spec::HistoryKind::Deep)));
    assert_eq!(kids[1].0, "docs_review");
    assert_eq!(kids[2].0, "risk_review");
    let entry = ir.entry.as_ref().unwrap();
    assert_eq!(entry.sets.len(), 1);
    assert_eq!(entry.emits.len(), 1);
    assert_eq!(ir.exit.as_ref().unwrap().sets.len(), 1);
    assert_eq!(ir.states[2].entry.as_ref().unwrap().sets.len(), 1);
    assert_eq!(spec.transitions.len(), 8);
    assert!(spec.transitions[4].to.is_none());
    assert_eq!(
        spec.invariants[0].mode,
        fsm_core::machine::EnforceMode::Enforce
    );
    assert!(matches!(
        spec.on_unhandled,
        fsm_core::spec::Unhandled::Reject
    ));
}

#[test]
fn model_round_trip() {
    let spec = parse_machine(&load_case()).unwrap();
    let v = spec.to_value();
    let spec2 = parse_machine(&v).unwrap();
    assert_eq!(spec.name, spec2.name);
    assert_eq!(spec.topology, spec2.topology);
    assert_eq!(spec.transitions.len(), spec2.transitions.len());
}

#[test]
fn malformed_dir() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/machines/malformed"
    );
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy();
        let (raw_code, _ptr) = name
            .split_once("__")
            .unwrap_or_else(|| panic!("bad name {name}"));
        let code = raw_code.replacen('_', "/", 1);
        // allow custom pointer encoding: use -- for empty? we use file: CODE__pointer_with_underscores
        let bytes = std::fs::read(&path).unwrap();
        let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        let errs = parse_machine(&v).unwrap_err();
        assert!(errs.iter().any(|e| e.code == code), "{name} got {:?}", errs);
        let want_path = reconstruct_path(&name);
        assert!(
            errs.iter().any(
                |e| e.code == code && (e.path == want_path || e.path.contains(&want_path[1..]))
            ),
            "{name} paths {:?}",
            errs.iter().map(|e| (&e.code, &e.path)).collect::<Vec<_>>()
        );
    }
}

fn reconstruct_path(stem: &str) -> String {
    // CODE__a_b_c → /a/b/c except known literals
    let ptr = stem.split_once("__").unwrap().1;
    match ptr {
        "badkey" => "/badkey".into(),
        "states" => "/states".into(),
        "format" => "/format".into(),
        "transitions_0" => "/transitions/0".into(),
        "transitions_0_on" => "/transitions/0/on".into(),
        "context_0_init" => "/context/0/init".into(),
        "states_1_entry_emit_0_args_total" => "/states/1/entry/emit/0/args/total".into(),
        other => format!("/{other}"),
    }
}
