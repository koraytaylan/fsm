//! Deterministic Mermaid and DOT exporters.

use std::collections::{BTreeMap, BTreeSet};

use crate::machine::CompiledMachine;
use crate::spec::{HistoryKind, StateNode, Topology};

/// Optional instance state rendered over a machine diagram.
pub struct InstanceOverlay {
    /// All currently active leaves; parallel machines contribute one per region.
    pub current_leaves: BTreeSet<String>,
    /// Previously visited state names to render as historical context.
    pub visited: BTreeSet<String>,
}

/// Edge label: the event, plus the guard that distinguishes this transition
/// from the other edges on the same event. Without the guard, two guarded
/// transitions on one event render as the same arrow and the diagram silently
/// misstates the machine. An eventless transition has no event to name; its
/// label is the guard alone, and the arrow itself says it is eventless.
fn edge_label(tr: &crate::spec::TransitionSpec, escape: fn(&str) -> String) -> String {
    match (&tr.on, &tr.guard) {
        (Some(event), Some(g)) => format!("{} [{}]", escape(event), escape(g)),
        (Some(event), None) => escape(event),
        (None, Some(g)) => format!("[{}]", escape(g)),
        (None, None) => String::new(),
    }
}

fn deadline_edge_label(deadline: &crate::spec::DeadlineSpec, escape: fn(&str) -> String) -> String {
    format!(
        "after {} [{}]",
        escape(&deadline.after),
        escape(&deadline.name)
    )
}

