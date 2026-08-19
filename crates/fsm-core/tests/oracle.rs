//! Naive event and deadline interpreter: recursive spec walks, no `Tree`
//! tables, compiled expression slots, transition lookup, or deadline selector.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Bindings, Budget, Val, eval};
use fsm_core::expr::parser;
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, EnforceMode, InstanceState, Status};
use fsm_core::spec::{Block, DeadlineSpec, HistoryKind, MachineSpec, StateNode, Topology};
use fsm_core::step::{
    Applied, DeadlineApplied, DeadlineOutcome, DeadlineRejected, EffectOut, Outcome,
    PendingDeadline, Rejection,
};

fn find<'a>(nodes: &'a [StateNode], name: &str) -> Option<&'a StateNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.states, name) {
            return Some(f);
        }
    }
    None
}

fn sequential_topology(spec: &MachineSpec) -> (&[StateNode], &str) {
    match &spec.topology {
        Topology::Sequential { states, initial } => (states, initial),
        Topology::Parallel { .. } => panic!("this oracle operation requires one region"),
    }
}

struct ActiveLeaf<'a> {
    region: Option<&'a str>,
    states: &'a [StateNode],
    leaf: String,
}

fn is_real_leaf(states: &[StateNode], name: &str) -> bool {
    find(states, name).is_some_and(|node| node.states.is_empty() && node.history.is_none())
}

/// Reconstruct active leaves directly from the definition and tagged public
/// configuration. This deliberately does not consult `Tree::active_leaves`.
fn active_leaves<'a>(
    spec: &'a MachineSpec,
    configuration: &ActiveConfiguration,
) -> Option<Vec<ActiveLeaf<'a>>> {
    match (&spec.topology, configuration) {
        (Topology::Sequential { states, .. }, ActiveConfiguration::Sequential { leaf })
            if is_real_leaf(states, leaf) =>
        {
            Some(vec![ActiveLeaf {
                region: None,
                states,
                leaf: leaf.clone(),
            }])
        }
        (Topology::Parallel { regions }, ActiveConfiguration::Parallel { leaves })
            if leaves.len() == regions.len() =>
        {
            let mut active = Vec::with_capacity(regions.len());
            for region in regions {
                let leaf = leaves.get(&region.name)?;
                if !is_real_leaf(&region.states, leaf) {
                    return None;
                }
                active.push(ActiveLeaf {
                    region: Some(&region.name),
                    states: &region.states,
                    leaf: leaf.clone(),
                });
            }
            Some(active)
        }
        _ => None,
    }
}

fn configuration_is_terminal(spec: &MachineSpec, configuration: &ActiveConfiguration) -> bool {
    active_leaves(spec, configuration).is_some_and(|active| {
        !active.is_empty()
            && active
                .into_iter()
                .all(|leaf| find(leaf.states, &leaf.leaf).is_some_and(|node| node.terminal))
    })
}

fn parent_of(nodes: &[StateNode], name: &str) -> Option<String> {
    fn rec(nodes: &[StateNode], name: &str, parent: Option<&str>) -> Option<String> {
        for n in nodes {
            if n.name == name {
                return parent.map(str::to_string);
            }
            if let Some(p) = rec(&n.states, name, Some(&n.name)) {
                return Some(p);
            }
        }
        None
    }
    rec(nodes, name, None)
}

fn chain(states: &[StateNode], leaf: &str) -> Vec<String> {
    let mut out = vec![leaf.to_string()];
    let mut cur = leaf.to_string();
    while let Some(p) = parent_of(states, &cur) {
        out.push(p.clone());
        cur = p;
    }
    out
}

fn depth(states: &[StateNode], name: &str) -> u32 {
    chain(states, name).len() as u32
}

fn lca(states: &[StateNode], a: &str, b: &str) -> Option<String> {
    let mut x = parent_of(states, a);
    let mut y = parent_of(states, b);
    while depth_opt(states, &x) > depth_opt(states, &y) {
        x = x.and_then(|n| parent_of(states, &n));
    }
    while depth_opt(states, &y) > depth_opt(states, &x) {
        y = y.and_then(|n| parent_of(states, &n));
    }
    while x != y {
        x = x.and_then(|n| parent_of(states, &n));
        y = y.and_then(|n| parent_of(states, &n));
    }
    x
}

fn depth_opt(states: &[StateNode], n: &Option<String>) -> u32 {
    n.as_ref().map(|s| depth(states, s)).unwrap_or(0)
}

fn exit_set(states: &[StateNode], leaf: &str, dom: &Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Some(leaf.to_string());
    while let Some(n) = cur {
        if Some(&n) == dom.as_ref() {
            break;
        }
        out.push(n.clone());
        cur = parent_of(states, &n);
    }
    out
}

fn entry_path(states: &[StateNode], dom: &Option<String>, target: &str) -> Vec<String> {
    let mut walk = Vec::new();
    let mut cur = Some(target.to_string());
    while let Some(n) = cur {
        if Some(&n) == dom.as_ref() {
            break;
        }
        walk.push(n.clone());
        cur = parent_of(states, &n);
    }
    walk.reverse();
    walk
}

fn initial_of(node: &StateNode) -> Option<&str> {
    node.initial.as_deref()
}

fn initial_descent(states: &[StateNode], from: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = find(states, from).and_then(initial_of).map(str::to_string);
    while let Some(n) = cur {
        out.push(n.clone());
        cur = find(states, &n).and_then(initial_of).map(str::to_string);
    }
    out
}

