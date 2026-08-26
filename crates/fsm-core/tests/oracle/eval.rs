use super::*;

pub(super) fn eval_bool(
    src: Option<&str>,
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    active: Option<&std::collections::BTreeSet<String>>,
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
                active,
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

pub(super) fn parse_init(s: &str, ty: &fsm_core::spec::TySpec) -> Result<Val, Rejection> {
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

pub(super) fn apply_sets(
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
        active: None,
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

pub(super) fn apply_emits(
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
        active: None,
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

/// Events a block raised, in block order, each with its evaluated payload.
pub(super) type Raised = Vec<(String, BTreeMap<String, Val>)>;

/// A raise is an emit turned inward: its `with` expressions read the same
/// pre-block snapshot an emit's arguments read.
pub(super) fn apply_raises(
    raises: &[fsm_core::spec::RaiseSpec],
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
    raised: &mut Raised,
) -> Result<(), Rejection> {
    let b = Bindings {
        ctx,
        evt: if see_evt { Some(evt) } else { None },
        active: None,
    };
    for raise in raises {
        let mut payload = BTreeMap::new();
        for (k, src) in &raise.with {
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
            payload.insert(k.clone(), v);
        }
        raised.push((raise.event.clone(), payload));
    }
    Ok(())
}

pub(super) fn apply_block(
    block: &fsm_core::spec::Block,
    ctx: &mut BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
    raised: &mut Raised,
) -> Result<(), Rejection> {
    let snapshot = ctx.clone();
    apply_sets(&block.sets, ctx, evt, see_evt, budget)?;
    apply_emits(&block.emits, &snapshot, evt, see_evt, budget, effects)?;
    apply_raises(&block.raises, &snapshot, evt, see_evt, budget, raised)?;
    Ok(())
}

pub(super) fn naive_validate(
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

pub(super) fn parse_typed(raw: &Value, ty: &fsm_core::spec::TySpec) -> Result<Val, &'static str> {
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

pub(super) fn reject(code: &'static str, what: &str) -> Rejection {
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

pub(super) fn eval_invariants(
    spec: &MachineSpec,
    ctx: &BTreeMap<String, Val>,
    active: &std::collections::BTreeSet<String>,
    budget: &mut Budget,
) -> Result<Vec<String>, Rejection> {
    let mut flags = Vec::new();
    for inv in &spec.invariants {
        match eval_bool(
            Some(inv.expr.as_str()),
            ctx,
            &BTreeMap::new(),
            Some(active),
            budget,
        ) {
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

pub(super) fn is_compound(n: &StateNode) -> bool {
    !n.states.is_empty() && n.history.is_none()
}

pub(super) fn apply_entry_chain(
    states: &[StateNode],
    start: &str,
    ctx: &mut BTreeMap<String, Val>,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
    raised: &mut Raised,
) -> Result<Vec<String>, Rejection> {
    let mut entered = vec![start.to_string()];
    if let Some(node) = find(states, start) {
        if let Some(b) = &node.entry {
            apply_block(b, ctx, &BTreeMap::new(), false, budget, effects, raised)?;
        }
    }
    entered.extend(initial_descent(states, start));
    for name in entered.iter().skip(1) {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.entry {
                apply_block(b, ctx, &BTreeMap::new(), false, budget, effects, raised)?;
            }
        }
    }
    Ok(entered)
}