/// A Mermaid transition label runs to end of line, so a newline would truncate
/// it and `;` would start a new statement. `#` introduces Mermaid's own numeric
/// entity escapes, so it must be escaped first. Comparison operators are legal
/// in `stateDiagram-v2` labels and stay readable as written.
fn mermaid_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for character in s.chars() {
        match character {
            // Mermaid expands numeric entities in labels. Encoding `%` also
            // prevents its global `%%{...}%%` preprocessor directives from
            // being smuggled through otherwise quoted user text.
            '#' => escaped.push_str("#35;"),
            '%' => escaped.push_str("#37;"),
            ':' => escaped.push_str("#58;"),
            ';' => escaped.push_str("#59;"),
            '"' => escaped.push_str("#34;"),
            '\\' => escaped.push_str("#92;"),
            control if control.is_control() => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}

fn bare_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn mermaid_bare_identifier(s: &str) -> bool {
    let lowercase = s.to_ascii_lowercase();
    bare_identifier(s)
        && !matches!(
            lowercase.as_str(),
            "state"
                | "class"
                | "classdef"
                | "direction"
                | "note"
                | "strict"
                | "graph"
                | "digraph"
                | "subgraph"
                | "node"
                | "edge"
                | "style"
                | "click"
                | "href"
                | "default"
                | "scale"
                | "statediagram"
                | "linkstyle"
                | "interpolate"
                | "title"
                | "acctitle"
                | "accdescr"
                | "end"
        )
}

fn collect_state_names(nodes: &[StateNode], names: &mut BTreeSet<String>) {
    for node in nodes {
        names.insert(node.name.clone());
        collect_state_names(&node.states, names);
    }
}

fn collect_state_ids(
    nodes: &[StateNode],
    ids: &mut BTreeMap<String, String>,
    occupied: &mut BTreeSet<String>,
    next: &mut usize,
) {
    for node in nodes {
        let index = *next;
        *next += 1;
        let id = if mermaid_bare_identifier(&node.name) {
            node.name.clone()
        } else {
            // Mermaid's `class` statement accepts only word identifiers.
            // Search deterministically because `_` is legal in user names.
            let mut candidate = format!("_fsm_state_{index}");
            while occupied.contains(&candidate) {
                candidate.push('_');
            }
            occupied.insert(candidate.clone());
            candidate
        };
        ids.insert(node.name.clone(), id);
        collect_state_ids(&node.states, ids, occupied, next);
    }
}

fn diagram_state_ids(m: &CompiledMachine) -> BTreeMap<String, String> {
    let mut occupied = BTreeSet::new();
    for (_, states, _) in m.spec.state_groups() {
        collect_state_names(states, &mut occupied);
    }
    let mut ids = BTreeMap::new();
    let mut next = 0;
    for (_, states, _) in m.spec.state_groups() {
        collect_state_ids(states, &mut ids, &mut occupied, &mut next);
    }
    ids
}

fn state_id<'a>(ids: &'a BTreeMap<String, String>, name: &'a str) -> &'a str {
    ids.get(name).map(String::as_str).unwrap_or(name)
}

/// Render sequential or parallel topology, events, and deadlines as Mermaid.
/// Names unsafe in Mermaid grammar receive deterministic aliases and escaped labels.
pub fn mermaid(m: &CompiledMachine, overlay: Option<&InstanceOverlay>) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    let ids = diagram_state_ids(m);
    match &m.spec.topology {
        Topology::Sequential { states, initial } => {
            s.push_str(&format!("  [*] --> {}\n", state_id(&ids, initial)));
            write_mermaid_states(states, &mut s, &ids, 1);
        }
        Topology::Parallel { regions } => {
            // `$`-prefixed identifiers are reserved by the machine format, so
            // this synthetic concurrent root cannot collide with user states.
            s.push_str("  [*] --> $parallel\n");
            s.push_str("  state \"parallel\" as $parallel {\n");
            for (index, region) in regions.iter().enumerate() {
                if index > 0 {
                    s.push_str("    --\n");
                }
                s.push_str(&format!(
                    "    state \"{}\" as $region_{index} {{\n",
                    mermaid_escape(&region.name)
                ));
                s.push_str(&format!(
                    "      [*] --> {}\n",
                    state_id(&ids, &region.initial)
                ));
                write_mermaid_states(&region.states, &mut s, &ids, 3);
                s.push_str("    }\n");
            }
            s.push_str("  }\n");
        }
    }
    for tr in &m.spec.transitions {
        let label = edge_label(tr, mermaid_escape);
        let from = state_id(&ids, &tr.from);
        // stateDiagram-v2 has one arrow form, so an eventless edge announces
        // itself in its label instead of a dash pattern; a diagram that drew
        // it as an ordinary event arrow would misstate the machine.
        let label = match (tr.is_eventless(), tr.to.is_some()) {
            (false, true) => label,
            (false, false) => format!("{label} (internal)"),
            (true, true) if label.is_empty() => "(eventless)".to_string(),
            (true, true) => format!("{label} (eventless)"),
            (true, false) if label.is_empty() => "(eventless, internal)".to_string(),
            (true, false) => format!("{label} (eventless, internal)"),
        };
        let to = tr.to.as_deref().map_or(from, |to| state_id(&ids, to));
        s.push_str(&format!("  {from} --> {to}: {label}\n"));
    }
    for deadline in &m.spec.deadlines {
        let label = deadline_edge_label(deadline, mermaid_escape);
        s.push_str(&format!(
            "  {} --> {}: {}\n",
            state_id(&ids, &deadline.from),
            state_id(&ids, &deadline.to),
            label
        ));
    }
    if let Some(ov) = overlay {
        s.push_str("  classDef current font-weight:bold\n");
        s.push_str("  classDef visited opacity:0.5\n");
        for current in &ov.current_leaves {
            if let Some(id) = ids.get(current) {
                s.push_str(&format!("  class {id} current\n"));
            }
        }
        for v in &ov.visited {
            if !ov.current_leaves.contains(v)
                && let Some(id) = ids.get(v)
            {
                s.push_str(&format!("  class {id} visited\n"));
            }
        }
    }
    s
}