fn hist_descent(states: &[StateNode], hist: &str, binding: Option<&str>) -> Vec<String> {
    let owner = parent_of(states, hist).unwrap();
    let kind = find(states, hist).and_then(|n| n.history);
    match (kind, binding) {
        (_, None) => initial_descent(states, &owner),
        (Some(HistoryKind::Deep), Some(b)) => entry_path(states, &Some(owner), b),
        (Some(HistoryKind::Shallow), Some(b)) => {
            let mut v = vec![b.to_string()];
            v.extend(initial_descent(states, b));
            v
        }
        _ => initial_descent(states, &owner),
    }
}

fn eval_bool(
    src: Option<&str>,
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<bool, Rejection> {
    match src {
        None => budget
            .tick(fsm_core::expr::lexer::Span::new(0, 4))
            .map(|()| true)
            .map_err(|err| Rejection {
                code: "run/guard_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                span: Some((err.span.start, err.span.end)),
                trace: Default::default(),
                cause: Some(err.code),
            }),
        Some(s) => {
            let e = parser::parse(s).map_err(|err| Rejection {
                code: "run/guard_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                span: Some((err.span.start, err.span.end)),
                trace: Default::default(),
                cause: None,
            })?;
            let b = Bindings {
                ctx,
                evt: Some(evt),
            };
            match eval(&e, &b, budget, false).0 {
                Ok(Val::Bool(v)) => Ok(v),
                Err(err) => Err(Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    trace: Default::default(),
                    cause: None,
                }),
                _ => Ok(false),
            }
        }
    }
}

fn parse_init(s: &str, ty: &fsm_core::spec::TySpec) -> Result<Val, Rejection> {
    use fsm_core::spec::TySpec;
    let reject = |code: &'static str| Rejection {
        code,
        message: s.into(),
        hint: "bad init".into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: Default::default(),
        cause: None,
    };
    match ty {
        TySpec::Int => s
            .parse()
            .map(Val::Int)
            .map_err(|_| reject("req/field_type")),
        TySpec::Bool => match s {
            "true" => Ok(Val::Bool(true)),
            "false" => Ok(Val::Bool(false)),
            _ => Err(reject("req/field_type")),
        },
        TySpec::Str => Ok(Val::Str(s.into())),
        TySpec::Ts => s.parse().map(Val::Ts).map_err(|_| reject("req/field_type")),
        TySpec::Dur => s
            .parse()
            .map(Val::Dur)
            .map_err(|_| reject("req/field_type")),
        TySpec::Dec { scale } => fsm_core::decimal::Dec::parse(s, *scale)
            .map(Val::Dec)
            .map_err(|_| reject("req/field_type")),
        TySpec::Enum { of } => Ok(Val::Enum {
            ty: of.clone(),
            variant: s.into(),
        }),
    }
}

fn apply_sets(
    sets: &[fsm_core::spec::SetSpec],
    ctx: &mut BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
) -> Result<(), Rejection> {
    let snapshot = ctx.clone();
    let b = Bindings {
        ctx: &snapshot,
        evt: if see_evt { Some(evt) } else { None },
    };
    let mut next = ctx.clone();
    for s in sets {
        let e = parser::parse(&s.value).map_err(|err| Rejection {
            code: "run/action_error",
            message: err.message,
            hint: err.hint,
            source_state: None,
            transition_idx: None,
            block: None,
            span: Some((err.span.start, err.span.end)),
            trace: Default::default(),
            cause: Some(err.code),
        })?;
        let (v, _) = eval(&e, &b, budget, false);
        let v = v.map_err(|err| Rejection {
            code: "run/action_error",
            message: err.message,
            hint: err.hint,
            source_state: None,
            transition_idx: None,
            block: None,
            span: Some((err.span.start, err.span.end)),
            trace: Default::default(),
            cause: Some(err.code),
        })?;
        next.insert(s.target.clone(), v);
    }
    *ctx = next;
    Ok(())
}

fn apply_emits(
    emits: &[fsm_core::spec::EmitSpec],
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
) -> Result<(), Rejection> {
    let b = Bindings {
        ctx,
        evt: if see_evt { Some(evt) } else { None },
    };
    for em in emits {
        let mut args = BTreeMap::new();
        for (k, src) in &em.args {
            let e = parser::parse(src).map_err(|err| Rejection {
                code: "run/action_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                span: Some((err.span.start, err.span.end)),
                trace: Default::default(),
                cause: Some(err.code),
            })?;
            let (v, _) = eval(&e, &b, budget, false);
            let v = v.map_err(|err| Rejection {
                code: "run/action_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                span: Some((err.span.start, err.span.end)),
                trace: Default::default(),
                cause: Some(err.code),
            })?;
            args.insert(k.clone(), v);
        }
        let k = effects.len() as u32;
        effects.push(EffectOut {
            name: em.effect.clone(),
            args,
            k,
        });
    }
    Ok(())
}

fn apply_block(
    block: &fsm_core::spec::Block,
    ctx: &mut BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
) -> Result<(), Rejection> {
    let snapshot = ctx.clone();
    apply_sets(&block.sets, ctx, evt, see_evt, budget)?;
    apply_emits(&block.emits, &snapshot, evt, see_evt, budget, effects)?;
    Ok(())
}

