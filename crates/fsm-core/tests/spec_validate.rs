//! Structural validation fixtures.

use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{load_machine_json, parse_machine, validate};

fn parse_s(s: &str) -> fsm_core::spec::MachineSpec {
    let v = parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    parse_machine(&v).unwrap_or_else(|e| panic!("{e:?}"))
}

fn codes_of(s: &str) -> Vec<&'static str> {
    let spec = match parse_s_res(s) {
        Ok(sp) => sp,
        Err(cs) => return cs,
    };
    match validate(&spec) {
        Ok(()) => vec![],
        Err(fs) => fs.into_iter().map(|f| f.code).collect(),
    }
}

fn parse_s_res(s: &str) -> Result<fsm_core::spec::MachineSpec, Vec<&'static str>> {
    let v = parse(s.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    parse_machine(&v).map_err(|e| e.into_iter().map(|f| f.code).collect())
}

fn wrap(states: &str, extra: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{states},"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"effects":[{{"name":"fx","fields":[]}}],"transitions":[{extra}]}}"#
    )
}

#[test]
fn case_review_validates() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    assert!(validate(&spec).is_ok(), "{:?}", validate(&spec));
}

#[test]
fn each_structural_rule() {
    // dup name: state + history same name
    let s = wrap(
        r#"[{"name":"a"},{"name":"c","initial":"h","states":[{"name":"a","history":"deep"},{"name":"x"}]}]"#,
        "",
    );
    assert!(codes_of(&s).contains(&"def/dup_name"), "{:?}", codes_of(&s));

    let s = wrap(r#"[{"name":"a"},{"name":"c","states":[{"name":"x"}]}]"#, "");
    assert!(
        codes_of(&s).contains(&"def/one_initial"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(
        r#"[{"name":"a"},{"name":"c","initial":"z","states":[{"name":"x","states":[{"name":"z"}],"initial":"z"}]}]"#,
        "",
    );
    assert!(
        codes_of(&s).contains(&"def/initial_not_child")
            || codes_of(&s).contains(&"def/one_initial"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(
        r#"[{"name":"a"},{"name":"c","initial":"h","states":[{"name":"h","history":"deep"},{"name":"x"}]}]"#,
        "",
    );
    assert!(
        codes_of(&s).contains(&"def/initial_is_history"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(r#"[{"name":"a"}]"#, r#"{"from":"a","on":"e","to":"nope"}"#);
    assert!(
        codes_of(&s).contains(&"def/unknown_state"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(r#"[{"name":"a"}]"#, r#"{"from":"a","on":"nope","to":"a"}"#);
    assert!(
        codes_of(&s).contains(&"def/unknown_event"),
        "{:?}",
        codes_of(&s)
    );

    let s = format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"e","fields":[]}}],"effects":[],"transitions":[{{"from":"a","on":"e","emit":[{{"effect":"nope","args":{{}}}}]}}]}}"#
    );
    assert!(
        codes_of(&s).contains(&"def/unknown_effect"),
        "{:?}",
        codes_of(&s)
    );

    let s = format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"r","ty":{{"enum":"Missing"}},"init":"x"}}],"events":[],"transitions":[]}}"#
    );
    assert!(
        codes_of(&s).contains(&"def/unknown_enum"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(
        r#"[{"name":"a","terminal":true,"states":[{"name":"b"}],"initial":"b"}]"#,
        "",
    );
    assert!(
        codes_of(&s).contains(&"def/terminal_not_leaf"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(
        r#"[{"name":"a","terminal":true}]"#,
        r#"{"from":"a","on":"e","to":"a"}"#,
    );
    assert!(
        codes_of(&s).contains(&"def/terminal_has_transitions")
            || codes_of(&s).contains(&"def/initial_terminal"),
        "{:?}",
        codes_of(&s)
    );

    let s = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"done","terminal":true}],"initial":"done","context":[],"events":[],"transitions":[]}"#;
    assert!(
        codes_of(s).contains(&"def/initial_terminal"),
        "{:?}",
        codes_of(s)
    );

    let s = wrap(
        r#"[{"name":"a"},{"name":"c","initial":"x","states":[{"name":"h1","history":"deep"},{"name":"h2","history":"shallow"},{"name":"x"}]}]"#,
        "",
    );
    assert!(
        codes_of(&s).contains(&"def/multiple_history"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(
        r#"[{"name":"a"},{"name":"c","initial":"x","states":[{"name":"h","history":"deep"},{"name":"x"}]}]"#,
        r#"{"from":"h","on":"e","to":"a"}"#,
    );
    assert!(
        codes_of(&s).contains(&"def/from_history"),
        "{:?}",
        codes_of(&s)
    );

    let s = wrap(
        r#"[{"name":"c","initial":"x","states":[{"name":"h","history":"deep"},{"name":"x"}]}]"#,
        r#"{"from":"x","on":"e","to":"h"}"#,
    );
    assert!(
        codes_of(&s).contains(&"def/history_target_from_inside"),
        "{:?}",
        codes_of(&s)
    );

    let s = format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}}],"initial":"a","context":[],"events":[{{"name":"$e","fields":[]}}],"transitions":[]}}"#
    );
    assert!(
        codes_of(&s).contains(&"def/reserved_ident"),
        "{:?}",
        codes_of(&s)
    );

    let s = format!(
        r#"{{"format":"fsm.machine/1","name":"m","regions":[],"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
    );
    let cs = parse_s_res(&s).err().unwrap_or_default();
    assert!(cs.contains(&"def/shape"), "{cs:?}");

    let s = format!(
        r#"{{"format":"fsm.machine/1","name":"m","deadlines":[],"states":[{{"name":"a"}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
    );
    assert!(parse_s_res(&s).is_ok());
}