/// The first eight hex of a child machine's digest: enough to tell two slots
/// apart at a glance without opening either definition.
fn invoke_digest(machine: &str) -> String {
    let digest = crate::hashes::digest_of(machine).unwrap_or(machine);
    digest.chars().take(8).collect()
}

fn write_mermaid_states(
    nodes: &[StateNode],
    s: &mut String,
    ids: &BTreeMap<String, String>,
    indent: usize,
) {
    let pad = "  ".repeat(indent);
    for n in nodes {
        let id = state_id(ids, &n.name);
        let aliased = id != n.name;
        if let Some(h) = n.history {
            let tag = match h {
                HistoryKind::Deep => "deep-history",
                HistoryKind::Shallow => "shallow-history",
            };
            if aliased {
                s.push_str(&format!(
                    "{pad}state \"{}\" as {id}\n",
                    mermaid_escape(&n.name)
                ));
            }
            s.push_str(&format!("{pad}{id} : <<{tag}>>\n"));
        } else if n.states.is_empty() {
            if aliased {
                s.push_str(&format!(
                    "{pad}state \"{}\" as {id}\n",
                    mermaid_escape(&n.name)
                ));
            } else {
                s.push_str(&format!("{pad}{id}\n"));
            }
            if n.terminal {
                s.push_str(&format!("{pad}{id} --> [*]\n"));
            }
            // Distinct from terminal on purpose: a final state ends its
            // compound, not the machine, so it never points at `[*]`.
            if n.final_state {
                s.push_str(&format!("{pad}{id} : <<final>>\n"));
            }
        } else {
            if aliased {
                s.push_str(&format!(
                    "{pad}state \"{}\" as {id} {{\n",
                    mermaid_escape(&n.name)
                ));
            } else {
                s.push_str(&format!("{pad}state {id} {{\n"));
            }
            if let Some(init) = &n.initial {
                s.push_str(&format!("{pad}  [*] --> {}\n", state_id(ids, init)));
            }
            write_mermaid_states(&n.states, s, ids, indent + 1);
            s.push_str(&format!("{pad}}}\n"));
        }
        // stateDiagram-v2 has no subgraph and a composite state means
        // something else here, so a slot is annotated the way this renderer
        // already annotates `<<final>>` and `<<deep-history>>`: a description
        // line on the state that holds it.
        for invoke in &n.invokes {
            s.push_str(&format!(
                "{pad}{id} : <<invoke {} → {}>>\n",
                mermaid_escape(&invoke.id),
                invoke_digest(&invoke.machine)
            ));
        }
    }
}

/// DOT labels are quoted strings: backslash and quote need escaping, and a
/// literal newline would end the attribute.
fn dot_escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for character in s.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            control if control.is_control() => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}

fn dot_identifier(id: &str) -> String {
    let lowercase = id.to_ascii_lowercase();
    if bare_identifier(id)
        && !matches!(
            lowercase.as_str(),
            "strict" | "graph" | "digraph" | "subgraph" | "node" | "edge"
        )
    {
        id.to_string()
    } else {
        format!("\"{}\"", dot_escape(id))
    }
}

fn dot_state_id(ids: &BTreeMap<String, String>, name: &str) -> String {
    dot_identifier(state_id(ids, name))
}