fn naive_validate(
    spec: &MachineSpec,
    name: &str,
    payload: &Value,
) -> Result<BTreeMap<String, Val>, Rejection> {
    let ev = spec
        .events
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| reject("req/event_unknown", name))?;
    let obj = match payload {
        Value::Obj(o) => o,
        _ => return Err(reject("req/field_type", "payload must be an object")),
    };
    let mut out = BTreeMap::new();
    for f in &ev.fields {
        let Some(raw) = obj.get(&f.name) else {
            return Err(reject("req/field_missing", &f.name));
        };
        if raw.as_num().is_some() {
            return Err(reject("req/number_token", &f.name));
        }
        let v = parse_typed(raw, &f.ty).map_err(|c| reject(c, &f.name))?;
        if let Val::Enum { ty, variant } = &v {
            let allowed = spec.enums.get(ty).cloned().unwrap_or_default();
            if !allowed.iter().any(|x| x == variant) {
                return Err(reject("req/field_type", &f.name));
            }
        }
        if let (Val::Dec(d), fsm_core::spec::TySpec::Dec { scale }) = (&v, &f.ty) {
            if d.scale != *scale {
                return Err(reject("req/field_scale", &f.name));
            }
        }
        out.insert(f.name.clone(), v);
    }
    for k in obj.keys() {
        if !ev.fields.iter().any(|f| f.name == *k) {
            return Err(reject("req/field_unknown", k));
        }
    }
    Ok(out)
}

fn parse_typed(raw: &Value, ty: &fsm_core::spec::TySpec) -> Result<Val, &'static str> {
    use fsm_core::spec::TySpec;
    match ty {
        TySpec::Bool => raw.as_bool().map(Val::Bool).ok_or("req/field_type"),
        TySpec::Str => raw
            .as_str()
            .map(|s| Val::Str(s.into()))
            .ok_or("req/field_type"),
        TySpec::Int => raw
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(Val::Int)
            .ok_or("req/field_type"),
        TySpec::Ts => raw
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(Val::Ts)
            .ok_or("req/field_type"),
        TySpec::Dur => raw
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(Val::Dur)
            .ok_or("req/field_type"),
        TySpec::Dec { scale } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            match fsm_core::decimal::Dec::parse(s, *scale) {
                Ok(d) => Ok(Val::Dec(d)),
                Err(fsm_core::decimal::DecError::Parse) => {
                    if s.contains('.')
                        && s.split('.').nth(1).map(|f| f.len()).unwrap_or(0) > *scale as usize
                    {
                        Err("req/field_scale")
                    } else {
                        Err("req/field_type")
                    }
                }
                Err(_) => Err("req/field_type"),
            }
        }
        TySpec::Enum { of } => raw
            .as_str()
            .map(|s| Val::Enum {
                ty: of.clone(),
                variant: s.into(),
            })
            .ok_or("req/field_type"),
    }
}

fn reject(code: &'static str, what: &str) -> Rejection {
    Rejection {
        code,
        message: format!("{code}: {what}"),
        hint: what.into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: Default::default(),
        cause: None,
    }
}

fn eval_invariants(
    spec: &MachineSpec,
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<Vec<String>, Rejection> {
    let mut flags = Vec::new();
    for inv in &spec.invariants {
        match eval_bool(Some(inv.expr.as_str()), ctx, &BTreeMap::new(), budget) {
            Ok(true) => {}
            Ok(false) => match inv.mode {
                EnforceMode::Monitor => flags.push(inv.name.clone()),
                EnforceMode::Enforce => {
                    return Err(Rejection {
                        code: "run/invariant",
                        message: inv.name.clone(),
                        hint: "fix context".into(),
                        source_state: None,
                        transition_idx: None,
                        block: None,
                        span: None,
                        trace: Default::default(),
                        cause: None,
                    });
                }
            },
            Err(r) => return Err(r),
        }
    }
    Ok(flags)
}

fn is_compound(n: &StateNode) -> bool {
    !n.states.is_empty() && n.history.is_none()
}

fn apply_entry_chain(
    states: &[StateNode],
    start: &str,
    ctx: &mut BTreeMap<String, Val>,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
) -> Result<Vec<String>, Rejection> {
    let mut entered = vec![start.to_string()];
    if let Some(node) = find(states, start) {
        if let Some(b) = &node.entry {
            apply_block(b, ctx, &BTreeMap::new(), false, budget, effects)?;
        }
    }
    entered.extend(initial_descent(states, start));
    for name in entered.iter().skip(1) {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.entry {
                apply_block(b, ctx, &BTreeMap::new(), false, budget, effects)?;
            }
        }
    }
    Ok(entered)
}

fn deadline_rejection(
    deadline: &DeadlineSpec,
    message: String,
    hint: String,
    span: Option<(u32, u32)>,
    cause: Option<&'static str>,
) -> Rejection {
    Rejection {
        code: "run/action_error",
        message,
        hint,
        source_state: Some(deadline.from.clone()),
        transition_idx: None,
        block: Some(format!("deadline({})", deadline.name)),
        span,
        trace: Default::default(),
        cause,
    }
}

