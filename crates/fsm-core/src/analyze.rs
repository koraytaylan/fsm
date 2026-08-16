//! Reachability, completeness, shadowing, and enabled-events.

#![allow(unused_imports, clippy::collapsible_if)]

use std::collections::{BTreeMap, BTreeSet};

use crate::expr::eval::{Budget, Val};
use crate::expr::parser;
use crate::expr::partial::{Truth, partial_eval_bool};
use crate::machine::{CompiledMachine, InstanceState};
use crate::spec::{Finding, HistoryKind, MachineSpec, Severity, TransitionSpec};
use crate::tree::{NodeKind, Tree};

pub use crate::spec::Finding as AnalyzeFinding;

/// Reachability lemma (history never extends the reachable set):
/// History bindings can only name configurations that were previously active,
/// and a shallow child's initial descent requires that child reachable some
/// other way first. Therefore modeling a history target as the owner's
/// initial chain is sound for the enterable-set over-approximation used here
/// (guard-optimistic).
pub fn enterable(m: &CompiledMachine, t: &Tree) -> BTreeSet<String> {
    let mut enterable = BTreeSet::new();
    if let Some(root) = t.id(&m.spec.initial) {
        enterable.insert(t.names[root as usize].clone());
        for n in t.initial_descent(root) {
            enterable.insert(t.names[n as usize].clone());
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for tr in &m.spec.transitions {
            if !enterable.contains(&tr.from) {
                continue;
            }
            let Some(to) = &tr.to else { continue };
            let Some(tid) = t.id(to) else { continue };
            let mut add = Vec::new();
            match &t.kind[tid as usize] {
                NodeKind::History(_) => {
                    if let Some(owner) = t.history_owner(tid) {
                        add.push(owner);
                        add.extend(t.initial_descent(owner));
                    }
                }
                NodeKind::Compound => {
                    add.push(tid);
                    add.extend(t.initial_descent(tid));
                }
                NodeKind::Leaf => add.push(tid),
            }
            // also add ancestors of the target down from root? entry path
            if let Some(src) = t.id(&tr.from) {
                if !matches!(t.kind[tid as usize], NodeKind::History(_)) {
                    let dom = t.proper_lca(src, tid);
                    for n in t.entry_path(dom, tid) {
                        add.push(n);
                    }
                }
            }
            for n in add {
                if enterable.insert(t.names[n as usize].clone()) {
                    changed = true;
                }
            }
        }
    }
    enterable
}

pub fn reachability_findings(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let ent = enterable(m, t);
    let mut out = Vec::new();
    for name in &t.names {
        if !ent.contains(name)
            && !matches!(t.kind[t.id(name).unwrap() as usize], NodeKind::History(_))
        {
            out.push(Finding::warn(
                "def/unreachable_state",
                format!("/states/{name}"),
                format!("{name} is not enterable"),
                "add a transition or initial path to this state",
            ));
        }
    }
    out
}

pub fn completeness_matrix(m: &CompiledMachine, t: &Tree) -> BTreeMap<(String, String), String> {
    let policy = match m.spec.on_unhandled {
        crate::spec::Unhandled::Ignore => "ignore",
        crate::spec::Unhandled::Reject => "reject",
    };
    let mut out = BTreeMap::new();
    let leaves: Vec<u16> = t
        .kind
        .iter()
        .enumerate()
        .filter_map(|(i, k)| match k {
            NodeKind::Leaf => Some(i as u16),
            _ => None,
        })
        .collect();
    for leaf in leaves {
        let chain = t.chain(leaf);
        let lname = t.names[leaf as usize].clone();
        for ev in &m.spec.events {
            let mut cell = format!("unhandled({policy})");
            for &sid in &chain {
                let sname = &t.names[sid as usize];
                if m.transitions_by
                    .contains_key(&(sname.clone(), ev.name.clone()))
                {
                    cell = format!("handled@{sname}");
                    break;
                }
            }
            out.insert((lname.clone(), ev.name.clone()), cell);
        }
    }
    out
}

fn strip_spans_eq(a: &str, b: &str) -> bool {
    match (parser::parse(a), parser::parse(b)) {
        (Ok(x), Ok(y)) => crate::expr::ast::render_ast(&x) == crate::expr::ast::render_ast(&y),
        _ => a.split_whitespace().collect::<String>() == b.split_whitespace().collect::<String>(),
    }
}

fn is_true_guard(g: &Option<String>) -> bool {
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
            if is_true_guard(g) {
                if i + 1 < idxs.len() {
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
                if let Some(idxs) = m.transitions_by.get(&(sname.clone(), tr.on.clone())) {
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
                format!("ancestor handler on {} is globally dead", tr.on),
                "every leaf under this compound masks the ancestor",
            ));
        }
    }
    out
}