#[test]
fn finding_quality() {
    let s = wrap(r#"[{"name":"a"}]"#, r#"{"from":"a","on":"e","to":"nope"}"#);
    let spec = parse_s(&s);
    let errs = validate(&spec).unwrap_err();
    let f = errs.iter().find(|e| e.code == "def/unknown_state").unwrap();
    assert_eq!(f.path, "/transitions/0/to");
    assert!(!f.hint.is_empty());
}

#[test]
fn limit_sets_and_generated_bytes() {
    let sets = (0..33)
        .map(|i| format!(r#"{{"target":"x","value":"{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let s = format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}}],"initial":"a","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","do":[{sets}]}}]}}"#
    );
    assert!(
        codes_of(&s).contains(&"def/limit_sets"),
        "{:?}",
        codes_of(&s)
    );

    let mut big = vec![b'{'; 1];
    big.extend(std::iter::repeat(b'x').take(256 * 1024 + 10));
    let r = load_machine_json(&big);
    assert!(r.unwrap_err().iter().any(|f| f.code == "def/limit_bytes"));
}

#[test]
fn fixtures_dir_named() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/machines/invalid"
    );
    let rd = std::fs::read_dir(dir);
    if rd.is_err() {
        return;
    }
    for entry in rd.unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            panic!("unknown fixture {}", path.display());
        }
        let stem = path.file_stem().unwrap().to_string_lossy();
        let code = stem.replacen('_', "/", 1);
        let bytes = std::fs::read(&path).unwrap();
        let spec = load_machine_json(&bytes).unwrap_or_else(|e| panic!("{stem} parse {e:?}"));
        let errs = validate(&spec).expect_err("should be invalid");
        assert!(errs.iter().any(|e| e.code == code), "{stem} got {:?}", errs);
        assert_eq!(errs.iter().filter(|e| e.code == code).count(), 1);
    }
}

// Plan 0009 task 4302: an eventless transition is the one construct that can
// fire without anybody asking, so the rules that keep it honest — no `evt`,
// no terminal source, no silent shadowing — belong at admission.

fn findings_of(src: &str) -> Vec<fsm_core::spec::Finding> {
    let spec = parse_s(src);
    validate(&spec).err().unwrap_or_default()
}

fn analysis_of(src: &str) -> Vec<fsm_core::spec::Finding> {
    let spec = parse_s(src);
    let compiled = fsm_core::spec::compile(spec).unwrap_or_else(|e| panic!("{e:?}"));
    let tree = fsm_core::tree::Tree::for_machine(&compiled.spec);
    fsm_core::analyze::analyze_all(&compiled, &tree)
}

fn reactive(transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"a"}},{{"name":"b"}},{{"name":"t","terminal":true}}],"initial":"a","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[{{"name":"amount","ty":"int"}}]}}],"effects":[{{"name":"fx","fields":[{{"name":"v","ty":"int"}}]}}],"transitions":[{transitions}]}}"#
    )
}