fn evaluate_deadline_after(
    deadline: &DeadlineSpec,
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<i64, Rejection> {
    let expression = parser::parse(&deadline.after).map_err(|error| {
        deadline_rejection(
            deadline,
            error.message,
            error.hint,
            Some((error.span.start, error.span.end)),
            Some(error.code),
        )
    })?;
    let bindings = Bindings { ctx, evt: None };
    match eval(&expression, &bindings, budget, false).0 {
        Ok(Val::Dur(duration)) if duration >= 0 => Ok(duration),
        Ok(Val::Dur(_)) => Err(deadline_rejection(
            deadline,
            "deadline duration is negative".into(),
            "return a zero or positive duration".into(),
            None,
            Some("run/overflow"),
        )),
        Ok(_) => Err(deadline_rejection(
            deadline,
            "deadline expression did not return a duration".into(),
            "return a duration".into(),
            None,
            None,
        )),
        Err(error) => Err(deadline_rejection(
            deadline,
            error.message,
            error.hint,
            Some((error.span.start, error.span.end)),
            Some(error.code),
        )),
    }
}

/// Rebuild schedules by literally applying SPEC's exit cancellation followed
/// by entry-order/document-order scheduling. This deliberately does not read
/// production expression slots or deadline indices.
fn update_deadline_schedules(
    spec: &MachineSpec,
    prior: &BTreeMap<String, i64>,
    exited: &[String],
    entered: &[String],
    ctx: &BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<BTreeMap<String, i64>, Rejection> {
    let mut schedules = prior.clone();
    for state in exited {
        for deadline in spec
            .deadlines
            .iter()
            .filter(|deadline| deadline.from == *state)
        {
            schedules.remove(&deadline.name);
        }
    }
    for state in entered {
        for deadline in spec
            .deadlines
            .iter()
            .filter(|deadline| deadline.from == *state)
        {
            let duration = evaluate_deadline_after(deadline, ctx, budget)?;
            let due_ms = now_ms.checked_add(duration).ok_or_else(|| {
                deadline_rejection(
                    deadline,
                    "deadline due timestamp overflowed".into(),
                    "use a smaller timestamp or duration".into(),
                    None,
                    Some("run/overflow"),
                )
            })?;
            schedules.insert(deadline.name.clone(), due_ms);
        }
    }
    Ok(schedules)
}

fn clear_terminal_region_deadlines(
    spec: &MachineSpec,
    configuration: &ActiveConfiguration,
    schedules: &mut BTreeMap<String, i64>,
) {
    let Some(active) = active_leaves(spec, configuration) else {
        return;
    };
    for active_leaf in active {
        if !find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
            continue;
        }
        let terminal_chain = chain(active_leaf.states, &active_leaf.leaf);
        schedules.retain(|name, _| {
            spec.deadlines
                .iter()
                .find(|deadline| deadline.name == *name)
                .is_none_or(|deadline| !terminal_chain.contains(&deadline.from))
        });
    }
}

pub fn naive_create(
    m: &CompiledMachine,
    overrides: &BTreeMap<String, Val>,
) -> Result<Applied, Rejection> {
    naive_create_at(m, overrides, 0)
}

pub fn naive_create_at(
    m: &CompiledMachine,
    overrides: &BTreeMap<String, Val>,
    now_ms: i64,
) -> Result<Applied, Rejection> {
    let mut ctx = BTreeMap::new();
    let mut budget = Budget::new(4096);
    for c in &m.spec.context {
        let v = if let Some(ov) = overrides.get(&c.name) {
            ov.clone()
        } else {
            parse_init(&c.init, &c.ty)?
        };
        ctx.insert(c.name.clone(), v);
    }
    let mut effects = Vec::new();
    let mut entered = Vec::new();
    let configuration_after = match &m.spec.topology {
        Topology::Sequential { states, initial } => {
            let path = apply_entry_chain(states, initial, &mut ctx, &mut budget, &mut effects)?;
            let leaf = path.last().cloned().unwrap_or_else(|| initial.to_string());
            entered.extend(path);
            ActiveConfiguration::Sequential { leaf }
        }
        Topology::Parallel { regions } => {
            let mut leaves = BTreeMap::new();
            for region in regions {
                let path = apply_entry_chain(
                    &region.states,
                    &region.initial,
                    &mut ctx,
                    &mut budget,
                    &mut effects,
                )?;
                let leaf = path
                    .last()
                    .cloned()
                    .unwrap_or_else(|| region.initial.clone());
                leaves.insert(region.name.clone(), leaf);
                entered.extend(path);
            }
            ActiveConfiguration::Parallel { leaves }
        }
    };
    let flags = eval_invariants(&m.spec, &ctx, &mut budget)?;
    let mut deadlines_after = update_deadline_schedules(
        &m.spec,
        &BTreeMap::new(),
        &[],
        &entered,
        &ctx,
        now_ms,
        &mut budget,
    )?;
    clear_terminal_region_deadlines(&m.spec, &configuration_after, &mut deadlines_after);
    let status_after = if configuration_is_terminal(&m.spec, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Ok(Applied {
        configuration_after,
        ctx_after: ctx,
        history_after: BTreeMap::new(),
        deadlines_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal: false,
        region: None,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered,
        trace: Default::default(),
    })
}

struct SelectedCandidate {
    region: Option<String>,
    leaf: String,
    source: String,
    transition_index: usize,
}

/// Apply SPEC's global scan literally: regions in document order, then each
/// recursive leaf-to-root chain, then transitions in document order.
fn select_event_candidate(
    spec: &MachineSpec,
    active: &[ActiveLeaf<'_>],
    state: &InstanceState,
    event: &str,
    fields: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<(Option<SelectedCandidate>, bool), Rejection> {
    let mut any_candidate = false;
    for active_leaf in active {
        if find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
            continue;
        }
        for source in chain(active_leaf.states, &active_leaf.leaf) {
            for (transition_index, transition) in spec.transitions.iter().enumerate() {
                if transition.from != source || transition.on != event {
                    continue;
                }
                any_candidate = true;
                match eval_bool(transition.guard.as_deref(), &state.ctx, fields, budget) {
                    Ok(true) => {
                        return Ok((
                            Some(SelectedCandidate {
                                region: active_leaf.region.map(str::to_string),
                                leaf: active_leaf.leaf.clone(),
                                source,
                                transition_index,
                            }),
                            true,
                        ));
                    }
                    Ok(false) => {}
                    Err(rejection) => return Err(rejection),
                }
            }
        }
    }
    Ok((None, any_candidate))
}

fn states_for_region<'a>(
    spec: &'a MachineSpec,
    region_name: Option<&str>,
) -> Option<&'a [StateNode]> {
    match (&spec.topology, region_name) {
        (Topology::Sequential { states, .. }, None) => Some(states),
        (Topology::Parallel { regions }, Some(region_name)) => regions
            .iter()
            .find(|region| region.name == region_name)
            .map(|region| region.states.as_slice()),
        _ => None,
    }
}

fn configuration_with_leaf(
    configuration: &ActiveConfiguration,
    region_name: Option<&str>,
    leaf: String,
) -> Option<ActiveConfiguration> {
    match (configuration, region_name) {
        (ActiveConfiguration::Sequential { .. }, None) => {
            Some(ActiveConfiguration::Sequential { leaf })
        }
        (ActiveConfiguration::Parallel { leaves }, Some(region_name))
            if leaves.contains_key(region_name) =>
        {
            let mut leaves = leaves.clone();
            leaves.insert(region_name.to_string(), leaf);
            Some(ActiveConfiguration::Parallel { leaves })
        }
        _ => None,
    }
}

pub fn naive_step(
    m: &CompiledMachine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    budget: &mut Budget,
) -> Outcome {
    naive_step_at(m, st, event, payload, 0, budget)
}

pub fn naive_step_at(
    m: &CompiledMachine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    if st.status != Status::Running {
        return Outcome::Rejected(Rejection {
            code: if st.status == Status::Completed {
                "run/instance_completed"
            } else {
                "run/instance_cancelled"
            },
            message: "not running".into(),
            hint: "create a new instance".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
            cause: None,
        });
    }
    let fields = match naive_validate(&m.spec, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let active = match active_leaves(&m.spec, &st.configuration) {
        Some(active) => active,
        None => return Outcome::Rejected(reject("run/unhandled", "invalid configuration")),
    };
    let (winner, any_candidate) =
        match select_event_candidate(&m.spec, &active, st, event, &fields, budget) {
            Ok(selection) => selection,
            Err(rejection) => return Outcome::Rejected(rejection),
        };
    let Some(winner) = winner else {
        if !any_candidate {
            return match m.spec.on_unhandled {
                fsm_core::spec::Unhandled::Ignore => Outcome::Ignored,
                fsm_core::spec::Unhandled::Reject => Outcome::Rejected(Rejection {
                    code: "run/unhandled",
                    message: "unhandled".into(),
                    hint: "n".into(),
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: None,
                    trace: Default::default(),
                    cause: None,
                }),
            };
        }
        return Outcome::Rejected(Rejection {
            code: "run/not_enabled",
            message: "not enabled".into(),
            hint: "n".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
            cause: None,
        });
    };
    let Some(states) = states_for_region(&m.spec, winner.region.as_deref()) else {
        return Outcome::Rejected(reject("run/unhandled", "invalid transition region"));
    };
    let tr = &m.spec.transitions[winner.transition_index];
    let internal = tr.to.is_none();
    let (exited, entered, new_leaf) = if internal {
        (Vec::new(), Vec::new(), winner.leaf.clone())
    } else {
        let mut target = tr.to.clone().unwrap();
        let mut extra = Vec::new();
        if let Some(tn) = find(states, &target) {
            if tn.history.is_some() {
                let owner = parent_of(states, &target).unwrap();
                extra = hist_descent(states, &target, st.history.get(&owner).map(String::as_str));
                target = owner;
            }
        }
        let external_self = tr.to.as_deref() == Some(winner.source.as_str());
        let dom = if external_self {
            parent_of(states, &winner.source)
        } else {
            lca(states, &winner.source, &target)
        };
        let exited = exit_set(states, &winner.leaf, &dom);
        let mut entered = entry_path(states, &dom, &target);
        if find(states, &target).is_some_and(is_compound) && extra.is_empty() {
            entered.extend(initial_descent(states, &target));
        }
        entered.extend(extra);
        let leaf = entered.last().cloned().unwrap_or(target);
        (exited, entered, leaf)
    };
    let Some(configuration_after) =
        configuration_with_leaf(&st.configuration, winner.region.as_deref(), new_leaf)
    else {
        return Outcome::Rejected(reject("run/unhandled", "invalid transition region"));
    };
    let mut ctx = st.ctx.clone();
    let mut effects = Vec::new();
    for name in &exited {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.exit {
                if let Err(r) = apply_block(b, &mut ctx, &fields, false, budget, &mut effects) {
                    return Outcome::Rejected(r);
                }
            }
        }
    }
    let tblock = fsm_core::spec::Block {
        sets: tr.sets.clone(),
        emits: tr.emits.clone(),
    };
    if let Err(r) = apply_block(&tblock, &mut ctx, &fields, true, budget, &mut effects) {
        return Outcome::Rejected(r);
    }
    for name in &entered {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.entry {
                if let Err(r) = apply_block(b, &mut ctx, &fields, false, budget, &mut effects) {
                    return Outcome::Rejected(r);
                }
            }
        }
    }
    let mut history_after = st.history.clone();
    for name in &exited {
        if let Some(node) = find(states, name) {
            if is_compound(node) {
                for ch in &node.states {
                    if let Some(hk) = ch.history {
                        let bound = match hk {
                            HistoryKind::Deep => winner.leaf.clone(),
                            HistoryKind::Shallow => chain(states, &winner.leaf)
                                .into_iter()
                                .find(|n| parent_of(states, n).as_deref() == Some(name.as_str()))
                                .unwrap_or_else(|| winner.leaf.clone()),
                        };
                        history_after.insert(name.clone(), bound);
                    }
                }
            }
        }
    }
    let flags = match eval_invariants(&m.spec, &ctx, budget) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let mut deadlines_after = match update_deadline_schedules(
        &m.spec,
        &st.deadlines,
        &exited,
        &entered,
        &ctx,
        now_ms,
        budget,
    ) {
        Ok(schedules) => schedules,
        Err(rejection) => return Outcome::Rejected(rejection),
    };
    clear_terminal_region_deadlines(&m.spec, &configuration_after, &mut deadlines_after);
    let status_after = if configuration_is_terminal(&m.spec, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Outcome::Applied(Applied {
        configuration_after,
        ctx_after: ctx,
        history_after,
        deadlines_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal,
        region: winner.region,
        source_state: winner.source,
        transition_idx: winner.transition_index as u32,
        exited,
        entered,
        trace: Default::default(),
    })
}

struct SelectedDeadline {
    due_ms: i64,
    document_index: usize,
    region: Option<String>,
    leaf: String,
    source: String,
}

/// Select the minimum active `(due_ms, document index)` by scanning the
/// definition and recursive active chains directly. No production selector or
/// compiled deadline index participates in this oracle.
fn select_deadline(
    spec: &MachineSpec,
    active: &[ActiveLeaf<'_>],
    schedules: &BTreeMap<String, i64>,
) -> Option<SelectedDeadline> {
    let mut selected: Option<SelectedDeadline> = None;
    for (document_index, deadline) in spec.deadlines.iter().enumerate() {
        let Some(&due_ms) = schedules.get(&deadline.name) else {
            continue;
        };
        let source = active.iter().find_map(|active_leaf| {
            if find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
                return None;
            }
            chain(active_leaf.states, &active_leaf.leaf)
                .into_iter()
                .find(|state| state == &deadline.from)
                .map(|source| {
                    (
                        active_leaf.region.map(str::to_string),
                        active_leaf.leaf.clone(),
                        source,
                    )
                })
        });
        let Some((region, leaf, source)) = source else {
            continue;
        };
        let candidate = SelectedDeadline {
            due_ms,
            document_index,
            region,
            leaf,
            source,
        };
        if selected.as_ref().is_none_or(|current| {
            (candidate.due_ms, candidate.document_index) < (current.due_ms, current.document_index)
        }) {
            selected = Some(candidate);
        }
    }
    selected
}

fn apply_naive_deadline(
    m: &CompiledMachine,
    st: &InstanceState,
    selected: &SelectedDeadline,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    let Some(states) = states_for_region(&m.spec, selected.region.as_deref()) else {
        return Outcome::Rejected(reject(
            "run/configuration_invalid",
            "invalid deadline region",
        ));
    };
    let deadline = &m.spec.deadlines[selected.document_index];
    let mut target = deadline.to.clone();
    let mut extra = Vec::new();
    if let Some(target_node) = find(states, &target)
        && target_node.history.is_some()
    {
        let owner = parent_of(states, &target).expect("compiled history has an owner");
        extra = hist_descent(states, &target, st.history.get(&owner).map(String::as_str));
        target = owner;
    }
    let domain = if deadline.to == selected.source {
        parent_of(states, &selected.source)
    } else {
        lca(states, &selected.source, &target)
    };
    let exited = exit_set(states, &selected.leaf, &domain);
    let mut entered = entry_path(states, &domain, &target);
    if find(states, &target).is_some_and(is_compound) && extra.is_empty() {
        entered.extend(initial_descent(states, &target));
    }
    entered.extend(extra);
    let new_leaf = entered.last().cloned().unwrap_or(target);
    let Some(configuration_after) =
        configuration_with_leaf(&st.configuration, selected.region.as_deref(), new_leaf)
    else {
        return Outcome::Rejected(reject(
            "run/configuration_invalid",
            "invalid deadline region",
        ));
    };

    let mut ctx = st.ctx.clone();
    let mut effects = Vec::new();
    let no_event = BTreeMap::new();
    for name in &exited {
        if let Some(block) = find(states, name).and_then(|node| node.exit.as_ref())
            && let Err(rejection) =
                apply_block(block, &mut ctx, &no_event, false, budget, &mut effects)
        {
            return Outcome::Rejected(rejection);
        }
    }
    let deadline_block = Block {
        sets: deadline.sets.clone(),
        emits: deadline.emits.clone(),
    };
    if let Err(rejection) = apply_block(
        &deadline_block,
        &mut ctx,
        &no_event,
        false,
        budget,
        &mut effects,
    ) {
        return Outcome::Rejected(rejection);
    }
    for name in &entered {
        if let Some(block) = find(states, name).and_then(|node| node.entry.as_ref())
            && let Err(rejection) =
                apply_block(block, &mut ctx, &no_event, false, budget, &mut effects)
        {
            return Outcome::Rejected(rejection);
        }
    }

    let mut history_after = st.history.clone();
    for name in &exited {
        if let Some(node) = find(states, name)
            && is_compound(node)
        {
            for child in &node.states {
                if let Some(kind) = child.history {
                    let bound = match kind {
                        HistoryKind::Deep => selected.leaf.clone(),
                        HistoryKind::Shallow => chain(states, &selected.leaf)
                            .into_iter()
                            .find(|state| {
                                parent_of(states, state).as_deref() == Some(name.as_str())
                            })
                            .unwrap_or_else(|| selected.leaf.clone()),
                    };
                    history_after.insert(name.clone(), bound);
                }
            }
        }
    }
    let monitor_flags = match eval_invariants(&m.spec, &ctx, budget) {
        Ok(flags) => flags,
        Err(rejection) => return Outcome::Rejected(rejection),
    };
    let mut deadlines_after = match update_deadline_schedules(
        &m.spec,
        &st.deadlines,
        &exited,
        &entered,
        &ctx,
        now_ms,
        budget,
    ) {
        Ok(schedules) => schedules,
        Err(rejection) => return Outcome::Rejected(rejection),
    };
    clear_terminal_region_deadlines(&m.spec, &configuration_after, &mut deadlines_after);
    let status_after = if configuration_is_terminal(&m.spec, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Outcome::Applied(Applied {
        configuration_after,
        ctx_after: ctx,
        history_after,
        deadlines_after,
        effects,
        monitor_flags,
        status_after,
        internal: false,
        region: selected.region.clone(),
        source_state: selected.source.clone(),
        transition_idx: selected.document_index as u32,
        exited,
        entered,
        trace: Default::default(),
    })
}

pub fn naive_poll_deadline(
    m: &CompiledMachine,
    st: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> DeadlineOutcome {
    if st.status != Status::Running {
        return DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: None,
            rejection: reject(
                if st.status == Status::Completed {
                    "run/instance_completed"
                } else {
                    "run/instance_cancelled"
                },
                "instance is not running",
            ),
        });
    }
    let Some(active) = active_leaves(&m.spec, &st.configuration) else {
        return DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: None,
            rejection: reject("run/configuration_invalid", "invalid configuration"),
        });
    };
    let Some(selected) = select_deadline(&m.spec, &active, &st.deadlines) else {
        return DeadlineOutcome::NotDue { next: None };
    };
    let pending = PendingDeadline {
        name: m.spec.deadlines[selected.document_index].name.clone(),
        deadline_idx: selected.document_index as u32,
        due_ms: selected.due_ms,
    };
    if selected.due_ms > now_ms {
        return DeadlineOutcome::NotDue {
            next: Some(pending),
        };
    }
    match apply_naive_deadline(m, st, &selected, now_ms, budget) {
        Outcome::Applied(transition) => DeadlineOutcome::Applied(DeadlineApplied {
            deadline: pending,
            transition,
        }),
        Outcome::Rejected(rejection) => DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: Some(pending),
            rejection,
        }),
        Outcome::Ignored => panic!("a selected deadline is never ignored"),
    }
}

