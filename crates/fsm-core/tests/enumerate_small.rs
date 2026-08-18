//! Bounded small-machine differential against the deliberately naive oracle.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

mod oracle;

fn payload() -> Value {
    Value::Obj(BTreeMap::new())
}

fn compile_src(src: &str) -> (fsm_core::machine::CompiledMachine, Tree) {
    let value = fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT)
        .unwrap_or_else(|err| panic!("generated JSON did not parse: {err:?}\n{src}"));
    let spec = parse_machine(&value)
        .unwrap_or_else(|findings| panic!("generated machine did not parse: {findings:?}\n{src}"));
    let machine = compile(spec).unwrap_or_else(|findings| {
        panic!("generated machine did not compile: {findings:?}\n{src}")
    });
    let tree = Tree::build(&machine.spec.states);
    (machine, tree)
}

#[derive(Clone, Debug)]
struct Node {
    kids: Vec<Node>,
}

fn trees(n: usize, max_depth: u32) -> Vec<Node> {
    if n == 0 || max_depth == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Node { kids: vec![] }];
    }
    forests(n - 1, max_depth - 1)
        .into_iter()
        .map(|kids| Node { kids })
        .collect()
}

fn forests(n: usize, max_depth: u32) -> Vec<Vec<Node>> {
    if n == 0 {
        return vec![vec![]];
    }
    if max_depth == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for k in 1..=n {
        for tree in trees(k, max_depth) {
            for rest in forests(n - k, max_depth) {
                let mut forest = vec![tree.clone()];
                forest.extend(rest);
                out.push(forest);
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
struct Named {
    name: String,
    kids: Vec<Named>,
}

fn name_forest(forest: &[Node]) -> Vec<Named> {
    let mut i = 0u32;
    fn walk(node: &Node, i: &mut u32) -> Named {
        let name = format!("s{i}");
        *i += 1;
        Named {
            name,
            kids: node.kids.iter().map(|child| walk(child, i)).collect(),
        }
    }
    forest.iter().map(|node| walk(node, &mut i)).collect()
}

fn find_named<'a>(nodes: &'a [Named], name: &str) -> Option<&'a Named> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = find_named(&node.kids, name) {
            return Some(found);
        }
    }
    None
}

fn collect_names(nodes: &[Named], out: &mut Vec<String>) {
    for node in nodes {
        out.push(node.name.clone());
        collect_names(&node.kids, out);
    }
}

fn collect_compounds(nodes: &[Named], out: &mut Vec<String>) {
    for node in nodes {
        if !node.kids.is_empty() {
            out.push(node.name.clone());
        }
        collect_compounds(&node.kids, out);
    }
}

fn collect_leaves(nodes: &[Named], out: &mut Vec<String>) {
    for node in nodes {
        if node.kids.is_empty() {
            out.push(node.name.clone());
        } else {
            collect_leaves(&node.kids, out);
        }
    }
}

fn contains_name(node: &Named, name: &str) -> bool {
    node.name == name || node.kids.iter().any(|child| contains_name(child, name))
}

fn is_descendant_or_self(nodes: &[Named], owner: &str, name: &str) -> bool {
    find_named(nodes, owner).is_some_and(|node| contains_name(node, name))
}

#[derive(Clone, Debug)]
struct InitialChoice {
    root: String,
    children: BTreeMap<String, String>,
}

fn initial_choices(nodes: &[Named]) -> Vec<InitialChoice> {
    fn axes(nodes: &[Named], out: &mut Vec<(String, Vec<String>)>) {
        for node in nodes {
            if !node.kids.is_empty() {
                out.push((
                    node.name.clone(),
                    node.kids.iter().map(|child| child.name.clone()).collect(),
                ));
            }
            axes(&node.kids, out);
        }
    }

    let mut choices: Vec<InitialChoice> = nodes
        .iter()
        .map(|node| InitialChoice {
            root: node.name.clone(),
            children: BTreeMap::new(),
        })
        .collect();
    let mut all_axes = Vec::new();
    axes(nodes, &mut all_axes);
    for (owner, children) in all_axes {
        let mut next = Vec::new();
        for choice in choices {
            for child in &children {
                let mut expanded = choice.clone();
                expanded.children.insert(owner.clone(), child.clone());
                next.push(expanded);
            }
        }
        choices = next;
    }
    choices
}

