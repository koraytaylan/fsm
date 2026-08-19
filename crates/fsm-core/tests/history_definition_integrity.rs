//! Hostile history shapes are rejected before tree construction or runtime.

use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::{Finding, compile_accepted};

fn compile_findings(source: &[u8]) -> Vec<Finding> {
    let value = parse(source, &JsonLimits::DEFAULT).expect("hostile definition is JSON");
    compile_accepted(&value).expect_err("hostile history definition must be rejected")
}

#[test]
fn ownerless_history_event_and_deadline_targets_are_rejected_before_runtime() {
    let source = br#"{
      "format":"fsm.machine/1",
      "name":"ownerless_history",
      "states":[{"name":"a"},{"name":"h","history":"deep"},{"name":"b"}],
      "initial":"a",
      "context":[],
      "events":[{"name":"go","fields":[]}],
      "transitions":[{"from":"a","on":"go","to":"h"}],
      "deadlines":[{"name":"timeout","from":"a","after":"dur(1, ms)","to":"h"}]
    }"#;

    let value = parse(source, &JsonLimits::DEFAULT).expect("hostile definition is JSON");
    let result = std::panic::catch_unwind(|| compile_accepted(&value));
    let compiled = result.expect("ownerless history admission must not panic");
    let findings = compiled.expect_err("ownerless history must not reach runtime");
    let shape_paths = findings
        .iter()
        .filter(|finding| finding.code == "def/shape")
        .map(|finding| finding.path.as_str())
        .collect::<Vec<_>>();
    assert!(shape_paths.contains(&"/states/h/history"), "{findings:?}");
    assert!(shape_paths.contains(&"/transitions/0/to"), "{findings:?}");
    assert!(shape_paths.contains(&"/deadlines/0/to"), "{findings:?}");
    assert!(
        findings
            .iter()
            .filter(|finding| finding.code == "def/shape")
            .all(|finding| !finding.hint.is_empty())
    );
}

#[test]
fn a_history_pseudostate_cannot_own_another_history_pseudostate() {
    let source = br#"{
      "format":"fsm.machine/1","name":"nested_history_owner",
      "states":[{"name":"a"},{"name":"owner","initial":"real","states":[
        {"name":"outer_history","history":"deep","states":[
          {"name":"inner_history","history":"shallow"}
        ]},
        {"name":"real"}
      ]}],
      "initial":"a","context":[],"events":[],"transitions":[]
    }"#;

    let findings = compile_findings(source);
    let inner = findings
        .iter()
        .find(|finding| {
            finding.code == "def/shape" && finding.path == "/states/inner_history/history"
        })
        .unwrap_or_else(|| panic!("{findings:?}"));
    assert!(inner.message.contains("compound owner"));
}

#[test]
fn history_pseudostates_are_leaf_like_and_do_not_have_initials() {
    let cases: &[(&str, &[u8])] = &[
        (
            "children",
            br#"{
              "format":"fsm.machine/1","name":"history_with_children",
              "states":[{"name":"a"},{"name":"owner","initial":"real","states":[
                {"name":"h","history":"shallow","states":[{"name":"nested"}]},
                {"name":"real"}
              ]}],
              "initial":"a","context":[],"events":[],"transitions":[]
            }"#,
        ),
        (
            "terminal",
            br#"{
              "format":"fsm.machine/1","name":"terminal_history",
              "states":[{"name":"a"},{"name":"owner","initial":"real","states":[
                {"name":"h","history":"deep","terminal":true},{"name":"real"}
              ]}],
              "initial":"a","context":[],"events":[],"transitions":[]
            }"#,
        ),
        (
            "initial",
            br#"{
              "format":"fsm.machine/1","name":"history_with_initial",
              "states":[{"name":"a"},{"name":"owner","initial":"real","states":[
                {"name":"h","history":"deep","initial":"ghost"},{"name":"real"}
              ]}],
              "initial":"a","context":[],"events":[],"transitions":[]
            }"#,
        ),
    ];

    for (label, source) in cases {
        let findings = compile_findings(source);
        let finding = findings
            .iter()
            .find(|finding| finding.code == "def/shape" && finding.path == "/states/h")
            .unwrap_or_else(|| panic!("{label}: {findings:?}"));
        assert!(
            finding.message.contains("childless"),
            "{label}: {finding:?}"
        );
        assert!(!finding.hint.is_empty(), "{label}: {finding:?}");
    }
}