pub fn brute_enterable(m: &CompiledMachine) -> std::collections::BTreeSet<String> {
    let (nodes, initial) = sequential_topology(&m.spec);
    let mut out = std::collections::BTreeSet::new();
    fn add_initial_chain(
        nodes: &[StateNode],
        name: &str,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        out.insert(name.to_string());
        for child in initial_descent(nodes, name) {
            out.insert(child);
        }
    }

    // Creation enters the selected root and follows every declared initial.
    // History pseudostates are never configurations and therefore never enterable.
    add_initial_chain(nodes, initial, &mut out);
    let mut changed = true;
    while changed {
        changed = false;
        for tr in &m.spec.transitions {
            if !out.contains(&tr.from) {
                continue;
            }
            let Some(to) = &tr.to else { continue };
            let before = out.len();
            let target = find(nodes, to).expect("compiled transition target exists");
            if target.history.is_some() {
                // The reachability lemma models a history target as its owner's
                // initial configuration. A binding cannot introduce a state that
                // was not already active on an earlier path.
                let owner = parent_of(nodes, to).expect("history has an owner");
                let dom = lca(nodes, &tr.from, &owner);
                for name in entry_path(nodes, &dom, &owner) {
                    out.insert(name);
                }
                add_initial_chain(nodes, &owner, &mut out);
            } else {
                let dom = lca(nodes, &tr.from, to);
                for name in entry_path(nodes, &dom, to) {
                    out.insert(name);
                }
                if is_compound(target) {
                    for name in initial_descent(nodes, to) {
                        out.insert(name);
                    }
                }
            }
            if out.len() != before {
                changed = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod independence {
    use super::*;
    use fsm_core::json::{JsonLimits, parse};
    use fsm_core::spec::{compile, parse_machine};
    use fsm_core::step::{create, poll_deadline, step};
    use fsm_core::tree::Tree;

    fn tiny() -> CompiledMachine {
        let src = br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
        let v = parse(src, &JsonLimits::DEFAULT).unwrap();
        compile(parse_machine(&v).unwrap()).unwrap()
    }

    #[test]
    fn history_reachability_adds_owner_initial_chain_not_pseudostate() {
        let src = br#"{"format":"fsm.machine/1","name":"reach","states":[{"name":"q","initial":"a","states":[{"name":"h","history":"deep"},{"name":"a"}]},{"name":"x"}],"initial":"x","context":[],"events":[{"name":"back","fields":[]}],"transitions":[{"from":"x","on":"back","to":"h"}]}"#;
        let value = parse(src, &JsonLimits::DEFAULT).unwrap();
        let machine = compile(parse_machine(&value).unwrap()).unwrap();
        let actual = brute_enterable(&machine);
        let expected = ["a", "q", "x"].into_iter().map(str::to_string).collect();
        assert_eq!(actual, expected);
        assert!(!actual.contains("h"));
    }

    #[test]
    fn hierarchical_entry_pipeline_leaf_b_n_11() {
        let src = br#"{"format":"fsm.machine/1","name":"h","states":[{"name":"q","initial":"a","states":[{"name":"a"},{"name":"b","entry":{"do":[{"target":"n","value":"ctx.n + 10"}]}}]}],"initial":"q","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
        let v = parse(src, &JsonLimits::DEFAULT).unwrap();
        let m = compile(parse_machine(&v).unwrap()).unwrap();
        let t = Tree::for_machine(&m.spec);
        let a = naive_create(&m, &BTreeMap::new()).unwrap();
        let e = create(&m, &t, &BTreeMap::new(), 0).unwrap();
        assert_eq!(a.configuration_after, e.configuration_after);
        let st = InstanceState {
            status: a.status_after,
            configuration: a.configuration_after,
            ctx: a.ctx_after,
            history: a.history_after,
            deadlines: BTreeMap::new(),
            pending: vec![],
        };
        let mut b1 = Budget::new(4096);
        let mut b2 = Budget::new(4096);
        let engine = step(&m, &t, &st, "go", &Value::Obj(BTreeMap::new()), 0, &mut b1);
        let naive = naive_step(&m, &st, "go", &Value::Obj(BTreeMap::new()), &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.configuration_after.sequential_leaf(), Some("b"));
                assert_eq!(y.configuration_after.sequential_leaf(), Some("b"));
                assert_eq!(x.ctx_after.get("n"), Some(&Val::Int(11)));
                assert_eq!(y.ctx_after.get("n"), Some(&Val::Int(11)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn oracle_emit_uses_pre_block_context() {
        let src = br#"{"format":"fsm.machine/1","name":"em","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
        let v = parse(src, &JsonLimits::DEFAULT).unwrap();
        let m = compile(parse_machine(&v).unwrap()).unwrap();
        let t = Tree::for_machine(&m.spec);
        let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
        let st = InstanceState {
            status: created.status_after,
            configuration: created.configuration_after,
            ctx: created.ctx_after,
            history: created.history_after,
            deadlines: BTreeMap::new(),
            pending: vec![],
        };
        let mut b1 = Budget::new(4096);
        let mut b2 = Budget::new(4096);
        let engine = step(&m, &t, &st, "e", &Value::Obj(BTreeMap::new()), 0, &mut b1);
        let naive = naive_step(&m, &st, "e", &Value::Obj(BTreeMap::new()), &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.effects, y.effects);
                assert_eq!(x.effects[0].args.get("v"), Some(&Val::Int(0)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn naive_step_matches_engine_and_not_wrong_apply() {
        let m = tiny();
        let t = Tree::for_machine(&m.spec);
        let a = naive_create(&m, &BTreeMap::new()).unwrap();
        let via_engine = create(&m, &t, &BTreeMap::new(), 0).unwrap();
        assert_eq!(a.ctx_after.get("n"), via_engine.ctx_after.get("n"));
        let st = InstanceState {
            status: a.status_after,
            configuration: a.configuration_after,
            ctx: a.ctx_after,
            history: a.history_after,
            deadlines: BTreeMap::new(),
            pending: vec![],
        };
        let mut b1 = Budget::new(4096);
        let mut b2 = Budget::new(4096);
        let engine = step(&m, &t, &st, "e", &Value::Obj(BTreeMap::new()), 0, &mut b1);
        let naive = naive_step(&m, &st, "e", &Value::Obj(BTreeMap::new()), &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.ctx_after.get("n"), y.ctx_after.get("n"));
                assert_eq!(x.ctx_after.get("n"), Some(&Val::Int(1)));
            }
            other => panic!("{other:?}"),
        }
        let mut wrong = st.ctx.clone();
        wrong.insert("n".into(), Val::Int(2));
        assert_ne!(
            match &engine {
                Outcome::Applied(x) => x.ctx_after.get("n").cloned(),
                _ => None,
            },
            wrong.get("n").cloned()
        );
    }

    #[test]
    fn deadline_oracle_selects_document_first_tie_without_production_tables() {
        let src = br#"{"format":"fsm.machine/1","name":"timed","states":[{"name":"waiting"}],"initial":"waiting","context":[{"name":"n","ty":"int","init":"0"}],"events":[],"transitions":[],"deadlines":[{"name":"first","from":"waiting","after":"dur(5, ms)","to":"waiting","do":[{"target":"n","value":"1"}]},{"name":"second","from":"waiting","after":"dur(5, ms)","to":"waiting","do":[{"target":"n","value":"2"}]}]}"#;
        let value = parse(src, &JsonLimits::DEFAULT).unwrap();
        let machine = compile(parse_machine(&value).unwrap()).unwrap();
        let tree = Tree::for_machine(&machine.spec);
        let engine_created = create(&machine, &tree, &BTreeMap::new(), 10).unwrap();
        let oracle_created = naive_create_at(&machine, &BTreeMap::new(), 10).unwrap();
        assert_eq!(
            engine_created.deadlines_after,
            oracle_created.deadlines_after
        );
        let engine_state = InstanceState {
            status: engine_created.status_after,
            configuration: engine_created.configuration_after,
            ctx: engine_created.ctx_after,
            history: engine_created.history_after,
            deadlines: engine_created.deadlines_after,
            pending: Vec::new(),
        };
        let oracle_state = InstanceState {
            status: oracle_created.status_after,
            configuration: oracle_created.configuration_after,
            ctx: oracle_created.ctx_after,
            history: oracle_created.history_after,
            deadlines: oracle_created.deadlines_after,
            pending: Vec::new(),
        };

        let mut engine_budget = Budget::new(4096);
        let mut oracle_budget = Budget::new(4096);
        assert!(matches!(
            (
                poll_deadline(&machine, &tree, &engine_state, 14, &mut engine_budget),
                naive_poll_deadline(&machine, &oracle_state, 14, &mut oracle_budget),
            ),
            (
                DeadlineOutcome::NotDue { next: Some(ref engine) },
                DeadlineOutcome::NotDue { next: Some(ref oracle) },
            ) if engine == oracle && engine.deadline_idx == 0
        ));

        let mut engine_budget = Budget::new(4096);
        let mut oracle_budget = Budget::new(4096);
        match (
            poll_deadline(&machine, &tree, &engine_state, 15, &mut engine_budget),
            naive_poll_deadline(&machine, &oracle_state, 15, &mut oracle_budget),
        ) {
            (DeadlineOutcome::Applied(engine), DeadlineOutcome::Applied(oracle)) => {
                assert_eq!(engine.deadline, oracle.deadline);
                assert_eq!(engine.deadline.deadline_idx, 0);
                assert_eq!(engine.transition.ctx_after, oracle.transition.ctx_after);
                assert_eq!(engine.transition.ctx_after.get("n"), Some(&Val::Int(1)));
            }
            outcomes => panic!("{outcomes:?}"),
        }
    }
}
