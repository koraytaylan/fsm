//! Reachability, completeness, shadowing, and enabled-events.

#![allow(unused_imports, clippy::collapsible_if)]

use std::collections::{BTreeMap, BTreeSet};

use crate::expr::eval::{Budget, Val};
use crate::expr::parser;
use crate::expr::partial::{Truth, partial_eval_bool};
use crate::machine::{CompiledMachine, InstanceState};
use crate::spec::{ALWAYS_KEY, Finding, HistoryKind, MachineSpec, Severity, TransitionSpec};
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
    for (_, root_initial) in &t.root_initials {
        enterable.insert(t.names[*root_initial as usize].clone());
        for state in t.initial_descent(*root_initial) {
            enterable.insert(t.names[state as usize].clone());
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for transition in &m.spec.transitions {
            if !enterable.contains(&transition.from) {
                continue;
            }
            let Some(target) = &transition.to else {
                continue;
            };
            if add_enterable_target(t, &transition.from, target, &mut enterable) {
                changed = true;
            }
        }
        for deadline in &m.spec.deadlines {
            if enterable.contains(&deadline.from)
                && add_enterable_target(t, &deadline.from, &deadline.to, &mut enterable)
            {
                changed = true;
            }
        }
    }
    enterable
}

fn add_enterable_target(
    tree: &Tree,
    source: &str,
    target: &str,
    enterable: &mut BTreeSet<String>,
) -> bool {
    let Some(target_id) = tree.id(target) else {
        return false;
    };
    let mut additions = Vec::new();
    match &tree.kind[target_id as usize] {
        NodeKind::History(_) => {
            if let Some(owner) = tree.history_owner(target_id) {
                additions.push(owner);
                additions.extend(tree.initial_descent(owner));
                if let Some(source_id) = tree.id(source) {
                    let domain = tree.proper_lca(source_id, owner);
                    additions.extend(tree.entry_path(domain, owner));
                }
            }
        }
        NodeKind::Compound => {
            additions.push(target_id);
            additions.extend(tree.initial_descent(target_id));
        }
        NodeKind::Leaf => additions.push(target_id),
    }
    if let Some(source_id) = tree.id(source)
        && !matches!(tree.kind[target_id as usize], NodeKind::History(_))
    {
        let domain = tree.proper_lca(source_id, target_id);
        additions.extend(tree.entry_path(domain, target_id));
    }
    let mut changed = false;
    for state in additions {
        changed |= enterable.insert(tree.names[state as usize].clone());
    }
    changed
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

/// Report structural event handling for every real leaf.
///
/// Terminal leaves are inert: their ancestor handlers are not candidates, even
/// while another region keeps a parallel instance running.
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
        let terminal = find_machine_node(&m.spec, &lname).is_some_and(|node| node.terminal);
        for ev in &m.spec.events {
            let mut cell = format!("unhandled({policy})");
            if !terminal {
                for &sid in &chain {
                    let sname = &t.names[sid as usize];
                    if m.transitions_by
                        .contains_key(&(sname.clone(), ev.name.clone()))
                    {
                        cell = format!("handled@{sname}");
                        break;
                    }
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

fn type_default(ty: &crate::spec::TySpec, enums: &BTreeMap<String, Vec<String>>) -> Option<Val> {
    use crate::spec::TySpec;
    Some(match ty {
        TySpec::Int => Val::Int(0),
        TySpec::Bool => Val::Bool(false),
        TySpec::Str => Val::Str(String::new()),
        TySpec::Ts => Val::Ts(0),
        TySpec::Dur => Val::Dur(0),
        TySpec::Dec { scale } => Val::Dec(crate::decimal::Dec {
            mant: 0,
            scale: *scale,
        }),
        TySpec::Enum { of } => {
            let v = enums.get(of).and_then(|vs| vs.first()).cloned()?;
            Val::Enum {
                ty: of.clone(),
                variant: v,
            }
        }
    })
}

fn type_alt(ty: &crate::spec::TySpec, enums: &BTreeMap<String, Vec<String>>) -> Option<Val> {
    use crate::spec::TySpec;
    Some(match ty {
        TySpec::Int => Val::Int(1),
        TySpec::Bool => Val::Bool(true),
        TySpec::Str => Val::Str("x".into()),
        TySpec::Ts => Val::Ts(1),
        TySpec::Dur => Val::Dur(1),
        TySpec::Dec { scale } => Val::Dec(crate::decimal::Dec {
            mant: 1,
            scale: *scale,
        }),
        TySpec::Enum { of } => {
            let vars = enums.get(of)?;
            let v = vars.get(1).or_else(|| vars.first())?.clone();
            Val::Enum {
                ty: of.clone(),
                variant: v,
            }
        }
    })
}

fn expr_reads_ctx(e: &crate::expr::ast::Expr) -> bool {
    use crate::expr::ast::{Arg, Expr};
    match e {
        Expr::CtxRef { .. } => true,
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => expr_reads_ctx(inner),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => expr_reads_ctx(lhs) || expr_reads_ctx(rhs),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => expr_reads_ctx(cond) || expr_reads_ctx(then_branch) || expr_reads_ctx(else_branch),
        Expr::Call { args, .. } => args.iter().any(|a| match a {
            Arg::Expr(inner) => expr_reads_ctx(inner),
            Arg::Word { .. } => false,
        }),
        _ => false,
    }
}

fn find_node<'a>(
    nodes: &'a [crate::spec::StateNode],
    name: &str,
) -> Option<&'a crate::spec::StateNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(hit) = find_node(&n.states, name) {
            return Some(hit);
        }
    }
    None
}

