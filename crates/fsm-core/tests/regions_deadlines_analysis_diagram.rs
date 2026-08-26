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
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
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

// ---- Plan 0009 task 4702: the reactive surface, analysed and drawn.

/// Two compounds owning a final child and two regions — every kind of
/// generated name at once — plus a guarded eventless edge, an internal
/// event, and terminal states to tell apart from the final ones. Region b's
/// own done event has no handler.
fn reactive_parallel() -> CompiledMachine {
    compile_json(
        r#"{"format":"fsm.machine/1","name":"reactive_parallel","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true},{"name":"stop","fields":[]}],"regions":[{"name":"a","initial":"review","states":[{"name":"review","initial":"open","states":[{"name":"open"},{"name":"approved","final":true}]},{"name":"a_done","terminal":true}]},{"name":"b","initial":"audit","states":[{"name":"audit","initial":"pending","states":[{"name":"pending"},{"name":"checked","final":true}]},{"name":"joined"},{"name":"b_done","terminal":true}]}],"transitions":[{"from":"open","on":"go","to":"approved"},{"from":"review","on":"$done.state.review","to":"a_done"},{"from":"pending","if":"ctx.n > 0","to":"checked"},{"from":"pending","on":"stop","to":"checked"},{"from":"audit","on":"$done.state.audit","to":"joined"},{"from":"joined","on":"$done.region.a","to":"b_done"}]}"#,
    )
}

#[test]
fn analysis_reports_the_reactive_surface() {
    let machine = reactive_parallel();
    let tree = Tree::for_machine(&machine.spec);
    let summary = fsm_core::analyze::reactive_summary(&machine, &tree);
    assert_eq!(summary.eventless_transitions, 1);
    assert!(
        summary.eventless_findings.is_empty(),
        "{:?}",
        summary.eventless_findings
    );
    assert_eq!(
        summary.done_events,
        ["$done.state.review", "$done.state.audit", "$done.region.a"]
    );
    assert_eq!(summary.unhandled_done_events, ["$done.region.b"]);
    assert_eq!(summary.internal_events, ["tick"]);
    // A plain parallel machine raises nothing; it could handle its region
    // names, and that is the one place they are discoverable.
    let (plain, plain_tree) = parallel_machine();
    let summary = fsm_core::analyze::reactive_summary(&plain, &plain_tree);
    assert_eq!(summary.eventless_transitions, 0);
    assert!(summary.done_events.is_empty());
    assert_eq!(
        summary.unhandled_done_events,
        ["$done.region.work", "$done.region.audit"]
    );
    assert!(summary.internal_events.is_empty());
}

#[test]
fn eventless_edges_are_drawn_as_such_in_both_formats() {
    let machine = reactive_parallel();
    let mermaid_output = mermaid(&machine, None);
    assert!(
        mermaid_output.contains("  pending --> checked: [ctx.n > 0] (eventless)\n"),
        "{mermaid_output}"
    );
    assert!(
        mermaid_output.contains("  pending --> checked: stop\n"),
        "{mermaid_output}"
    );
    let dot_output = dot(&machine, None);
    assert!(
        dot_output.contains("  pending -> checked [label=\"[ctx.n > 0]\" style=dashed];\n"),
        "{dot_output}"
    );
    assert!(
        dot_output.contains("  pending -> checked [label=\"stop\"];\n"),
        "{dot_output}"
    );
}

#[test]
fn final_states_are_drawn_apart_from_terminal_ones() {
    let machine = reactive_parallel();
    let mermaid_output = mermaid(&machine, None);
    assert!(
        mermaid_output.contains("approved : <<final>>\n"),
        "{mermaid_output}"
    );
    assert!(
        mermaid_output.contains("a_done --> [*]\n"),
        "{mermaid_output}"
    );
    assert!(
        !mermaid_output.contains("approved --> [*]"),
        "a final state does not end the machine: {mermaid_output}"
    );
    let dot_output = dot(&machine, None);
    assert!(
        dot_output.contains("approved [ shape=doublecircle];\n"),
        "{dot_output}"
    );
    assert!(dot_output.contains("a_done [];\n"), "{dot_output}");
}

#[test]
fn done_event_labels_survive_escaping() {
    let machine = reactive_parallel();
    let mermaid_output = mermaid(&machine, None);
    assert!(
        mermaid_output.contains("  joined --> b_done: $done.region.a\n"),
        "{mermaid_output}"
    );
    assert!(
        mermaid_output.contains("  review --> a_done: $done.state.review\n"),
        "{mermaid_output}"
    );
    let dot_output = dot(&machine, None);
    assert!(
        dot_output.contains("  joined -> b_done [label=\"$done.region.a\"];\n"),
        "{dot_output}"
    );
    assert!(
        dot_output.contains("  review -> a_done [label=\"$done.state.review\"];\n"),
        "{dot_output}"
    );
}