fn active_chain(nodes: &[Named], choice: &InitialChoice) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = choice.root.as_str();
    loop {
        out.push(current.to_string());
        let node = find_named(nodes, current).expect("initial choice names a state");
        if node.kids.is_empty() {
            break;
        }
        current = choice
            .children
            .get(current)
            .expect("compound initial choice exists");
    }
    out
}

fn choice_activating(nodes: &[Named], choices: &[InitialChoice], state: &str) -> InitialChoice {
    choices
        .iter()
        .find(|choice| active_chain(nodes, choice).iter().any(|name| name == state))
        .unwrap_or_else(|| panic!("no initial choice activates {state}"))
        .clone()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockCase {
    None,
    SetOne,
    Increment,
    Emit,
    IncrementAndEmit,
}

const TRANSITION_BLOCKS: [BlockCase; 5] = [
    BlockCase::None,
    BlockCase::SetOne,
    BlockCase::Increment,
    BlockCase::Emit,
    BlockCase::IncrementAndEmit,
];
const STATE_BLOCKS: [BlockCase; 4] = [
    BlockCase::SetOne,
    BlockCase::Increment,
    BlockCase::Emit,
    BlockCase::IncrementAndEmit,
];

fn block_members(case: BlockCase) -> Vec<&'static str> {
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

fn block_json(case: BlockCase) -> String {
    format!("{{{}}}", block_members(case).join(","))
}

#[derive(Clone, Debug, Default)]
struct Decorations {
    entry: Option<(String, BlockCase)>,
    exit: Option<(String, BlockCase)>,
}

#[derive(Clone, Debug)]
struct HistoryPlacement {
    owner: String,
    name: String,
    kind: &'static str,
}

fn emit_states(
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

fn transition_json(
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

fn machine_json(
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

fn sequences<'a>(events: &'a [&'a str]) -> Vec<Vec<&'a str>> {
    fn extend<'a>(events: &'a [&'a str], prefix: &mut Vec<&'a str>, out: &mut Vec<Vec<&'a str>>) {
        out.push(prefix.clone());
        if prefix.len() == 4 {
            return;
        }
        for event in events {
            prefix.push(*event);
            extend(events, prefix, out);
            prefix.pop();
        }
    }

    let mut out = Vec::new();
    extend(events, &mut Vec::new(), &mut out);
    out
}

#[derive(Debug, Default)]
struct RunCounts {
    sequences: u64,
    steps: u64,
    applied: u64,
    rejected: u64,
    ignored: u64,
    internal_applied: u64,
    external_applied: u64,
    leaf_changes: u64,
    effects: u64,
    history_changes: u64,
}

fn compare_run(src: &str) -> RunCounts {
    let (machine, tree) = compile_src(src);
    let event_names: Vec<String> = machine
        .spec
        .events
        .iter()
        .map(|event| event.name.clone())
        .collect();
    let event_refs: Vec<&str> = event_names.iter().map(String::as_str).collect();
    let engine_create = create(&machine, &tree, &BTreeMap::new())
        .unwrap_or_else(|err| panic!("engine create failed for generated machine: {err:?}\n{src}"));
    let oracle_create = oracle::naive_create(&machine, &BTreeMap::new())
        .unwrap_or_else(|err| panic!("oracle create failed for generated machine: {err:?}\n{src}"));
    assert_eq!(
        engine_create.leaf_after, oracle_create.leaf_after,
        "create leaf {src}"
    );
    assert_eq!(
        engine_create.ctx_after, oracle_create.ctx_after,
        "create ctx {src}"
    );
    assert_eq!(
        engine_create.history_after, oracle_create.history_after,
        "create history {src}"
    );
    assert_eq!(
        engine_create.effects, oracle_create.effects,
        "create effects {src}"
    );
    assert_eq!(
        engine_create.monitor_flags, oracle_create.monitor_flags,
        "create monitor flags {src}"
    );
    assert_eq!(
        engine_create.status_after, oracle_create.status_after,
        "create status {src}"
    );
    assert_eq!(
        engine_create.entered, oracle_create.entered,
        "create entry path {src}"
    );

    let engine_enterable = fsm_core::analyze::enterable(&machine, &tree);
    let oracle_enterable = oracle::brute_enterable(&machine);
    assert_eq!(engine_enterable, oracle_enterable, "enterable {src}");

    let initial_engine = InstanceState {
        status: engine_create.status_after,
        leaf: engine_create.leaf_after,
        ctx: engine_create.ctx_after,
        history: engine_create.history_after,
        pending: vec![],
    };
    let initial_oracle = InstanceState {
        status: oracle_create.status_after,
        leaf: oracle_create.leaf_after,
        ctx: oracle_create.ctx_after,
        history: oracle_create.history_after,
        pending: vec![],
    };
    let all_sequences = sequences(&event_refs);
    let mut counts = RunCounts {
        sequences: all_sequences.len() as u64,
        ..RunCounts::default()
    };
    for sequence in &all_sequences {
        let mut engine_state = initial_engine.clone();
        let mut oracle_state = initial_oracle.clone();
        for event in sequence {
            counts.steps += 1;
            let pre_engine = engine_state.clone();
            let pre_oracle = oracle_state.clone();
            let mut engine_budget = Budget::new(4096);
            let mut oracle_budget = Budget::new(4096);
            let engine_outcome = step(
                &machine,
                &tree,
                &engine_state,
                event,
                &payload(),
                &mut engine_budget,
            );
            let oracle_outcome = oracle::naive_step(
                &machine,
                &oracle_state,
                event,
                &payload(),
                &mut oracle_budget,
            );
            match (&engine_outcome, &oracle_outcome) {
                (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
                    counts.applied += 1;
                    counts.effects += engine.effects.len() as u64;
                    if engine.internal {
                        counts.internal_applied += 1;
                    } else {
                        counts.external_applied += 1;
                    }
                    if engine.leaf_after != pre_engine.leaf {
                        counts.leaf_changes += 1;
                    }
                    if engine.history_after != pre_engine.history {
                        counts.history_changes += 1;
                    }
                    assert_eq!(engine.leaf_after, oracle.leaf_after, "{src} {sequence:?}");
                    assert_eq!(engine.ctx_after, oracle.ctx_after, "{src} {sequence:?}");
                    assert_eq!(
                        engine.history_after, oracle.history_after,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(
                        engine.status_after, oracle.status_after,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.effects, oracle.effects, "{src} {sequence:?}");
                    assert_eq!(
                        engine.monitor_flags, oracle.monitor_flags,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.internal, oracle.internal, "{src} {sequence:?}");
                    assert_eq!(
                        engine.source_state, oracle.source_state,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(
                        engine.transition_idx, oracle.transition_idx,
                        "{src} {sequence:?}"
                    );
                    assert_eq!(engine.exited, oracle.exited, "{src} {sequence:?}");
                    assert_eq!(engine.entered, oracle.entered, "{src} {sequence:?}");
                    engine_state.leaf = engine.leaf_after.clone();
                    engine_state.ctx = engine.ctx_after.clone();
                    engine_state.history = engine.history_after.clone();
                    engine_state.status = engine.status_after;
                    oracle_state.leaf = oracle.leaf_after.clone();
                    oracle_state.ctx = oracle.ctx_after.clone();
                    oracle_state.history = oracle.history_after.clone();
                    oracle_state.status = oracle.status_after;
                }
                (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
                    counts.rejected += 1;
                    assert_eq!(engine.code, oracle.code, "{src} {sequence:?}");
                    assert_eq!(engine.cause, oracle.cause, "{src} {sequence:?}");
                    assert_eq!(engine_state, pre_engine, "engine mutated on reject {src}");
                    assert_eq!(oracle_state, pre_oracle, "oracle mutated on reject {src}");
                    assert_ne!(
                        engine.code, "internal/budget",
                        "normal budget tripped {src}"
                    );
                    assert_ne!(
                        engine.cause,
                        Some("internal/budget"),
                        "normal budget tripped inside a block {src}"
                    );
                }
                (Outcome::Ignored, Outcome::Ignored) => {
                    counts.ignored += 1;
                    assert_eq!(engine_state, pre_engine, "engine mutated on ignore {src}");
                    assert_eq!(oracle_state, pre_oracle, "oracle mutated on ignore {src}");
                }
                _ => panic!(
                    "outcome mismatch for {sequence:?}: engine={engine_outcome:?} oracle={oracle_outcome:?}\n{src}"
                ),
            }
        }
    }
    counts
}

#[derive(Debug, Default)]
struct SuiteCounts {
    generated: u64,
    executed: u64,
    runs: RunCounts,
}

fn execute_case(src: String, counts: &mut SuiteCounts) {
    counts.generated += 1;
    let run = compare_run(&src);
    counts.executed += 1;
    counts.runs.sequences += run.sequences;
    counts.runs.steps += run.steps;
    counts.runs.applied += run.applied;
    counts.runs.rejected += run.rejected;
    counts.runs.ignored += run.ignored;
    counts.runs.internal_applied += run.internal_applied;
    counts.runs.external_applied += run.external_applied;
    counts.runs.leaf_changes += run.leaf_changes;
    counts.runs.effects += run.effects;
    counts.runs.history_changes += run.history_changes;
}

fn state_from_create(machine: &fsm_core::machine::CompiledMachine, tree: &Tree) -> InstanceState {
    let created = create(machine, tree, &BTreeMap::new()).unwrap();
    InstanceState {
        status: created.status_after,
        leaf: created.leaf_after,
        ctx: created.ctx_after,
        history: created.history_after,
        pending: vec![],
    }
}

#[test]
fn enumerate_small_differential() {
    let guards = [
        None,
        Some("true"),
        Some("false"),
        Some("ctx.b"),
        Some("not ctx.b"),
    ];
    let mut counts = SuiteCounts::default();
    let mut forest_count = 0u64;
    let mut topology_internal = 0u64;
    let mut topology_external = 0u64;
    let mut initial_cases = 0u64;
    let mut guard_block_cases = 0u64;
    let mut entry_block_cases = 0u64;
    let mut exit_block_cases = 0u64;
    let mut pipeline_cases = 0u64;

    for n in 1..=5 {
        for forest in forests(n, 3) {
            if forest.is_empty() {
                continue;
            }
            forest_count += 1;
            let named = name_forest(&forest);
            let choices = initial_choices(&named);
            let mut names = Vec::new();
            collect_names(&named, &mut names);

            // Every state is made active in turn, then crossed with internal and
            // every external target. Thus every generated source/target topology
            // executes at least once rather than merely compiling unreachable rows.
            for source in &names {
                let choice = choice_activating(&named, &choices, source);
                let states = emit_states(&named, &choice, &Decorations::default(), None);
                let internal = transition_json(source, "e", None, None, BlockCase::None);
                execute_case(
                    machine_json(&states, &choice.root, &["e", "f"], &[internal]),
                    &mut counts,
                );
                topology_internal += 1;
                for target in &names {
                    let external =
                        transition_json(source, "e", Some(target), None, BlockCase::None);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[external]),
                        &mut counts,
                    );
                    topology_external += 1;
                }
            }

            // Cross every legal selection of the root and every compound's
            // initial child. The active leaf owns an executing internal row.
            for choice in &choices {
                let chain = active_chain(&named, choice);
                let source = chain.last().expect("initial chain has a leaf");
                let states = emit_states(&named, choice, &Decorations::default(), None);
                let transition =
                    transition_json(source, "e", None, Some("ctx.b"), BlockCase::Increment);
                execute_case(
                    machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                    &mut counts,
                );
                initial_cases += 1;
            }

            // The finite guard pool is crossed with the complete transition-block
            // pool (none, both assignments, emit, and assignment+emit).
            let choice = choices.first().expect("nonempty initial choices");
            let chain = active_chain(&named, choice);
            let source = chain.last().expect("initial chain has a leaf");
            let states = emit_states(&named, choice, &Decorations::default(), None);
            for guard in guards {
                for block in TRANSITION_BLOCKS {
                    let transition = transition_json(source, "e", None, guard, block);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                        &mut counts,
                    );
                    guard_block_cases += 1;
                }
            }

            // Put every nonempty block variant at entry and exit on every state.
            // An external self-transition on that active state traverses the block.
            for state in &names {
                let choice = choice_activating(&named, &choices, state);
                for block in STATE_BLOCKS {
                    let entry = Decorations {
                        entry: Some((state.clone(), block)),
                        exit: None,
                    };
                    let states = emit_states(&named, &choice, &entry, None);
                    let transition =
                        transition_json(state, "e", Some(state), None, BlockCase::None);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                        &mut counts,
                    );
                    entry_block_cases += 1;

                    let exit = Decorations {
                        entry: None,
                        exit: Some((state.clone(), block)),
                    };
                    let states = emit_states(&named, &choice, &exit, None);
                    let transition =
                        transition_json(state, "e", Some(state), None, BlockCase::None);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                        &mut counts,
                    );
                    exit_block_cases += 1;
                }
            }

            // One row per topology crosses exit, transition, and entry blocks in
            // one event, making effect order and pre-block snapshots observable.
            let choice = choices.first().expect("nonempty initial choices");
            let source = active_chain(&named, choice)
                .last()
                .expect("initial chain has a leaf")
                .clone();
            let decorations = Decorations {
                entry: Some((source.clone(), BlockCase::IncrementAndEmit)),
                exit: Some((source.clone(), BlockCase::IncrementAndEmit)),
            };
            let states = emit_states(&named, choice, &decorations, None);
            let transition = transition_json(
                &source,
                "e",
                Some(&source),
                None,
                BlockCase::IncrementAndEmit,
            );
            execute_case(
                machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                &mut counts,
            );
            pipeline_cases += 1;
        }
    }

    let mut history_placements = 0u64;
    let mut history_targets = 0u64;
    // A history pseudostate counts toward the five-state bound, so its owner
    // topologies use at most four ordinary states. Every compound owner, kind,
    // and owner-initial child is emitted. Whenever an outside leaf exists, the
    // generated go/back pair first binds and then targets history legally.
    for n in 2..=4 {
        for forest in forests(n, 3) {
            if forest.is_empty() {
                continue;
            }
            let named = name_forest(&forest);
            let choices = initial_choices(&named);
            let mut compounds = Vec::new();
            let mut leaves = Vec::new();
            collect_compounds(&named, &mut compounds);
            collect_leaves(&named, &mut leaves);
            for owner in compounds {
                let owner_node = find_named(&named, &owner).expect("compound exists");
                for owner_initial in &owner_node.kids {
                    let choice = choices
                        .iter()
                        .find(|choice| {
                            choice.children.get(&owner) == Some(&owner_initial.name)
                                && active_chain(&named, choice)
                                    .iter()
                                    .any(|name| name == &owner)
                        })
                        .unwrap_or_else(|| panic!("no choice activates history owner {owner}"));
                    let inside = active_chain(&named, choice)
                        .last()
                        .expect("owner initial descends to a leaf")
                        .clone();
                    let outside = leaves
                        .iter()
                        .find(|leaf| !is_descendant_or_self(&named, &owner, leaf.as_str()));
                    for kind in ["shallow", "deep"] {
                        let history = HistoryPlacement {
                            owner: owner.clone(),
                            name: format!("h_{kind}"),
                            kind,
                        };
                        let states =
                            emit_states(&named, choice, &Decorations::default(), Some(&history));
                        let transitions = if let Some(outside) = outside {
                            history_targets += 1;
                            vec![
                                transition_json(
                                    &inside,
                                    "go",
                                    Some(outside),
                                    None,
                                    BlockCase::IncrementAndEmit,
                                ),
                                transition_json(
                                    outside,
                                    "back",
                                    Some(&history.name),
                                    None,
                                    BlockCase::Emit,
                                ),
                            ]
                        } else {
                            vec![transition_json(
                                &inside,
                                "go",
                                None,
                                None,
                                BlockCase::IncrementAndEmit,
                            )]
                        };
                        execute_case(
                            machine_json(&states, &choice.root, &["go", "back"], &transitions),
                            &mut counts,
                        );
                        history_placements += 1;
                    }
                }
            }
        }
    }

    // Outcome::Ignored is part of the differential contract as well.
    let ignore_src = r#"{"format":"fsm.machine/1","name":"ignore","states":[{"name":"a"}],"initial":"a","on_unhandled":"ignore","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]},{"name":"f","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
    execute_case(ignore_src.to_string(), &mut counts);

    // Exhaustion in a transition block has one exact public/private error pair.
    let budget_src = r#"{"format":"fsm.machine/1","name":"budget","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
    let (machine, tree) = compile_src(budget_src);
    let state = state_from_create(&machine, &tree);
    let before = state.clone();
    let mut engine_budget = Budget::new(1);
    let mut oracle_budget = Budget::new(1);
    let engine = step(&machine, &tree, &state, "e", &payload(), &mut engine_budget);
    let oracle = oracle::naive_step(&machine, &state, "e", &payload(), &mut oracle_budget);
    match (&engine, &oracle) {
        (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
            assert_eq!(engine.code, "run/action_error");
            assert_eq!(engine.cause, Some("internal/budget"));
            assert_eq!(oracle.code, "run/action_error");
            assert_eq!(oracle.cause, Some("internal/budget"));
        }
        other => panic!("budget exhaustion mismatch: {other:?}"),
    }
    assert_eq!(state, before, "budget rejection mutated input state");

    let enum_src = r#"{"format":"fsm.machine/1","name":"en","enums":{"Color":["red","blue"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"paint","fields":[{"name":"c","ty":{"enum":"Color"}}]}],"transitions":[{"from":"a","on":"paint"}]}"#;
    let (machine, tree) = compile_src(enum_src);
    let state = state_from_create(&machine, &tree);
    let mut bad = BTreeMap::new();
    bad.insert("c".into(), Value::Str("green".into()));
    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        step(
            &machine,
            &tree,
            &state,
            "paint",
            &Value::Obj(bad.clone()),
            &mut engine_budget,
        ),
        oracle::naive_step(
            &machine,
            &state,
            "paint",
            &Value::Obj(bad),
            &mut oracle_budget,
        ),
    ) {
        (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
            assert_eq!(engine.code, oracle.code);
            assert_eq!(engine.code, "req/field_type");
        }
        other => panic!("enum mismatch: {other:?}"),
    }
    let mut good = BTreeMap::new();
    good.insert("c".into(), Value::Str("red".into()));
    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        step(
            &machine,
            &tree,
            &state,
            "paint",
            &Value::Obj(good.clone()),
            &mut engine_budget,
        ),
        oracle::naive_step(
            &machine,
            &state,
            "paint",
            &Value::Obj(good),
            &mut oracle_budget,
        ),
    ) {
        (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
            assert_eq!(engine.effects, oracle.effects);
        }
        other => panic!("valid enum mismatch: {other:?}"),
    }

    let decimal_src = r#"{"format":"fsm.machine/1","name":"dc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"pay","fields":[{"name":"amt","ty":{"decimal":"2"}}]}],"transitions":[{"from":"a","on":"pay"}]}"#;
    let (machine, tree) = compile_src(decimal_src);
    let state = state_from_create(&machine, &tree);
    let mut wide = BTreeMap::new();
    wide.insert("amt".into(), Value::Str("1.000".into()));
    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        step(
            &machine,
            &tree,
            &state,
            "pay",
            &Value::Obj(wide.clone()),
            &mut engine_budget,
        ),
        oracle::naive_step(
            &machine,
            &state,
            "pay",
            &Value::Obj(wide),
            &mut oracle_budget,
        ),
    ) {
        (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
            assert_eq!(engine.code, oracle.code);
            assert_eq!(engine.code, "req/field_scale");
        }
        other => panic!("decimal mismatch: {other:?}"),
    }

    assert_eq!(forest_count, 55, "bounded forest grammar changed");
    assert_eq!(topology_internal, 242, "internal topology rows changed");
    assert_eq!(topology_external, 1_112, "external topology rows changed");
    assert_eq!(initial_cases, 181, "initial-choice cross-product changed");
    assert_eq!(
        guard_block_cases, 1_375,
        "guard/block cross-product changed"
    );
    assert_eq!(entry_block_cases, 968, "entry-block placement rows changed");
    assert_eq!(exit_block_cases, 968, "exit-block placement rows changed");
    assert_eq!(pipeline_cases, 55, "pipeline rows changed");
    assert!(history_placements > 0, "history placement grammar is empty");
    assert!(
        history_targets > 0,
        "no legal outside-to-history target executed"
    );
    assert_eq!(
        counts.generated, counts.executed,
        "a generated machine was skipped"
    );
    assert_eq!(
        counts.runs.sequences,
        counts.executed * 31,
        "not every event sequence of length at most four executed"
    );
    assert_eq!(
        counts.runs.steps,
        counts.executed * 98,
        "executed event count changed"
    );
    assert!(counts.runs.internal_applied > 0);
    assert!(counts.runs.external_applied > 0);
    assert!(counts.runs.leaf_changes > 0);
    assert!(counts.runs.effects > 0);
    assert!(counts.runs.history_changes > 0);
    assert!(counts.runs.rejected > 0);
    assert!(counts.runs.ignored > 0);

    eprintln!(
        "enumerate_small generated={} executed={} sequences={} steps={} applied={} rejected={} ignored={} history_placements={} history_targets={}",
        counts.generated,
        counts.executed,
        counts.runs.sequences,
        counts.runs.steps,
        counts.runs.applied,
        counts.runs.rejected,
        counts.runs.ignored,
        history_placements,
        history_targets
    );
}