fn find_machine_node<'a>(spec: &'a MachineSpec, name: &str) -> Option<&'a crate::spec::StateNode> {
    spec.state_groups()
        .into_iter()
        .find_map(|(_, states, _)| find_node(states, name))
}

fn create_path_depends_on_override(m: &CompiledMachine, t: &Tree) -> bool {
    let mut srcs = Vec::new();
    for inv in &m.spec.invariants {
        srcs.push(inv.expr.as_str());
    }
    let mut entered_names = BTreeSet::new();
    for (_, root_initial) in &t.root_initials {
        let mut entry_path = vec![*root_initial];
        entry_path.extend(t.initial_descent(*root_initial));
        for state in entry_path {
            let name = &t.names[state as usize];
            entered_names.insert(name.as_str());
            if let Some(node) = find_machine_node(&m.spec, name) {
                if let Some(b) = &node.entry {
                    for s in &b.sets {
                        srcs.push(s.value.as_str());
                    }
                    for em in &b.emits {
                        for src in em.args.values() {
                            srcs.push(src.as_str());
                        }
                    }
                }
            }
        }
    }
    for deadline in &m.spec.deadlines {
        if entered_names.contains(deadline.from.as_str()) {
            srcs.push(deadline.after.as_str());
        }
    }
    srcs.iter().any(|src| {
        parser::parse(src)
            .map(|e| expr_reads_ctx(&e))
            .unwrap_or(false)
    })
}

pub fn create_always_fails(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let declared = crate::step::create(m, t, &BTreeMap::new(), 0);
    let Err(r) = declared else {
        return Vec::new();
    };
    if r.code != "run/create_failed" {
        return Vec::new();
    }
    let mut defaults = BTreeMap::new();
    for c in &m.spec.context {
        if let Some(v) = type_default(&c.ty, &m.spec.enums) {
            defaults.insert(c.name.clone(), v);
        }
    }
    if crate::step::create(m, t, &defaults, 0).is_ok() {
        return Vec::new();
    }
    let mut alts = BTreeMap::new();
    for c in &m.spec.context {
        if let Some(v) = type_alt(&c.ty, &m.spec.enums) {
            alts.insert(c.name.clone(), v);
        }
    }
    if crate::step::create(m, t, &alts, 0).is_ok() {
        return Vec::new();
    }
    for c in &m.spec.context {
        if let Some(v) = type_alt(&c.ty, &m.spec.enums) {
            let mut one = BTreeMap::new();
            one.insert(c.name.clone(), v);
            if crate::step::create(m, t, &one, 0).is_ok() {
                return Vec::new();
            }
        }
    }
    if create_path_depends_on_override(m, t) {
        return Vec::new();
    }
    vec![Finding {
        severity: Severity::Error,
        code: "def/create_always_fails",
        message: r.message,
        path: "/".into(),
        span: r
            .span
            .map(|(s, e)| crate::expr::lexer::Span::new(s as usize, e as usize)),
        hint: "creation fails on declared inits".into(),
    }]
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
    enabled_events_with_guard_accounting(m, t, st, budget, OmittedGuardAccounting::Current)
}

