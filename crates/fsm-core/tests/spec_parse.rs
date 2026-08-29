//! Parse `fsm.machine/1` and malformed variants.

use fsm_core::json::{JsonLimits, Value, parse};
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

// Plan 0009 task 4301: `on` is optional, and its absence is the whole syntax
// of an eventless transition. The risk is identity — `machine_id` hashes the
// canonical definition — so the serializer must never invent the key.

fn parse_src(src: &str) -> Result<fsm_core::spec::MachineSpec, Vec<fsm_core::spec::Finding>> {
    parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
}

fn with_transitions(transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}},{{"name":"b"}}],"initial":"a","context":[],"events":[{{"name":"go","fields":[]}}],"transitions":{transitions}}}"#
    )
}

#[test]
fn an_omitted_on_parses_as_an_eventless_transition() {
    let spec = parse_src(&with_transitions(r#"[{"from":"a","to":"b"}]"#)).unwrap();
    assert_eq!(spec.transitions[0].on, None);
    assert!(spec.transitions[0].is_eventless());
    assert_eq!(spec.transitions[0].cell_key(), fsm_core::spec::ALWAYS_KEY);
    let evented = parse_src(&with_transitions(r#"[{"from":"a","on":"go","to":"b"}]"#)).unwrap();
    assert_eq!(evented.transitions[0].on.as_deref(), Some("go"));
    assert_eq!(evented.transitions[0].cell_key(), "go");
}

#[test]
fn an_explicit_null_on_is_def_shape_at_the_pointer() {
    let errs = parse_src(&with_transitions(r#"[{"from":"a","on":null,"to":"b"}]"#)).unwrap_err();
    let finding = errs
        .iter()
        .find(|f| f.code == "def/shape")
        .expect("def/shape for a null on");
    assert_eq!(finding.path, "/transitions/0/on");
    assert!(
        finding.hint.contains("omit on"),
        "the hint says to omit the key: {}",
        finding.hint
    );
}

#[test]
fn an_empty_on_keeps_the_unknown_event_rule() {
    let spec = parse_src(&with_transitions(r#"[{"from":"a","on":"","to":"b"}]"#)).unwrap();
    assert_eq!(spec.transitions[0].on.as_deref(), Some(""));
    let errs = fsm_core::spec::validate(&spec).unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "def/unknown_event"),
        "{errs:?}"
    );
}

#[test]
fn round_trip_keeps_an_eventless_transition_without_an_on_key() {
    let src =
        with_transitions(r#"[{"from":"a","on":"go","to":"b"},{"from":"b","if":"true","to":"a"}]"#);
    let spec = parse_src(&src).unwrap();
    let rendered = spec.to_value();
    let transitions = rendered.get("transitions").and_then(Value::as_arr).unwrap();
    assert_eq!(transitions[0].get("on").and_then(Value::as_str), Some("go"));
    assert!(
        transitions[1].get("on").is_none(),
        "no on key for an eventless transition"
    );
    let again = parse_machine(&rendered).unwrap();
    assert_eq!(again.transitions, spec.transitions);
    assert_eq!(
        fsm_core::canon::canon_bytes(&again.to_value()),
        fsm_core::canon::canon_bytes(&rendered),
        "parse → serialize → parse is byte-stable"
    );
}

/// The compatibility anchor: every committed example keeps the machine id the
/// pre-change build computed, recorded in `fixtures/hashes/identity.jsonl`.
/// Every shipped example, keyed by digest: the catalogue a store holding
/// them would offer an `invoke` slot.
fn example_catalogue(root: &std::path::Path) -> fsm_core::spec::Catalogue {
    let mut catalogue = fsm_core::spec::Catalogue::new();
    for entry in std::fs::read_dir(root.join("examples")).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") || name.ends_with(".handlers.json") {
            continue;
        }
        let bytes = std::fs::read(entry.path()).unwrap();
        let document = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        if let Ok(spec) = fsm_core::spec::parse_machine(&document)
            && let Some(digest) =
                fsm_core::hashes::digest_of(&fsm_core::hashes::machine_id(&document))
        {
            catalogue.insert(digest.to_string(), spec);
        }
    }
    catalogue
}

#[test]
fn every_example_keeps_its_committed_machine_id() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let identities = include_str!("fixtures/hashes/identity.jsonl");
    let mut checked = 0;
    for line in identities.lines().filter(|l| !l.is_empty()) {
        let record = parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        let Some(file) = record.get("file").and_then(Value::as_str) else {
            continue;
        };
        let Some(example) = file.strip_prefix("../../../../../examples/") else {
            continue;
        };
        let bytes = std::fs::read(root.join("examples").join(example)).unwrap();
        let document = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        // A composing example types its done-invoke payload from the child's
        // declarations, so the catalogue here is every shipped example —
        // which is what a store holding them would offer.
        let compiled =
            fsm_core::spec::compile_accepted_with_catalogue(&document, &example_catalogue(&root))
                .unwrap_or_else(|e| panic!("{example} {e:?}"));
        assert_eq!(
            compiled.machine_id,
            record.get("id").and_then(Value::as_str).unwrap(),
            "{example}"
        );
        checked += 1;
    }
    // Every machine in `examples/` has a pinned identity. `.handlers.json` is
    // an executor table and `.cases.json` is a case file: neither is a
    // machine, so neither has a machine id to pin.
    let shipped = std::fs::read_dir(root.join("examples"))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.ends_with(".json")
                && !name.ends_with(".handlers.json")
                && !name.ends_with(".cases.json")
        })
        .count();
    assert_eq!(checked, shipped, "every shipped example is pinned");
    assert!(checked >= 5);
}

#[test]
fn an_eventless_transition_compiles_under_the_always_cell() {
    let spec = parse_src(&with_transitions(
        r#"[{"from":"a","on":"go","to":"b"},{"from":"a","if":"true","to":"b"}]"#,
    ))
    .unwrap();
    let compiled = fsm_core::spec::compile(spec).unwrap();
    assert_eq!(
        compiled.transitions_by.get(&("a".into(), "go".into())),
        Some(&vec![0])
    );
    assert_eq!(
        compiled
            .transitions_by
            .get(&("a".into(), fsm_core::spec::ALWAYS_KEY.into())),
        Some(&vec![1])
    );
}

#[test]
fn thirty_three_eventless_transitions_from_one_state_exceed_the_cell() {
    let transitions: Vec<String> = (0..33)
        .map(|i| format!(r#"{{"from":"a","if":"{i} > 0","to":"b"}}"#))
        .collect();
    let spec = parse_src(&with_transitions(&format!("[{}]", transitions.join(",")))).unwrap();
    let errs = fsm_core::spec::validate(&spec).unwrap_err();
    assert!(errs.iter().any(|f| f.code == "def/limit_cell"), "{errs:?}");
}

#[test]
fn a_declared_event_named_always_is_reserved() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"$always","fields":[]}],"transitions":[]}"#;
    let errs = parse_src(src).unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "def/reserved_ident"),
        "the collision argument in ALWAYS_KEY's doc is load-bearing: {errs:?}"
    );
}
