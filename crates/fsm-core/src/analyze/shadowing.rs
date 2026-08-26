//! Transitions that can never fire because an earlier one always wins.

use crate::expr::parser;
use crate::machine::CompiledMachine;
use crate::spec::{ALWAYS_KEY, Finding};
use crate::tree::{NodeKind, Tree};

fn strip_spans_eq(a: &str, b: &str) -> bool {
    match (parser::parse(a), parser::parse(b)) {
        (Ok(x), Ok(y)) => crate::expr::ast::render_ast(&x) == crate::expr::ast::render_ast(&y),
        _ => a.split_whitespace().collect::<String>() == b.split_whitespace().collect::<String>(),
    }
}

pub(super) fn is_true_guard(g: &Option<String>) -> bool {
    match g {
        None => true,
        Some(s) => matches!(parser::parse(s), Ok(e) if crate::expr::ast::render_ast(&e) == "true"),
    }
}

pub fn shadowing_findings(m: &CompiledMachine) -> Vec<Finding> {
    let mut out = Vec::new();
    for ((from, on), idxs) in &m.transitions_by {
        for (i, &idx) in idxs.iter().enumerate() {
            let g = &m.spec.transitions[idx].guard;
            if is_true_guard(g) && i + 1 < idxs.len() {
                // Same rule, two spellings: a guardless eventless transition
                // masks every later eventless transition from its state just
                // as a guardless handler masks its `(from, on)` siblings.
                if on == ALWAYS_KEY {
                    out.push(Finding::err(
                        "def/eventless_shadowed",
                        format!("/transitions/{idx}"),
                        format!(
                            "eventless transition {idx} shadows later eventless transitions from {from}"
                        ),
                        format!("indices {idx} then {}", idxs[i + 1]),
                    ));
                } else {
                    out.push(Finding::err(
                        "def/shadowed",
                        format!("/transitions/{idx}"),
                        format!("transition {idx} shadows later entries on ({from}, {on})"),
                        format!("indices {idx} then {}", idxs[i + 1]),
                    ));
                }
            }
            for &later in &idxs[i + 1..] {
                if let (Some(a), Some(b)) = (
                    &m.spec.transitions[idx].guard,
                    &m.spec.transitions[later].guard,
                ) {
                    if strip_spans_eq(a, b) {
                        out.push(Finding::err(
                            "def/duplicate_guard",
                            format!("/transitions/{idx}"),
                            "duplicate guards in one (from, on) group",
                            format!("indices {idx} and {later}"),
                        ));
                    }
                }
            }
        }
    }
    out
}

pub fn ancestor_shadowed(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let mut out = Vec::new();
    for (i, tr) in m.spec.transitions.iter().enumerate() {
        let Some(aid) = t.id(&tr.from) else { continue };
        if !matches!(t.kind[aid as usize], NodeKind::Compound) {
            continue;
        }
        // leaves under A
        let mut leaves = Vec::new();
        fn collect_leaves(t: &Tree, id: u16, out: &mut Vec<u16>) {
            if matches!(t.kind[id as usize], NodeKind::Leaf) {
                out.push(id);
            }
            for &c in &t.children[id as usize] {
                collect_leaves(t, c, out);
            }
        }
        collect_leaves(t, aid, &mut leaves);
        if leaves.is_empty() {
            continue;
        }
        let mut all_masked = true;
        for leaf in &leaves {
            let chain = t.chain(*leaf);
            let mut masked = false;
            for &sid in &chain {
                if sid == aid {
                    break;
                }
                let sname = &t.names[sid as usize];
                if let Some(idxs) = m
                    .transitions_by
                    .get(&(sname.clone(), tr.cell_key().to_string()))
                {
                    for &idx in idxs {
                        let g = &m.spec.transitions[idx].guard;
                        if is_true_guard(g)
                            || (tr.guard.is_some()
                                && g.is_some()
                                && strip_spans_eq(g.as_ref().unwrap(), tr.guard.as_ref().unwrap()))
                        {
                            masked = true;
                        }
                    }
                }
            }
            if !masked {
                all_masked = false;
                break;
            }
        }
        if all_masked && !leaves.is_empty() {
            out.push(Finding::warn(
                "def/ancestor_shadowed",
                format!("/transitions/{i}"),
                match &tr.on {
                    Some(on) => format!("ancestor handler on {on} is globally dead"),
                    None => "eventless ancestor handler is globally dead".to_string(),
                },
                "every leaf under this compound masks the ancestor",
            ));
        }
    }
    out
}