/// Reproduce the legacy diagnostic accounting used in already-sealed
/// rejection details. Runtime selection always charged omitted guards; only
/// the historical enabled-event diagnostic omitted those ticks.
pub(crate) fn enabled_events_historical(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    budget: &mut Budget,
) -> Vec<EventReport> {
    enabled_events_with_guard_accounting(m, t, st, budget, OmittedGuardAccounting::Historical)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OmittedGuardAccounting {
    Current,
    Historical,
}

fn enabled_events_with_guard_accounting(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    budget: &mut Budget,
    omitted_guard_accounting: OmittedGuardAccounting,
) -> Vec<EventReport> {
    let active_leaves = t.active_leaves(&st.configuration).unwrap_or_default();
    let state_names = m.spec.state_names();
    let ctx_tys: BTreeMap<String, crate::expr::typeck::Ty> = m
        .spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let mut reports = Vec::new();
    for ev in &m.spec.events {
        let evt_tys: BTreeMap<String, crate::expr::typeck::Ty> = ev
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.ty.to_ty()))
            .collect();
        let mut cands = Vec::new();
        let mut summary = EventStatus::Disabled;
        let mut fields = Vec::new();
        let mut preempt = None;
        for (_, leaf) in &active_leaves {
            let leaf_name = &t.names[*leaf as usize];
            if find_machine_node(&m.spec, leaf_name).is_some_and(|node| node.terminal) {
                continue;
            }
            for sid in t.chain(*leaf) {
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
                            None if omitted_guard_accounting == OmittedGuardAccounting::Current => {
                                // Runtime evaluates an omitted guard as an
                                // implicit `true`, including its one budget
                                // tick. Analysis follows the same path; as for
                                // any other concrete evaluation error, budget
                                // exhaustion is conservatively Unknown.
                                let implicit =
                                    parser::parse("true").expect("static omitted-guard expression");
                                match partial_eval_bool(
                                    &implicit,
                                    &st.ctx,
                                    &crate::expr::typeck::Scope {
                                        kind: crate::expr::typeck::ScopeKind::Guard,
                                        ctx: &ctx_tys,
                                        evt: Some(&evt_tys),
                                        enums: &m.spec.enums,
                                        states: &state_names,
                                    },
                                    budget,
                                ) {
                                    Truth::True => EventStatus::Enabled,
                                    Truth::False => EventStatus::Disabled,
                                    Truth::Unknown => EventStatus::DependsOnPayload,
                                }
                            }
                            None => EventStatus::Enabled,
                            Some(src) => {
                                let e = m
                                    .compiled_exprs
                                    .get(&crate::machine::ExprSlot::TransitionGuard(idx))
                                    .map(|c| c.expr.clone())
                                    .or_else(|| parser::parse(src).ok());
                                match e {
                                    Some(e) => match partial_eval_bool(
                                        &e,
                                        &st.ctx,
                                        &crate::expr::typeck::Scope {
                                            kind: crate::expr::typeck::ScopeKind::Guard,
                                            ctx: &ctx_tys,
                                            evt: Some(&evt_tys),
                                            enums: &m.spec.enums,
                                            states: &state_names,
                                        },
                                        budget,
                                    ) {
                                        Truth::True => EventStatus::Enabled,
                                        Truth::False => EventStatus::Disabled,
                                        Truth::Unknown => {
                                            fields = field_reads(src);
                                            EventStatus::DependsOnPayload
                                        }
                                    },
                                    None => EventStatus::DependsOnPayload,
                                }
                            }
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
    if let Ok(e) = parser::parse(src) {
        collect_evt_refs(&e, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn collect_evt_refs(e: &crate::expr::ast::Expr, out: &mut Vec<String>) {
    use crate::expr::ast::{Arg, Expr};
    match e {
        Expr::EvtRef { name, .. } => out.push(name.clone()),
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => collect_evt_refs(inner, out),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => {
            collect_evt_refs(lhs, out);
            collect_evt_refs(rhs, out);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            collect_evt_refs(cond, out);
            collect_evt_refs(then_branch, out);
            collect_evt_refs(else_branch, out);
        }
        Expr::Call { args, .. } => {
            for a in args {
                if let Arg::Expr(inner) = a {
                    collect_evt_refs(inner, out);
                }
            }
        }
        _ => {}
    }
}

/// `def/eventless_internal_noop`: an eventless transition with no target and
/// no actions can only spend a microstep.
///
/// A warning, not an error: a definition may be mid-authoring, but in a
/// shipped machine such a transition is always a mistake.
pub fn eventless_noop_findings(m: &CompiledMachine) -> Vec<Finding> {
    m.spec
        .transitions
        .iter()
        .enumerate()
        .filter(|(_, transition)| {
            transition.is_eventless()
                && transition.to.is_none()
                && transition.sets.is_empty()
                && transition.emits.is_empty()
        })
        .map(|(index, transition)| {
            Finding::warn(
                "def/eventless_internal_noop",
                format!("/transitions/{index}"),
                format!(
                    "eventless transition {index} from {} has no to, do, or emit",
                    transition.from
                ),
                "give it a target or an action; as written it can only burn a microstep",
            )
        })
        .collect()
}

/// One eventless reaction the scan could select from an active leaf.
struct EventlessEdge {
    from_leaf: u16,
    transition_idx: usize,
    to_leaf: u16,
    /// Guardless or literally `true`: the scan selects it whenever it is
    /// reached, so nothing after it on the chain can fire.
    certain: bool,
}

/// The eventless transition graph over the leaves a scan can start from.
///
/// From each non-terminal leaf, the candidates in scan order — innermost
/// state first, document order within a cell — up to and including the
/// first certain one, because a certain winner ends the scan. Targets
/// resolve through the same tree rules `step` uses: an internal transition
/// keeps the leaf, a history target descends its owner's initial chain (the
/// binding is unknown at admission), an external self-transition re-enters
/// `from`, and a compound target descends to its initial leaf.
///
/// Guard truth is decided syntactically and never by partial evaluation:
/// admission must be a pure function of the definition, and evaluating a
/// guard over an unknown context would make whether a machine is accepted
/// depend on which context a caller might later supply.
fn eventless_edges(m: &CompiledMachine, t: &Tree) -> Vec<EventlessEdge> {
    let mut edges = Vec::new();
    for leaf in 0..t.names.len() as u16 {
        if !matches!(t.kind[leaf as usize], NodeKind::Leaf)
            || find_machine_node(&m.spec, &t.names[leaf as usize]).is_some_and(|node| node.terminal)
        {
            continue;
        }
        'scan: for source in t.chain(leaf) {
            let cell = (t.names[source as usize].clone(), ALWAYS_KEY.to_string());
            for &transition_idx in m.transitions_by.get(&cell).into_iter().flatten() {
                let transition = &m.spec.transitions[transition_idx];
                let to_leaf = match &transition.to {
                    None => leaf,
                    Some(target) => landing_leaf(t, target),
                };
                let certain = is_true_guard(&transition.guard);
                edges.push(EventlessEdge {
                    from_leaf: leaf,
                    transition_idx,
                    to_leaf,
                    certain,
                });
                if certain {
                    break 'scan;
                }
            }
        }
    }
    edges
}

/// The leaf a transition to `target` lands in, before any history binding.
fn landing_leaf(t: &Tree, target: &str) -> u16 {
    let Some(target_id) = t.id(target) else {
        return 0;
    };
    let (root, descent) = match t.kind[target_id as usize] {
        NodeKind::History(_) => match t.history_owner(target_id) {
            Some(owner) => (owner, t.history_descent(target_id, None)),
            None => (target_id, Vec::new()),
        },
        NodeKind::Compound => (target_id, t.initial_descent(target_id)),
        NodeKind::Leaf => (target_id, Vec::new()),
    };
    descent.last().copied().unwrap_or(root)
}

/// Strongly connected components of the eventless graph, iteratively.
///
/// Iterative rather than recursive: `MAX_STATES` is 256 and depth 12, but a
/// hostile definition must not be able to blow the stack.
fn strongly_connected_components(node_count: usize, edges: &[EventlessEdge]) -> Vec<Vec<u16>> {
    let mut adjacency: Vec<Vec<u16>> = vec![Vec::new(); node_count];
    for edge in edges {
        adjacency[edge.from_leaf as usize].push(edge.to_leaf);
    }
    let mut index_of = vec![usize::MAX; node_count];
    let mut low_link = vec![0usize; node_count];
    let mut on_stack = vec![false; node_count];
    let mut stack: Vec<u16> = Vec::new();
    let mut components = Vec::new();
    let mut next_index = 0usize;
    for start in 0..node_count as u16 {
        if index_of[start as usize] != usize::MAX {
            continue;
        }
        let mut work: Vec<(u16, usize)> = vec![(start, 0)];
        index_of[start as usize] = next_index;
        low_link[start as usize] = next_index;
        next_index += 1;
        stack.push(start);
        on_stack[start as usize] = true;
        while let Some(&mut (node, ref mut next_edge)) = work.last_mut() {
            if *next_edge < adjacency[node as usize].len() {
                let successor = adjacency[node as usize][*next_edge];
                *next_edge += 1;
                if index_of[successor as usize] == usize::MAX {
                    index_of[successor as usize] = next_index;
                    low_link[successor as usize] = next_index;
                    next_index += 1;
                    stack.push(successor);
                    on_stack[successor as usize] = true;
                    work.push((successor, 0));
                } else if on_stack[successor as usize] {
                    low_link[node as usize] =
                        low_link[node as usize].min(index_of[successor as usize]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low_link[parent as usize] = low_link[parent as usize].min(low_link[node as usize]);
            }
            if low_link[node as usize] == index_of[node as usize] {
                let mut component = Vec::new();
                loop {
                    let member = stack.pop().expect("tarjan stack holds the component");
                    on_stack[member as usize] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.sort_unstable();
                components.push(component);
            }
        }
    }
    components
}

/// Cycles in the eventless graph, and how deep an acyclic cascade can run.
///
/// A component that the machine provably cannot leave — every node in it has
/// a certain edge, and every edge that could be selected from any node stays
/// inside it — is `def/eventless_cycle`, an error: whatever the guards say,
/// a macrostep that enters it never quiesces. Any other cycle is
/// `def/eventless_cycle_guarded`, a warning, because the engine cannot
/// decide the guard at admission and `MAX_MICROSTEPS` is what stops it at
/// run time. `def/eventless_depth` warns when the longest acyclic cascade,
/// multiplied by the region count that shares the ceiling, reaches half of
/// `MAX_MICROSTEPS`.
pub fn eventless_cycle_findings(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let edges = eventless_edges(m, t);
    if edges.is_empty() {
        return Vec::new();
    }
    let node_count = t.names.len();
    let components = strongly_connected_components(node_count, &edges);
    let mut component_of = vec![usize::MAX; node_count];
    for (component_index, component) in components.iter().enumerate() {
        for &member in component {
            component_of[member as usize] = component_index;
        }
    }
    let mut out = Vec::new();
    for (component_index, component) in components.iter().enumerate() {
        let inside = |leaf: u16| component_of[leaf as usize] == component_index;
        let component_edges: Vec<&EventlessEdge> =
            edges.iter().filter(|edge| inside(edge.from_leaf)).collect();
        let cyclic = component.len() > 1
            || component_edges
                .iter()
                .any(|edge| edge.to_leaf == edge.from_leaf);
        if !cyclic {
            continue;
        }
        let inescapable = component.iter().all(|&member| {
            let from_member = component_edges
                .iter()
                .filter(|edge| edge.from_leaf == member);
            let mut has_certain = false;
            let mut all_inside = true;
            for edge in from_member {
                has_certain |= edge.certain;
                all_inside &= inside(edge.to_leaf);
            }
            has_certain && all_inside
        });
        let mut transition_indices: Vec<usize> = component_edges
            .iter()
            .filter(|edge| inside(edge.to_leaf))
            .map(|edge| edge.transition_idx)
            .collect();
        transition_indices.sort_unstable();
        transition_indices.dedup();
        let states: Vec<&str> = component
            .iter()
            .map(|&member| t.names[member as usize].as_str())
            .collect();
        let indices = transition_indices
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let path = format!("/transitions/{}", transition_indices[0]);
        if inescapable {
            out.push(Finding::err(
                "def/eventless_cycle",
                path,
                format!(
                    "eventless transitions {indices} cycle through {} and no guard can stop them",
                    states.join(", ")
                ),
                "the machine can never quiesce; guard one transition on the cycle, or point it at a state outside the cycle",
            ));
        } else {
            out.push(Finding::warn(
                "def/eventless_cycle_guarded",
                path,
                format!(
                    "eventless transitions {indices} form a cycle through {} that only a guard can break",
                    states.join(", ")
                ),
                format!(
                    "the engine cannot decide the guard at admission; a macrostep that never settles is refused after {} reactions as run/microstep_limit",
                    crate::limits::MAX_MICROSTEPS
                ),
            ));
        }
    }
    let region_count = match &m.spec.topology {
        crate::spec::Topology::Sequential { .. } => 1,
        crate::spec::Topology::Parallel { regions } => regions.len(),
    };
    let longest = longest_cascade(&components, &component_of, &edges);
    let shared = longest * region_count;
    if shared >= crate::limits::MAX_MICROSTEPS as usize / 2 {
        out.push(Finding::warn(
            "def/eventless_depth",
            "/transitions",
            format!(
                "the longest eventless cascade is {longest} microsteps and {region_count} region(s) share one ceiling: {shared} of the {} reactions a macrostep allows",
                crate::limits::MAX_MICROSTEPS
            ),
            "shorten the cascade or merge decisions so one macrostep stays well under the ceiling",
        ));
    }
    out
}

/// The longest path, in reactions, through the condensation of the eventless
/// graph. Tarjan emits components in reverse topological order, so every
/// successor component is finished before the component that reaches it.
fn longest_cascade(
    components: &[Vec<u16>],
    component_of: &[usize],
    edges: &[EventlessEdge],
) -> usize {
    let mut longest = vec![0usize; components.len()];
    for (component_index, component) in components.iter().enumerate() {
        for &member in component {
            for edge in edges.iter().filter(|edge| edge.from_leaf == member) {
                let successor = component_of[edge.to_leaf as usize];
                if successor != component_index {
                    longest[component_index] = longest[component_index].max(longest[successor] + 1);
                }
            }
        }
    }
    longest.into_iter().max().unwrap_or(0)
}

pub fn analyze_all(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let mut out = reachability_findings(m, t);
    out.extend(shadowing_findings(m));
    out.extend(ancestor_shadowed(m, t));
    out.extend(create_always_fails(m, t));
    out.extend(eventless_noop_findings(m));
    out.extend(eventless_cycle_findings(m, t));
    out
}
