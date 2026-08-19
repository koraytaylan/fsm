//! Analysis and diagram contract tests for region-aware machines.
//!
//! Reachability and enabled-event scans must traverse every region, while DOT
//! and Mermaid preserve sequential output and render parallel topology clearly.

use std::collections::BTreeMap;

use fsm_core::analyze::{EventStatus, completeness_matrix, enabled_events, reachability_findings};
use fsm_core::diagram::{dot, mermaid};
use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::spec::{compile, parse_machine};
use fsm_core::tree::Tree;

fn compile_json(source: &str) -> CompiledMachine {
    let value = parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    compile(parse_machine(&value).unwrap()).unwrap()
}

fn parallel_machine() -> (CompiledMachine, Tree) {
    let machine = compile_json(
        r#"{
            "format":"fsm.machine/1",
            "name":"parallel",
            "regions":[
                {
                    "name":"work",
                    "states":[
                        {
                            "name":"w_flow",
                            "states":[
                                {"name":"w_idle"},
                                {"name":"w_done","terminal":true}
                            ],
                            "initial":"w_idle"
                        },
                        {"name":"ghost"}
                    ],
                    "initial":"w_flow"
                },
                {
                    "name":"audit",
                    "states":[
                        {"name":"a_wait"},
                        {"name":"a_event_done","terminal":true},
                        {"name":"a_expired","terminal":true}
                    ],
                    "initial":"a_wait"
                }
            ],
            "context":[],
            "events":[{"name":"advance","fields":[]}],
            "transitions":[
                {"from":"w_flow","on":"advance","to":"w_done"},
                {"from":"a_wait","on":"advance","to":"a_event_done"}
            ],
            "deadlines":[
                {"name":"expire","from":"a_wait","after":"dur(5, s)","to":"a_expired"}
            ]
        }"#,
    );
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

fn parallel_state(work_leaf: &str) -> InstanceState {
    InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([
                ("audit".into(), "a_wait".into()),
                ("work".into(), work_leaf.into()),
            ]),
        },
        ctx: BTreeMap::new(),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
    }
}

#[test]
fn all_region_initials_and_deadline_targets_are_reachable() {
    let (machine, tree) = parallel_machine();
    let findings = reachability_findings(&machine, &tree);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, "def/unreachable_state");
    assert!(findings[0].message.contains("ghost"));
}

#[test]
fn enabled_events_scan_live_regions_in_document_order() {
    let (machine, tree) = parallel_machine();
    let mut budget = Budget::new(4096);
    let reports = enabled_events(&machine, &tree, &parallel_state("w_idle"), &mut budget);
    let advance = &reports[0];
    assert_eq!(advance.status, EventStatus::Enabled);
    assert_eq!(advance.candidates.len(), 2);
    assert_eq!(advance.candidates[0].source_state, "w_flow");
    assert_eq!(advance.candidates[0].truth, EventStatus::Enabled);
    assert_eq!(advance.candidates[1].source_state, "a_wait");
    assert_eq!(advance.candidates[1].truth, EventStatus::Preempted);

    let mut budget = Budget::new(4096);
    let reports = enabled_events(&machine, &tree, &parallel_state("w_done"), &mut budget);
    let advance = &reports[0];
    assert_eq!(advance.status, EventStatus::Enabled);
    assert_eq!(advance.candidates.len(), 1);
    assert_eq!(advance.candidates[0].source_state, "a_wait");
    assert_eq!(advance.candidates[0].truth, EventStatus::Enabled);
}

#[test]
fn completeness_treats_terminal_parallel_region_as_inert() {
    let (machine, tree) = parallel_machine();
    let matrix = completeness_matrix(&machine, &tree);

    assert_eq!(
        matrix
            .get(&("w_idle".into(), "advance".into()))
            .map(String::as_str),
        Some("handled@w_flow")
    );
    assert_eq!(
        matrix
            .get(&("w_done".into(), "advance".into()))
            .map(String::as_str),
        Some("unhandled(reject)"),
        "a terminal region must not inherit an ancestor handler"
    );
}

#[test]
fn parallel_diagrams_show_regions_initials_and_deadlines() {
    let (machine, _) = parallel_machine();
    let mermaid_output = mermaid(&machine, None);
    assert!(mermaid_output.contains("    state \"work\" as $region_0 {\n      [*] --> w_flow\n"));
    assert!(mermaid_output.contains("    state \"audit\" as $region_1 {\n      [*] --> a_wait\n"));
    assert!(mermaid_output.contains("    }\n    --\n    state \"audit\" as $region_1"));
    assert!(mermaid_output.contains("  a_wait --> a_expired: after dur(5, s) [expire]\n"));

    let dot_output = dot(&machine, None);
    assert!(dot_output.contains("  subgraph \"cluster_$region_0\" {\n    label=\"work\";\n"));
    assert!(dot_output.contains("  subgraph \"cluster_$region_1\" {\n    label=\"audit\";\n"));
    assert!(dot_output.contains("  __start -> w_flow;\n"));
    assert!(dot_output.contains("  __start -> a_wait;\n"));
    assert!(dot_output.contains("  a_wait -> a_expired [label=\"after dur(5, s) [expire]\"];\n"));
}

#[test]
fn sequential_diagrams_remain_byte_exact() {
    let machine = compile_json(
        r#"{
            "format":"fsm.machine/1",
            "name":"sequential",
            "states":[
                {"name":"a"},
                {"name":"b","terminal":true}
            ],
            "initial":"a",
            "context":[],
            "events":[{"name":"go","fields":[]}],
            "transitions":[{"from":"a","on":"go","to":"b"}]
        }"#,
    );
    assert_eq!(
        mermaid(&machine, None),
        "stateDiagram-v2\n  [*] --> a\n  a\n  b\n  b --> [*]\n  a --> b: go\n"
    );
    assert_eq!(
        dot(&machine, None),
        concat!(
            "digraph fsm {\n",
            "  a [];\n",
            "  b [];\n",
            "  __start [shape=point];\n",
            "  __start -> a;\n",
            "  a -> b [label=\"go\"];\n",
            "}\n"
        )
    );
}
