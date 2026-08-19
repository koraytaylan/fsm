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