pub fn create_always_fails(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    match crate::step::create(m, t, &BTreeMap::new()) {
        Err(r)
            if r.code == "run/create_failed"
                || r.code == "run/action_error"
                || r.code == "run/overflow" =>
        {
            vec![Finding {
                severity: Severity::Error,
                code: "def/create_always_fails",
                message: r.message,
                path: "/".into(),
                span: None,
                hint: "creation fails on declared inits".into(),
            }]
        }
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
    Enabled,
    Disabled,
    DependsOnPayload,
    Preempted,
    PreemptedMaybe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateReport {
    pub source_state: String,
    pub transition_idx: usize,
    pub truth: EventStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventReport {
    pub event: String,
    pub status: EventStatus,
    pub candidates: Vec<CandidateReport>,
    pub payload_fields: Vec<String>,
}

pub fn enabled_events(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    budget: &mut Budget,
) -> Vec<EventReport> {
    let leaf = t.id(&st.leaf).unwrap();
    let chain = t.chain(leaf);
    let mut reports = Vec::new();
    for ev in &m.spec.events {
        let mut cands = Vec::new();
        let mut summary = EventStatus::Disabled;
        let mut fields = Vec::new();
        let mut preempt = None;
        for &sid in &chain {
            let sname = t.names[sid as usize].clone();
            let idxs = m
                .transitions_by
                .get(&(sname.clone(), ev.name.clone()))
                .cloned()
                .unwrap_or_default();
            for idx in idxs {
                let status = if let Some(p) = preempt {
                    p
                } else {
                    match &m.spec.transitions[idx].guard {
                        None => EventStatus::Enabled,
                        Some(src) => match parser::parse(src) {
                            Ok(e) => match partial_eval_bool(&e, &st.ctx, budget) {
                                Truth::True => EventStatus::Enabled,
                                Truth::False => EventStatus::Disabled,
                                Truth::Unknown => {
                                    fields = field_reads(src);
                                    EventStatus::DependsOnPayload
                                }
                            },
                            Err(_) => EventStatus::DependsOnPayload,
                        },
                    }
                };
                if preempt.is_none() {
                    match status {
                        EventStatus::Enabled => {
                            summary = EventStatus::Enabled;
                            preempt = Some(EventStatus::Preempted);
                        }
                        EventStatus::DependsOnPayload => {
                            summary = EventStatus::DependsOnPayload;
                            preempt = Some(EventStatus::PreemptedMaybe);
                        }
                        EventStatus::Disabled => {}
                        _ => {}
                    }
                }
                cands.push(CandidateReport {
                    source_state: sname.clone(),
                    transition_idx: idx,
                    truth: status,
                });
            }
        }
        if summary == EventStatus::Disabled && !cands.is_empty() {
            // all false
            summary = EventStatus::Disabled;
        }
        reports.push(EventReport {
            event: ev.name.clone(),
            status: summary,
            candidates: cands,
            payload_fields: fields,
        });
    }
    reports
}

fn field_reads(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i + 4 < bytes.len() {
        if src[i..].starts_with("evt.") {
            i += 4;
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_lowercase() || bytes[i] == b'_' || bytes[i].is_ascii_digit())
            {
                i += 1;
            }
            out.push(src[start..i].to_string());
        } else {
            i += 1;
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn analyze_all(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let mut out = reachability_findings(m, t);
    out.extend(shadowing_findings(m));
    out.extend(ancestor_shadowed(m, t));
    out.extend(create_always_fails(m, t));
    out
}
