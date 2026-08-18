//! Deterministic Mermaid and DOT exporters.

use std::collections::BTreeSet;

use crate::machine::CompiledMachine;
use crate::spec::{HistoryKind, StateNode};

pub struct InstanceOverlay {
    pub current_leaf: String,
    pub visited: BTreeSet<String>,
}

/// Edge label: the event, plus the guard that distinguishes this transition
/// from the other edges on the same event. Without the guard, two guarded
/// transitions on one event render as the same arrow and the diagram silently
/// misstates the machine.
fn edge_label(tr: &crate::spec::TransitionSpec, escape: fn(&str) -> String) -> String {
    match &tr.guard {
        Some(g) => format!("{} [{}]", tr.on, escape(g)),
        None => tr.on.clone(),
    }
}

/// A Mermaid transition label runs to end of line, so a newline would truncate
/// it and `;` would start a new statement. `#` introduces Mermaid's own numeric
/// entity escapes, so it must be escaped first. Comparison operators are legal
/// in `stateDiagram-v2` labels and stay readable as written.
fn mermaid_escape(s: &str) -> String {
    s.replace('#', "#35;")
        .replace(';', "#59;")
        .replace(['\n', '\r'], " ")
}

pub fn mermaid(m: &CompiledMachine, overlay: Option<&InstanceOverlay>) -> String {
    let mut s = String::from("stateDiagram-v2\n");
    s.push_str(&format!("  [*] --> {}\n", m.spec.initial));
    write_mermaid_states(&m.spec.states, &mut s, overlay, 1);
    for tr in &m.spec.transitions {
        let label = edge_label(tr, mermaid_escape);
        if let Some(to) = &tr.to {
            s.push_str(&format!("  {} --> {}: {}\n", tr.from, to, label));
        } else {
            s.push_str(&format!(
                "  {} --> {}: {} (internal)\n",
                tr.from, tr.from, label
            ));
        }
    }
    if let Some(ov) = overlay {
        s.push_str("  classDef current font-weight:bold\n");
        s.push_str("  classDef visited opacity:0.5\n");
        s.push_str(&format!("  class {} current\n", ov.current_leaf));
        for v in &ov.visited {
            if v != &ov.current_leaf {
                s.push_str(&format!("  class {v} visited\n"));
            }
        }
    }
    s
}

fn write_mermaid_states(
    nodes: &[StateNode],
    s: &mut String,
    _overlay: Option<&InstanceOverlay>,
    indent: usize,
) {
    let pad = "  ".repeat(indent);
    for n in nodes {
        if let Some(h) = n.history {
            let tag = match h {
                HistoryKind::Deep => "deep-history",
                HistoryKind::Shallow => "shallow-history",
            };
            s.push_str(&format!("{pad}{name} : <<{tag}>>\n", name = n.name));
        } else if n.states.is_empty() {
            s.push_str(&format!("{pad}{name}\n", name = n.name));
            if n.terminal {
                s.push_str(&format!("{pad}{name} --> [*]\n", name = n.name));
            }
        } else {
            s.push_str(&format!("{pad}state {name} {{\n", name = n.name));
            if let Some(init) = &n.initial {
                s.push_str(&format!("{pad}  [*] --> {init}\n"));
            }
            write_mermaid_states(&n.states, s, _overlay, indent + 1);
            s.push_str(&format!("{pad}}}\n"));
        }
    }
}

/// DOT labels are quoted strings: backslash and quote need escaping, and a
/// literal newline would end the attribute.
fn dot_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}

pub fn dot(m: &CompiledMachine, overlay: Option<&InstanceOverlay>) -> String {
    let mut s = String::from("digraph fsm {\n");
    write_dot_states(&m.spec.states, &mut s, overlay, 1);
    s.push_str(&format!(
        "  __start [shape=point];\n  __start -> {};\n",
        m.spec.initial
    ));
    for tr in &m.spec.transitions {
        let to = tr.to.as_deref().unwrap_or(&tr.from);
        let label = edge_label(tr, dot_escape);
        s.push_str(&format!("  {} -> {} [label=\"{}\"];\n", tr.from, to, label));
    }
    s.push_str("}\n");
    s
}

fn write_dot_states(
    nodes: &[StateNode],
    s: &mut String,
    overlay: Option<&InstanceOverlay>,
    indent: usize,
) {
    let pad = "  ".repeat(indent);
    for n in nodes {
        if n.states.is_empty() {
            let extra = overlay
                .map(|o| {
                    if o.current_leaf == n.name {
                        " style=bold"
                    } else if o.visited.contains(&n.name) {
                        " color=gray"
                    } else {
                        ""
                    }
                })
                .unwrap_or("");
            if n.history.is_some() {
                let tag = match n.history.unwrap() {
                    HistoryKind::Deep => "deep-history",
                    HistoryKind::Shallow => "shallow-history",
                };
                s.push_str(&format!(
                    "{pad}{} [label=\"{}\\n<<{tag}>>\"{extra}];\n",
                    n.name, n.name
                ));
            } else {
                s.push_str(&format!("{pad}{} [{extra}];\n", n.name));
            }
        } else {
            s.push_str(&format!(
                "{pad}subgraph cluster_{} {{\n{pad}  label=\"{}\";\n",
                n.name, n.name
            ));
            write_dot_states(&n.states, s, overlay, indent + 1);
            s.push_str(&format!("{pad}}}\n"));
        }
    }
}
