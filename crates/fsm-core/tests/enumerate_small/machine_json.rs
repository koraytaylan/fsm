use super::trees::{InitialChoice, Named};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BlockCase {
    None,
    SetOne,
    Increment,
    Emit,
    IncrementAndEmit,
}

pub(super) const TRANSITION_BLOCKS: [BlockCase; 5] = [
    BlockCase::None,
    BlockCase::SetOne,
    BlockCase::Increment,
    BlockCase::Emit,
    BlockCase::IncrementAndEmit,
];
pub(super) const STATE_BLOCKS: [BlockCase; 4] = [
    BlockCase::SetOne,
    BlockCase::Increment,
    BlockCase::Emit,
    BlockCase::IncrementAndEmit,
];

pub(super) fn block_members(case: BlockCase) -> Vec<&'static str> {
    match case {
        BlockCase::None => Vec::new(),
        BlockCase::SetOne => vec![r#""do":[{"target":"n","value":"1"}]"#],
        BlockCase::Increment => {
            vec![r#""do":[{"target":"n","value":"ctx.n + 1"}]"#]
        }
        BlockCase::Emit => {
            vec![r#""emit":[{"effect":"fx","args":{"v":"ctx.n"}}]"#]
        }
        BlockCase::IncrementAndEmit => vec![
            r#""do":[{"target":"n","value":"ctx.n + 1"}]"#,
            r#""emit":[{"effect":"fx","args":{"v":"ctx.n"}}]"#,
        ],
    }
}

pub(super) fn block_json(case: BlockCase) -> String {
    format!("{{{}}}", block_members(case).join(","))
}

#[derive(Clone, Debug, Default)]
pub(super) struct Decorations {
    pub(super) entry: Option<(String, BlockCase)>,
    pub(super) exit: Option<(String, BlockCase)>,
}

#[derive(Clone, Debug)]
pub(super) struct HistoryPlacement {
    pub(super) owner: String,
    pub(super) name: String,
    pub(super) kind: &'static str,
}

pub(super) fn emit_states(
    nodes: &[Named],
    choice: &InitialChoice,
    decorations: &Decorations,
    history: Option<&HistoryPlacement>,
) -> String {
    let rendered: Vec<String> = nodes
        .iter()
        .map(|node| {
            let mut fields = vec![format!(r#""name":"{}""#, node.name)];
            if !node.kids.is_empty() {
                fields.push(format!(
                    r#""initial":"{}""#,
                    choice
                        .children
                        .get(&node.name)
                        .expect("compound initial is selected")
                ));
                let mut children = Vec::new();
                if let Some(history) = history.filter(|history| history.owner == node.name) {
                    children.push(format!(
                        r#"{{"name":"{}","history":"{}"}}"#,
                        history.name, history.kind
                    ));
                }
                let nested = emit_states(&node.kids, choice, decorations, history);
                if nested.len() > 2 {
                    children.push(nested[1..nested.len() - 1].to_string());
                }
                fields.push(format!(r#""states":[{}]"#, children.join(",")));
            }
            if let Some((name, block)) = &decorations.entry
                && name == &node.name
            {
                fields.push(format!(r#""entry":{}"#, block_json(*block)));
            }
            if let Some((name, block)) = &decorations.exit
                && name == &node.name
            {
                fields.push(format!(r#""exit":{}"#, block_json(*block)));
            }
            format!("{{{}}}", fields.join(","))
        })
        .collect();
    format!("[{}]", rendered.join(","))
}

pub(super) fn transition_json(
    from: &str,
    on: &str,
    to: Option<&str>,
    guard: Option<&str>,
    block: BlockCase,
) -> String {
    let mut fields = vec![format!(r#""from":"{from}""#), format!(r#""on":"{on}""#)];
    if let Some(to) = to {
        fields.push(format!(r#""to":"{to}""#));
    }
    if let Some(guard) = guard {
        fields.push(format!(r#""if":"{guard}""#));
    }
    fields.extend(block_members(block).into_iter().map(str::to_string));
    format!("{{{}}}", fields.join(","))
}

pub(super) fn machine_json(
    states: &str,
    initial: &str,
    event_names: &[&str],
    transitions: &[String],
) -> String {
    let events: Vec<String> = event_names
        .iter()
        .map(|name| format!(r#"{{"name":"{name}","fields":[]}}"#))
        .collect();
    format!(
        r#"{{"format":"fsm.machine/1","name":"g","states":{states},"initial":"{initial}","context":[{{"name":"b","ty":"bool","init":"true"}},{{"name":"n","ty":"int","init":"0"}}],"events":[{}],"effects":[{{"name":"fx","fields":[{{"name":"v","ty":"int"}}]}}],"transitions":[{}],"invariants":[{{"name":"nneg","expr":"ctx.n >= 0","mode":"enforce"}}]}}"#,
        events.join(","),
        transitions.join(",")
    )
}

pub(super) fn parallel_machine_json(selection_transitions: &[String]) -> String {
    let mut transitions = selection_transitions.to_vec();
    transitions.push(transition_json(
        "a",
        "finish_a",
        Some("af"),
        None,
        BlockCase::Emit,
    ));
    transitions.push(transition_json(
        "b",
        "finish_b",
        Some("bf"),
        None,
        BlockCase::Emit,
    ));
    let events = ["choose", "finish_a", "finish_b", "idle"]
        .map(|name| format!(r#"{{"name":"{name}","fields":[]}}"#))
        .join(",");
    format!(
        r#"{{"format":"fsm.machine/1","name":"parallel_generated","regions":[{{"name":"alpha","states":[{{"name":"a","initial":"a0","entry":{{"do":[{{"target":"n","value":"ctx.n + 1"}}],"emit":[{{"effect":"fx","args":{{"v":"ctx.n"}}}}]}},"states":[{{"name":"a0","entry":{{"do":[{{"target":"n","value":"ctx.n + 1"}}]}}}},{{"name":"a1"}},{{"name":"af","terminal":true}}]}}],"initial":"a"}},{{"name":"beta","states":[{{"name":"b","initial":"b0","entry":{{"do":[{{"target":"n","value":"ctx.n + 1"}}],"emit":[{{"effect":"fx","args":{{"v":"ctx.n"}}}}]}},"states":[{{"name":"b0","entry":{{"do":[{{"target":"n","value":"ctx.n + 1"}}]}}}},{{"name":"b1"}},{{"name":"bf","terminal":true}}]}}],"initial":"b"}}],"context":[{{"name":"b","ty":"bool","init":"true"}},{{"name":"n","ty":"int","init":"0"}}],"events":[{events}],"effects":[{{"name":"fx","fields":[{{"name":"v","ty":"int"}}]}}],"on_unhandled":"ignore","transitions":[{}],"invariants":[{{"name":"nneg","expr":"ctx.n >= 0","mode":"enforce"}}]}}"#,
        transitions.join(",")
    )
}

#[derive(Clone, Copy)]
pub(super) struct ParallelSelectionRow {
    pub(super) source: &'static str,
    pub(super) target: Option<&'static str>,
    pub(super) guard: &'static str,
}

pub(super) struct ParallelWinnerCase {
    pub(super) name: &'static str,
    pub(super) rows: &'static [ParallelSelectionRow],
    pub(super) expected_region: &'static str,
    pub(super) expected_source: &'static str,
    pub(super) expected_alpha: &'static str,
    pub(super) expected_beta: &'static str,
}

pub(super) const PARALLEL_WINNER_CASES: &[ParallelWinnerCase] = &[
    ParallelWinnerCase {
        name: "region order overrides transition document order",
        rows: &[
            ParallelSelectionRow {
                source: "b0",
                target: Some("b1"),
                guard: "ctx.b",
            },
            ParallelSelectionRow {
                source: "a0",
                target: Some("a1"),
                guard: "ctx.b",
            },
        ],
        expected_region: "alpha",
        expected_source: "a0",
        expected_alpha: "a1",
        expected_beta: "b0",
    },
    ParallelWinnerCase {
        name: "later region fallback",
        rows: &[
            ParallelSelectionRow {
                source: "a0",
                target: Some("a1"),
                guard: "false",
            },
            ParallelSelectionRow {
                source: "b0",
                target: Some("b1"),
                guard: "ctx.b",
            },
        ],
        expected_region: "beta",
        expected_source: "b0",
        expected_alpha: "a0",
        expected_beta: "b1",
    },
    ParallelWinnerCase {
        name: "same-state document order",
        rows: &[
            ParallelSelectionRow {
                source: "a0",
                target: None,
                guard: "false",
            },
            ParallelSelectionRow {
                source: "a0",
                target: None,
                guard: "ctx.b",
            },
            ParallelSelectionRow {
                source: "b0",
                target: Some("b1"),
                guard: "ctx.b",
            },
        ],
        expected_region: "alpha",
        expected_source: "a0",
        expected_alpha: "a0",
        expected_beta: "b0",
    },
    ParallelWinnerCase {
        name: "ancestor before later region",
        rows: &[
            ParallelSelectionRow {
                source: "a0",
                target: Some("a1"),
                guard: "false",
            },
            ParallelSelectionRow {
                source: "a",
                target: Some("a1"),
                guard: "ctx.b",
            },
            ParallelSelectionRow {
                source: "b0",
                target: Some("b1"),
                guard: "ctx.b",
            },
        ],
        expected_region: "alpha",
        expected_source: "a",
        expected_alpha: "a1",
        expected_beta: "b0",
    },
    ParallelWinnerCase {
        name: "leaf before ancestor",
        rows: &[
            ParallelSelectionRow {
                source: "a0",
                target: None,
                guard: "ctx.b",
            },
            ParallelSelectionRow {
                source: "a",
                target: Some("a1"),
                guard: "ctx.b",
            },
            ParallelSelectionRow {
                source: "b0",
                target: Some("b1"),
                guard: "ctx.b",
            },
        ],
        expected_region: "alpha",
        expected_source: "a0",
        expected_alpha: "a0",
        expected_beta: "b0",
    },
    ParallelWinnerCase {
        name: "later-region ancestor",
        rows: &[
            ParallelSelectionRow {
                source: "a",
                target: Some("a1"),
                guard: "not ctx.b",
            },
            ParallelSelectionRow {
                source: "b",
                target: Some("b1"),
                guard: "ctx.b",
            },
        ],
        expected_region: "beta",
        expected_source: "b",
        expected_alpha: "a0",
        expected_beta: "b1",
    },
];
