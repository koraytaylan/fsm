//! Hostile accepted identifiers stay data, never Mermaid or DOT grammar.

use std::collections::BTreeSet;

use fsm_core::diagram::{InstanceOverlay, dot, mermaid};
use fsm_core::json::{JsonLimits, parse};
use fsm_core::spec::compile_accepted;

fn hostile_machine() -> fsm_core::machine::CompiledMachine {
    let source = br#"{
        "format":"fsm.machine/1",
        "name":"hostile diagram",
        "context":[],
        "events":[{"name":"go\";\n%%{init: {'theme':'dark'}}%%\u0000\t","fields":[]}],
        "regions":[
            {
                "name":"region \";\nstate INJECT",
                "initial":"__start",
                "states":[
                    {"name":"__start"},
                    {"name":"done\";\nINJECT","terminal":true},
                    {"name":"STYLE"},
                    {"name":"click"},
                    {"name":"scale"},
                    {"name":"stateDiagram"},
                    {"name":"Graph"},
                    {"name":"HREF"},
                    {"name":"default"},
                    {"name":"_fsm_state_1"}
                ]
            },
            {
                "name":"audit region",
                "initial":"audit wait",
                "states":[
                    {"name":"audit wait"},
                    {"name":"audit done","terminal":true}
                ]
            }
        ],
        "transitions":[
            {"from":"__start","on":"go\";\n%%{init: {'theme':'dark'}}%%\u0000\t","to":"done\";\nINJECT"}
        ],
        "deadlines":[
            {
                "name":"timer\";\nINJECT",
                "from":"audit wait",
                "after":"dur(1, ms)",
                "to":"audit done"
            }
        ]
    }"#;
    let value = parse(source, &JsonLimits::DEFAULT).expect("hostile JSON parses");
    compile_accepted(&value).expect("hostile identifiers remain valid machine data")
}

#[test]
fn exporters_alias_and_escape_hostile_identifiers() {
    let machine = hostile_machine();
    let overlay = InstanceOverlay {
        current_leaves: BTreeSet::from(["done\";\nINJECT".to_string()]),
        visited: BTreeSet::from([
            "audit wait".to_string(),
            "unknown\nclass INJECT".to_string(),
        ]),
    };

    let mermaid_output = mermaid(&machine, Some(&overlay));
    assert!(mermaid_output.contains("state \"region #34;#59; state INJECT\" as $region_0 {"));
    assert!(mermaid_output.contains("state \"done#34;#59; INJECT\" as _fsm_state_1_"));
    assert!(mermaid_output.contains(
        "__start --> _fsm_state_1_: go#34;#59; #37;#37;{init#58; {'theme'#58;'dark'}}#37;#37;  "
    ));
    assert!(mermaid_output.contains("state \"STYLE\" as _fsm_state_2"));
    assert!(mermaid_output.contains("state \"click\" as _fsm_state_3"));
    assert!(mermaid_output.contains("state \"scale\" as _fsm_state_4"));
    assert!(mermaid_output.contains("state \"stateDiagram\" as _fsm_state_5"));
    assert!(mermaid_output.contains("state \"Graph\" as _fsm_state_6"));
    assert!(mermaid_output.contains("state \"HREF\" as _fsm_state_7"));
    assert!(mermaid_output.contains("state \"default\" as _fsm_state_8"));
    assert!(mermaid_output.contains("  _fsm_state_1\n"));
    assert!(mermaid_output.contains("class _fsm_state_1_ current"));
    assert!(!mermaid_output.contains("\nINJECT"));
    assert!(!mermaid_output.contains("class INJECT"));
    assert!(!mermaid_output.contains("%%{"));
    assert!(!mermaid_output.contains(['\0', '\t']));

    let dot_output = dot(&machine, Some(&overlay));
    assert!(dot_output.contains("\"$start\" [shape=point]"));
    assert!(
        dot_output.contains(
            "__start -> _fsm_state_1_ [label=\"go\\\"; %%{init: {'theme':'dark'}}%%  \"]"
        )
    );
    assert!(dot_output.contains("_fsm_state_1_ [label=\"done\\\"; INJECT\" style=bold]"));
    assert!(!dot_output.contains("\nINJECT"));
    assert!(!dot_output.contains(['\0', '\t']));
}

/// A join on a region whose name is hostile: the generated event name carries
/// the hostile bytes through the same escaping as any other label, and the
/// `$` prefix survives it. A state whose name mimics the generated name minus
/// the prefix is aliased and escaped like any other name, so the two cannot
/// be confused — `def/reserved_ident` keeps a real collision impossible.
#[test]
fn generated_event_labels_take_the_escaping_path_with_the_prefix_intact() {
    let source = br#"{"format":"fsm.machine/1","name":"hostile join","context":[],"events":[{"name":"go","fields":[]}],"regions":[{"name":"b \";\nINJECT","initial":"b_work","states":[{"name":"b_work"},{"name":"b_done","terminal":true}]},{"name":"a","initial":"waiting","states":[{"name":"waiting"},{"name":"proceed"},{"name":"done.region.b \";\nINJECT"}]}],"transitions":[{"from":"b_work","on":"go","to":"b_done"},{"from":"waiting","on":"$done.region.b \";\nINJECT","to":"proceed"},{"from":"proceed","on":"go","to":"done.region.b \";\nINJECT"}]}"#;
    let value = parse(source, &JsonLimits::DEFAULT).expect("hostile JSON parses");
    let machine = compile_accepted(&value).expect("hostile identifiers remain valid machine data");

    let mermaid_output = mermaid(&machine, None);
    assert!(
        mermaid_output.contains("  waiting --> proceed: $done.region.b #34;#59; INJECT\n"),
        "{mermaid_output}"
    );
    assert!(
        mermaid_output.contains("state \"done.region.b #34;#59; INJECT\" as _fsm_state_"),
        "{mermaid_output}"
    );
    assert!(
        !mermaid_output.contains("state \"$done"),
        "{mermaid_output}"
    );
    assert!(!mermaid_output.contains("\nINJECT"));

    let dot_output = dot(&machine, None);
    assert!(
        dot_output.contains("  waiting -> proceed [label=\"$done.region.b \\\"; INJECT\"];\n"),
        "{dot_output}"
    );
    assert!(!dot_output.contains("\nINJECT"));
}