/// Render sequential or parallel topology, events, and deadlines as Graphviz DOT.
/// Names unsafe in DOT grammar receive deterministic aliases and escaped labels.
pub fn dot(m: &CompiledMachine, overlay: Option<&InstanceOverlay>) -> String {
    let mut s = String::from("digraph fsm {\n");
    let ids = diagram_state_ids(m);
    let start = if ids.values().any(|id| id == "__start") {
        dot_identifier("$start")
    } else {
        "__start".to_string()
    };
    match &m.spec.topology {
        Topology::Sequential { states, initial } => {
            write_dot_states(states, &mut s, overlay, &ids, 1);
            s.push_str(&format!(
                "  {start} [shape=point];\n  {start} -> {};\n",
                dot_state_id(&ids, initial)
            ));
        }
        Topology::Parallel { regions } => {
            for (index, region) in regions.iter().enumerate() {
                s.push_str(&format!(
                    "  subgraph {} {{\n    label=\"{}\";\n",
                    dot_identifier(&format!("cluster_$region_{index}")),
                    dot_escape(&region.name)
                ));
                write_dot_states(&region.states, &mut s, overlay, &ids, 2);
                s.push_str("  }\n");
            }
            s.push_str(&format!("  {start} [shape=point];\n"));
            for region in regions {
                s.push_str(&format!(
                    "  {start} -> {};\n",
                    dot_state_id(&ids, &region.initial)
                ));
            }
        }
    }
    for tr in &m.spec.transitions {
        let to = tr.to.as_deref().unwrap_or(&tr.from);
        let label = edge_label(tr, dot_escape);
        let style = if tr.is_eventless() {
            " style=dashed"
        } else {
            ""
        };
        s.push_str(&format!(
            "  {} -> {} [label=\"{}\"{style}];\n",
            dot_state_id(&ids, &tr.from),
            dot_state_id(&ids, to),
            label
        ));
    }
    for deadline in &m.spec.deadlines {
        let label = deadline_edge_label(deadline, dot_escape);
        s.push_str(&format!(
            "  {} -> {} [label=\"{}\"];\n",
            dot_state_id(&ids, &deadline.from),
            dot_state_id(&ids, &deadline.to),
            label
        ));
    }
    s.push_str("}\n");
    s
}

fn write_dot_states(
    nodes: &[StateNode],
    s: &mut String,
    overlay: Option<&InstanceOverlay>,
    ids: &BTreeMap<String, String>,
    indent: usize,
) {
    let pad = "  ".repeat(indent);
    for n in nodes {
        let raw_id = state_id(ids, &n.name);
        let id = dot_identifier(raw_id);
        let aliased = raw_id != n.name;
        if n.states.is_empty() {
            let extra = overlay
                .map(|o| {
                    if o.current_leaves.contains(&n.name) {
                        " style=bold"
                    } else if o.visited.contains(&n.name) {
                        " color=gray"
                    } else {
                        ""
                    }
                })
                .unwrap_or("");
            let shape = if n.final_state {
                " shape=doublecircle"
            } else {
                ""
            };
            if let Some(h) = n.history {
                let tag = match h {
                    HistoryKind::Deep => "deep-history",
                    HistoryKind::Shallow => "shallow-history",
                };
                s.push_str(&format!(
                    "{pad}{} [label=\"{}\\n<<{tag}>>\"{extra}];\n",
                    id,
                    dot_escape(&n.name)
                ));
            } else if aliased {
                s.push_str(&format!(
                    "{pad}{id} [label=\"{}\"{extra}{shape}];\n",
                    dot_escape(&n.name)
                ));
            } else {
                s.push_str(&format!("{pad}{id} [{extra}{shape}];\n"));
            }
        } else {
            s.push_str(&format!(
                "{pad}subgraph {} {{\n{pad}  label=\"{}\";\n",
                dot_identifier(&format!("cluster_{raw_id}")),
                dot_escape(&n.name)
            ));
            write_dot_states(&n.states, s, overlay, ids, indent + 1);
            s.push_str(&format!("{pad}}}\n"));
        }
        // A slot is a child node hanging off its state: `box3d` says "this is
        // another machine" without claiming to be a state of this one.
        for invoke in &n.invokes {
            let slot_id = dot_identifier(&format!("{raw_id}__invoke__{}", invoke.id));
            s.push_str(&format!(
                "{pad}{slot_id} [label=\"{} · {}\" shape=box3d];\n",
                dot_escape(&invoke.id),
                invoke_digest(&invoke.machine)
            ));
            s.push_str(&format!("{pad}{id} -> {slot_id} [style=dotted];\n"));
        }
    }
}
