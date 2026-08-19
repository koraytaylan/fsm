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