#[test]
fn an_eventless_guard_may_not_read_evt() {
    let findings = findings_of(&reactive(r#"{"from":"a","if":"evt.amount > 0","to":"b"}"#));
    let finding = findings
        .iter()
        .find(|f| f.code == "def/eventless_evt")
        .expect("def/eventless_evt");
    assert_eq!(finding.path, "/transitions/0/if");
    let span = finding.span.expect("span covers the reference");
    assert_eq!((span.start, span.end), (0, 10), "the span is `evt.amount`");
    assert!(finding.hint.contains("ctx"), "{}", finding.hint);
    // The same guard on an evented transition is accepted.
    assert!(
        codes_of(&reactive(
            r#"{"from":"a","on":"e","if":"evt.amount > 0","to":"b"}"#
        ))
        .is_empty()
    );
}

#[test]
fn an_eventless_block_may_not_read_evt_but_may_read_ctx() {
    let bad = reactive(
        r#"{"from":"a","to":"b","do":[{"target":"x","value":"evt.amount"}],"emit":[{"effect":"fx","args":{"v":"evt.amount + 1"}}]}"#,
    );
    let findings = findings_of(&bad);
    let paths: Vec<&str> = findings
        .iter()
        .filter(|f| f.code == "def/eventless_evt")
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        paths,
        ["/transitions/0/do/0/value", "/transitions/0/emit/0/args/v"]
    );
    let good = reactive(
        r#"{"from":"a","to":"b","do":[{"target":"x","value":"ctx.x + 1"}],"emit":[{"effect":"fx","args":{"v":"ctx.x"}}]}"#,
    );
    assert!(codes_of(&good).is_empty(), "{:?}", codes_of(&good));
}

#[test]
fn an_eventless_transition_from_a_terminal_state_has_its_own_code() {
    let eventless = codes_of(&reactive(r#"{"from":"t","to":"a"}"#));
    assert!(
        eventless.contains(&"def/eventless_from_terminal"),
        "{eventless:?}"
    );
    assert!(
        !eventless.contains(&"def/terminal_has_transitions"),
        "{eventless:?}"
    );
    let evented = codes_of(&reactive(r#"{"from":"t","on":"e","to":"a"}"#));
    assert!(
        evented.contains(&"def/terminal_has_transitions"),
        "{evented:?}"
    );
    assert!(
        !evented.contains(&"def/eventless_from_terminal"),
        "{evented:?}"
    );
}

#[test]
fn a_guardless_eventless_transition_shadows_its_later_siblings() {
    let shadowed = analysis_of(&reactive(
        r#"{"from":"a","to":"b"},{"from":"a","if":"ctx.x > 0","to":"b"}"#,
    ));
    let finding = shadowed
        .iter()
        .find(|f| f.code == "def/eventless_shadowed")
        .expect("def/eventless_shadowed");
    assert_eq!(finding.path, "/transitions/0");
    assert_eq!(
        finding.hint, "indices 0 then 1",
        "the same shape as def/shadowed"
    );
    assert!(!shadowed.iter().any(|f| f.code == "def/shadowed"));
    let reversed = analysis_of(&reactive(
        r#"{"from":"a","if":"ctx.x > 0","to":"b"},{"from":"a","to":"b"}"#,
    ));
    assert!(
        !reversed.iter().any(|f| f.code == "def/eventless_shadowed"),
        "{reversed:?}"
    );
}

#[test]
fn an_eventless_transition_that_can_only_burn_a_microstep_is_a_warning() {
    let src = reactive(r#"{"from":"a","on":"e","to":"b"},{"from":"b","if":"ctx.x > 0"}"#);
    assert!(
        codes_of(&src).is_empty(),
        "still accepted: {:?}",
        codes_of(&src)
    );
    let finding = analysis_of(&src)
        .into_iter()
        .find(|f| f.code == "def/eventless_internal_noop")
        .expect("def/eventless_internal_noop");
    assert_eq!(finding.severity, fsm_core::spec::Severity::Warning);
    assert_eq!(finding.path, "/transitions/1");
    for doing_something in [
        r#"{"from":"a","on":"e","to":"b"},{"from":"b","if":"ctx.x > 0","to":"a"}"#,
        r#"{"from":"a","on":"e","to":"b"},{"from":"b","if":"ctx.x > 0","do":[{"target":"x","value":"0"}]}"#,
        r#"{"from":"a","on":"e","to":"b"},{"from":"b","if":"ctx.x > 0","emit":[{"effect":"fx","args":{"v":"1"}}]}"#,
    ] {
        let findings = analysis_of(&reactive(doing_something));
        assert!(
            !findings
                .iter()
                .any(|f| f.code == "def/eventless_internal_noop"),
            "{findings:?}"
        );
    }
}

#[test]
fn identical_eventless_guards_reuse_duplicate_guard() {
    let findings = analysis_of(&reactive(
        r#"{"from":"a","if":"ctx.x > 0","to":"b"},{"from":"a","if":"ctx.x > 0","to":"b"}"#,
    ));
    assert!(
        findings.iter().any(|f| f.code == "def/duplicate_guard"),
        "{findings:?}"
    );
}

#[test]
fn eventless_findings_are_stable_and_non_eventless_findings_are_unchanged() {
    let malformed = reactive(
        r#"{"from":"t","if":"evt.amount > 0","to":"nowhere"},{"from":"a","on":"nope","to":"b"}"#,
    );
    let first = codes_of(&malformed);
    let second = codes_of(&malformed);
    assert_eq!(first, second);
    assert_eq!(
        first,
        [
            "def/unknown_state",
            "def/unknown_event",
            "def/eventless_evt",
            "def/eventless_from_terminal",
        ],
        "structural findings first, per transition in document order, then the reactive ones"
    );
    // A definition with no eventless transition reports exactly what it did
    // before the reactive rules existed.
    let evented =
        reactive(r#"{"from":"t","on":"e","to":"nowhere"},{"from":"a","on":"nope","to":"b"}"#);
    assert_eq!(
        codes_of(&evented),
        [
            "def/terminal_has_transitions",
            "def/unknown_state",
            "def/unknown_event",
        ]
    );
}
