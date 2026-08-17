//! Strict, budgeted evaluation with optional traces.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use super::ExprError;
use super::ast::{Arg, BinOp, CmpOp, Expr};
use super::lexer::Span;

use crate::decimal::{Dec, RoundMode};
use crate::json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val {
    Bool(bool),
    Int(i64),
    Dec(Dec),
    Str(String),
    Enum { ty: String, variant: String },
    Ts(i64),
    Dur(i64),
}

impl Val {
    pub fn canonical_string(&self) -> String {
        match self {
            Val::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            Val::Int(n) => n.to_string(),
            Val::Dec(d) => d.format(),
            Val::Str(s) => s.clone(),
            Val::Enum { ty, variant } => format!("{ty}.{variant}"),
            Val::Ts(n) | Val::Dur(n) => n.to_string(),
        }
    }
}

pub struct Bindings<'a> {
    pub ctx: &'a BTreeMap<String, Val>,
    pub evt: Option<&'a BTreeMap<String, Val>>,
}

pub struct Budget {
    remaining: u32,
}

impl Budget {
    pub fn new(limit: u32) -> Self {
        Self { remaining: limit }
    }

    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    pub fn tick(&mut self, span: Span) -> Result<(), ExprError> {
        if self.remaining == 0 {
            return Err(ExprError::new(
                "internal/budget",
                span,
                "evaluation budget exhausted",
                "this is an engine invariant breach",
            ));
        }
        self.remaining -= 1;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceOutcome {
    Value(String),
    Skipped,
    Error {
        code: &'static str,
        inputs: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceNode {
    pub span: Span,
    pub outcome: TraceOutcome,
    pub children: Vec<TraceNode>,
}

pub fn trace_to_value(t: &TraceNode) -> Value {
    let mut obj = BTreeMap::new();
    obj.insert(
        "span".into(),
        Value::Arr(vec![
            Value::Num(t.span.start.to_string()),
            Value::Num(t.span.end.to_string()),
        ]),
    );
    match &t.outcome {
        TraceOutcome::Value(v) => {
            obj.insert("outcome".into(), Value::Str("value".into()));
            obj.insert("value".into(), Value::Str(v.clone()));
        }
        TraceOutcome::Skipped => {
            obj.insert("outcome".into(), Value::Str("skipped".into()));
        }
        TraceOutcome::Error { code, inputs } => {
            obj.insert("outcome".into(), Value::Str("error".into()));
            obj.insert("code".into(), Value::Str((*code).into()));
            obj.insert(
                "inputs".into(),
                Value::Arr(inputs.iter().cloned().map(Value::Str).collect()),
            );
        }
    }
    obj.insert(
        "children".into(),
        Value::Arr(t.children.iter().map(trace_to_value).collect()),
    );
    Value::Obj(obj)
}

fn skipped(e: &Expr) -> TraceNode {
    TraceNode {
        span: e.span(),
        outcome: TraceOutcome::Skipped,
        children: Vec::new(),
    }
}

pub fn eval(
    e: &Expr,
    b: &Bindings<'_>,
    budget: &mut Budget,
    trace: bool,
) -> (Result<Val, ExprError>, Option<TraceNode>) {
    match eval_inner(e, b, budget, trace) {
        Ok((v, t)) => (Ok(v), t),
        Err((err, t)) => (Err(err), t),
    }
}

type EvalOk = (Val, Option<TraceNode>);
pub(crate) type EvalErr = (ExprError, Option<TraceNode>);

fn eval_inner(
    e: &Expr,
    b: &Bindings<'_>,
    budget: &mut Budget,
    trace: bool,
) -> Result<EvalOk, EvalErr> {
    budget
        .tick(e.span())
        .map_err(|err| (err, Some(err_node(e.span(), "internal/budget", vec![]))))?;
    match e {
        Expr::IntLit { digits, span } => {
            let n: i64 = digits.parse().unwrap();
            ok(Val::Int(n), *span, trace, vec![])
        }
        Expr::DecLit {
            digits,
            scale,
            span,
        } => {
            let d = Dec::parse(digits, *scale).map_err(|_| {
                (
                    overflow(*span, vec![digits.clone()]),
                    Some(err_node(*span, "run/overflow", vec![digits.clone()])),
                )
            })?;
            ok(Val::Dec(d), *span, trace, vec![])
        }
        Expr::StrLit { value, span } => ok(Val::Str(value.clone()), *span, trace, vec![]),
        Expr::BoolLit { value, span } => ok(Val::Bool(*value), *span, trace, vec![]),
        Expr::CtxRef { name, span } => {
            let v = b.ctx.get(name).cloned().ok_or_else(|| {
                let err = ExprError::new(
                    "expr/unknown_var",
                    *span,
                    format!("unbound ctx.{name}"),
                    "missing binding",
                );
                (err, Some(err_node(*span, "expr/unknown_var", vec![])))
            })?;
            ok(v, *span, trace, vec![])
        }
        Expr::EvtRef { name, span } => {
            let evt = b.evt.ok_or_else(|| {
                let err =
                    ExprError::new("expr/unknown_field", *span, "no event", "no event bindings");
                (err, Some(err_node(*span, "expr/unknown_field", vec![])))
            })?;
            let v = evt.get(name).cloned().ok_or_else(|| {
                let err = ExprError::new(
                    "expr/unknown_field",
                    *span,
                    format!("unbound evt.{name}"),
                    "missing binding",
                );
                (err, Some(err_node(*span, "expr/unknown_field", vec![])))
            })?;
            ok(v, *span, trace, vec![])
        }
        Expr::EnumLit { ty, variant, span } => ok(
            Val::Enum {
                ty: ty.clone(),
                variant: variant.clone(),
            },
            *span,
            trace,
            vec![],
        ),
        Expr::Not { inner, span } => {
            let (v, c) = eval_inner(inner, b, budget, trace)?;
            match v {
                Val::Bool(x) => ok(Val::Bool(!x), *span, trace, kid(c)),
                other => Err(type_err(*span, other)),
            }
        }
        Expr::Neg { inner, span } => {
            let (v, c) = eval_inner(inner, b, budget, trace)?;
            let out = neg_val(v, *span)?;
            ok(out, *span, trace, kid(c))
        }
        Expr::And { lhs, rhs, span } => eval_logic(*span, lhs, rhs, b, budget, trace, true),
        Expr::Or { lhs, rhs, span } => eval_logic(*span, lhs, rhs, b, budget, trace, false),
        Expr::Cmp { op, lhs, rhs, span } => {
            let (lv, lc) = eval_inner(lhs, b, budget, trace)?;
            let (rv, rc) = eval_inner(rhs, b, budget, trace)?;
            let res = cmp_vals(*op, &lv, &rv)
                .map_err(|e| (e, Some(err_node(*span, "expr/type_mismatch", vec![]))))?;
            ok(Val::Bool(res), *span, trace, kids(lc, rc))
        }
        Expr::Bin { op, lhs, rhs, span } => {
            let (lv, lc) = eval_inner(lhs, b, budget, trace)?;
            let (rv, rc) = eval_inner(rhs, b, budget, trace)?;
            let out = bin_vals(*op, lv, rv, *span)?;
            ok(out, *span, trace, kids(lc, rc))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            widen,
            span,
        } => {
            let (cv, cc) = eval_inner(cond, b, budget, trace)?;
            let flag = match cv {
                Val::Bool(x) => x,
                other => return Err(type_err(*span, other)),
            };
            if flag {
                let (v, tc) = eval_inner(then_branch, b, budget, trace)?;
                let v = apply_compiled_dec(v, *widen, *span)?;
                let mut ch = kid(cc);
                ch.extend(kid(tc));
                if trace {
                    ch.push(skipped(else_branch));
                }
                ok(v, *span, trace, ch)
            } else {
                let (v, ec) = eval_inner(else_branch, b, budget, trace)?;
                let v = apply_compiled_dec(v, *widen, *span)?;
                let mut ch = kid(cc);
                if trace {
                    ch.push(skipped(then_branch));
                }
                ch.extend(kid(ec));
                ok(v, *span, trace, ch)
            }
        }
        Expr::Call {
            name, args, span, ..
        } => eval_call(name, args, *span, b, budget, trace),
    }
}

fn eval_logic(
    span: Span,
    lhs: &Expr,
    rhs: &Expr,
    b: &Bindings<'_>,
    budget: &mut Budget,
    trace: bool,
    is_and: bool,
) -> Result<EvalOk, EvalErr> {
    let (lv, lc) = eval_inner(lhs, b, budget, trace)?;
    let lb = match lv {
        Val::Bool(x) => x,
        other => return Err(type_err(span, other)),
    };
    let short = if is_and { !lb } else { lb };
    if short {
        let mut ch = kid(lc);
        if trace {
            ch.push(skipped(rhs));
        }
        return ok(Val::Bool(lb), span, trace, ch);
    }
    let (rv, rc) = eval_inner(rhs, b, budget, trace)?;
    let rb = match rv {
        Val::Bool(x) => x,
        other => return Err(type_err(span, other)),
    };
    ok(Val::Bool(rb), span, trace, kids(lc, rc))
}

pub(crate) fn apply_compiled_dec(v: Val, widen: Option<u8>, span: Span) -> Result<Val, EvalErr> {
    let Val::Dec(d) = v else {
        return Ok(v);
    };
    let Some(target) = widen else {
        return Err((
            ExprError::new(
                "internal/untyped_if",
                span,
                "decimal if was evaluated without a compile-time result type",
                "typecheck the expression before eval",
            ),
            Some(err_node(span, "internal/untyped_if", vec![])),
        ));
    };
    if target == d.scale {
        return Ok(Val::Dec(d));
    }
    match d.rescale_up(target) {
        Ok(w) => Ok(Val::Dec(w)),
        Err(_) => Err((
            ExprError::new(
                "run/overflow",
                span,
                "decimal rescale overflow",
                "use a smaller magnitude",
            ),
            Some(err_node(span, "run/overflow", vec![])),
        )),
    }
}

fn eval_call(
    name: &str,
    args: &[Arg],
    span: Span,
    b: &Bindings<'_>,
    budget: &mut Budget,
    trace: bool,
) -> Result<EvalOk, EvalErr> {
    let mut children = Vec::new();
    let mut vals = Vec::new();
    for a in args {
        match a {
            Arg::Expr(e) => {
                let (v, c) = eval_inner(e, b, budget, trace)?;
                if let Some(c) = c {
                    children.push(c);
                }
                vals.push(v);
            }
            Arg::Word { name, .. } => vals.push(Val::Str(name.clone())),
        }
    }
    let out = apply_builtin(name, &vals, span)?;
    ok(out, span, trace, children)
}

pub(crate) fn apply_builtin(name: &str, vals: &[Val], span: Span) -> Result<Val, EvalErr> {
    let need = match name {
        "min" | "max" => 2,
        "abs" => 1,
        "dec" => 2,
        "round" => 3,
        "div" => 4,
        "dur" => 2,
        _ => 0,
    };
    if need > 0 && vals.len() != need {
        return Err((
            ExprError::new(
                "expr/arity",
                span,
                format!("{name} expected {need} arguments, found {}", vals.len()),
                format!("expected {need}"),
            ),
            Some(err_node(span, "expr/arity", vec![])),
        ));
    }
    match name {
        "min" => min_max(true, vals, span),
        "max" => min_max(false, vals, span),
        "abs" => abs_val(&vals[0], span),
        "dec" => dec_val(vals, span),
        "round" => round_val(vals, span),
        "div" => div_val(vals, span),
        "dur" => dur_val(vals, span),
        other => Err((
            ExprError::new(
                "expr/unknown_builtin",
                span,
                format!("unknown {other}"),
                "unknown builtin",
            ),
            Some(err_node(span, "expr/unknown_builtin", vec![])),
        )),
    }
}

pub(crate) fn neg_val(v: Val, span: Span) -> Result<Val, EvalErr> {
    match v {
        Val::Int(n) => Ok(Val::Int(
            n.checked_neg()
                .ok_or_else(|| ovf(span, vec![n.to_string()]))?,
        )),
        Val::Dec(d) => Ok(Val::Dec(Dec {
            mant: d
                .mant
                .checked_neg()
                .ok_or_else(|| ovf(span, vec![d.format()]))?,
            scale: d.scale,
        })),
        Val::Dur(n) => Ok(Val::Dur(
            n.checked_neg()
                .ok_or_else(|| ovf(span, vec![n.to_string()]))?,
        )),
        other => Err(type_err(span, other)),
    }
}

fn min_max(is_min: bool, vals: &[Val], span: Span) -> Result<Val, EvalErr> {
    let (a, b) = (&vals[0], &vals[1]);
    let less = cmp_vals(CmpOp::Lt, a, b).map_err(|e| (e, None))?;
    let pick = if is_min {
        less
    } else {
        !less && !cmp_vals(CmpOp::Eq, a, b).unwrap_or(false)
    };
    let chosen = if pick { a } else { b };
    match (a, b, chosen) {
        (Val::Dec(x), Val::Dec(y), Val::Dec(c)) => {
            let scale = x.scale.max(y.scale);
            let d = c
                .round(scale, RoundMode::Down)
                .map_err(|_| ovf(span, vec![c.format()]))?;
            Ok(Val::Dec(d))
        }
        _ => Ok(chosen.clone()),
    }
}

fn abs_val(v: &Val, span: Span) -> Result<Val, EvalErr> {
    match v {
        Val::Int(n) => Ok(Val::Int(
            n.checked_abs()
                .ok_or_else(|| ovf(span, vec![n.to_string()]))?,
        )),
        Val::Dec(d) => Ok(Val::Dec(Dec {
            mant: d
                .mant
                .checked_abs()
                .ok_or_else(|| ovf(span, vec![d.format()]))?,
            scale: d.scale,
        })),
        Val::Dur(n) => Ok(Val::Dur(
            n.checked_abs()
                .ok_or_else(|| ovf(span, vec![n.to_string()]))?,
        )),
        other => Err(type_err(span, other.clone())),
    }
}

fn dec_val(vals: &[Val], span: Span) -> Result<Val, EvalErr> {
    let s = int_of(&vals[1], span)? as u8;
    match &vals[0] {
        Val::Int(n) => {
            let d = Dec::parse(&n.to_string(), s).map_err(|_| ovf(span, vec![n.to_string()]))?;
            Ok(Val::Dec(d))
        }
        Val::Dec(d) => {
            let out = d
                .round(s, RoundMode::Down)
                .map_err(|_| ovf(span, vec![d.format()]))?;
            // dec only widens; typeck already rejected narrow
            Ok(Val::Dec(out))
        }
        other => Err(type_err(span, other.clone())),
    }
}

fn round_val(vals: &[Val], span: Span) -> Result<Val, EvalErr> {
    let s = int_of(&vals[1], span)? as u8;
    let mode = mode_of(&vals[2], span)?;
    match &vals[0] {
        Val::Dec(d) => {
            let out = d.round(s, mode).map_err(|_| ovf(span, vec![d.format()]))?;
            Ok(Val::Dec(out))
        }
        other => Err(type_err(span, other.clone())),
    }
}

fn div_val(vals: &[Val], span: Span) -> Result<Val, EvalErr> {
    let a = as_dec(&vals[0], span)?;
    let b = as_dec(&vals[1], span)?;
    let s = int_of(&vals[2], span)? as u8;
    let mode = mode_of(&vals[3], span)?;
    match a.div(b, s, mode) {
        Ok(d) => Ok(Val::Dec(d)),
        Err(crate::decimal::DecError::DivZero) => Err((
            ExprError::new(
                "run/div_zero",
                span,
                "division by zero",
                "the divisor is zero",
            )
            .with_details(vec![("lhs".into(), a.format()), ("rhs".into(), b.format())]),
            Some(err_node(span, "run/div_zero", vec![a.format(), b.format()])),
        )),
        Err(_) => Err(ovf(span, vec![a.format(), b.format()])),
    }
}

fn dur_val(vals: &[Val], span: Span) -> Result<Val, EvalErr> {
    let n = int_of(&vals[0], span)?;
    let unit = match &vals[1] {
        Val::Str(s) => s.as_str(),
        _ => return Err(type_err(span, vals[1].clone())),
    };
    let factor: i64 = match unit {
        "ms" => 1,
        "s" => 1000,
        "min" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => {
            return Err((
                ExprError::new(
                    "expr/mode_invalid",
                    span,
                    format!("unknown unit {unit}"),
                    "ms s min h d",
                ),
                None,
            ));
        }
    };
    let ms = n
        .checked_mul(factor)
        .ok_or_else(|| ovf(span, vec![n.to_string(), unit.into()]))?;
    Ok(Val::Dur(ms))
}

fn as_dec(v: &Val, span: Span) -> Result<Dec, EvalErr> {
    match v {
        Val::Dec(d) => Ok(*d),
        Val::Int(n) => Dec::parse(&n.to_string(), 0).map_err(|_| ovf(span, vec![n.to_string()])),
        other => Err(type_err(span, other.clone())),
    }
}

fn int_of(v: &Val, span: Span) -> Result<i64, EvalErr> {
    match v {
        Val::Int(n) => Ok(*n),
        other => Err(type_err(span, other.clone())),
    }
}

fn mode_of(v: &Val, span: Span) -> Result<RoundMode, EvalErr> {
    match v {
        Val::Str(s) => RoundMode::from_name(s).ok_or_else(|| {
            (
                ExprError::new(
                    "expr/mode_invalid",
                    span,
                    format!("unknown mode {s}"),
                    "invalid mode",
                ),
                None,
            )
        }),
        other => Err(type_err(span, other.clone())),
    }
}

pub(crate) fn bin_vals(op: BinOp, lv: Val, rv: Val, span: Span) -> Result<Val, EvalErr> {
    match (op, lv, rv) {
        (BinOp::Add, Val::Int(a), Val::Int(b)) => {
            Ok(Val::Int(a.checked_add(b).ok_or_else(|| {
                ovf(span, vec![a.to_string(), b.to_string()])
            })?))
        }
        (BinOp::Sub, Val::Int(a), Val::Int(b)) => {
            Ok(Val::Int(a.checked_sub(b).ok_or_else(|| {
                ovf(span, vec![a.to_string(), b.to_string()])
            })?))
        }
        (BinOp::Mul, Val::Int(a), Val::Int(b)) => {
            Ok(Val::Int(a.checked_mul(b).ok_or_else(|| {
                ovf(span, vec![a.to_string(), b.to_string()])
            })?))
        }
        (BinOp::Add, Val::Dec(a), Val::Dec(b)) => Ok(Val::Dec(
            a.checked_add(b)
                .map_err(|_| ovf(span, vec![a.format(), b.format()]))?,
        )),
        (BinOp::Sub, Val::Dec(a), Val::Dec(b)) => Ok(Val::Dec(
            a.checked_sub(b)
                .map_err(|_| ovf(span, vec![a.format(), b.format()]))?,
        )),
        (BinOp::Mul, Val::Dec(a), Val::Dec(b)) => Ok(Val::Dec(
            a.checked_mul(b)
                .map_err(|_| ovf(span, vec![a.format(), b.format()]))?,
        )),
        (BinOp::Mul, Val::Dec(a), Val::Int(b)) => {
            let d = Dec::parse(&b.to_string(), 0).map_err(|_| ovf(span, vec![b.to_string()]))?;
            Ok(Val::Dec(
                a.checked_mul(d)
                    .map_err(|_| ovf(span, vec![a.format(), b.to_string()]))?,
            ))
        }
        (BinOp::Mul, Val::Int(a), Val::Dec(b)) => {
            let d = Dec::parse(&a.to_string(), 0).map_err(|_| ovf(span, vec![a.to_string()]))?;
            Ok(Val::Dec(
                d.checked_mul(b)
                    .map_err(|_| ovf(span, vec![a.to_string(), b.format()]))?,
            ))
        }
        (BinOp::Add, Val::Ts(t), Val::Dur(d)) | (BinOp::Add, Val::Dur(d), Val::Ts(t)) => {
            Ok(Val::Ts(t.checked_add(d).ok_or_else(|| {
                ovf(span, vec![t.to_string(), d.to_string()])
            })?))
        }
        (BinOp::Sub, Val::Ts(t), Val::Dur(d)) => {
            Ok(Val::Ts(t.checked_sub(d).ok_or_else(|| {
                ovf(span, vec![t.to_string(), d.to_string()])
            })?))
        }
        (BinOp::Sub, Val::Ts(a), Val::Ts(b)) => {
            Ok(Val::Dur(a.checked_sub(b).ok_or_else(|| {
                ovf(span, vec![a.to_string(), b.to_string()])
            })?))
        }
        (BinOp::Add, Val::Dur(a), Val::Dur(b)) => {
            Ok(Val::Dur(a.checked_add(b).ok_or_else(|| {
                ovf(span, vec![a.to_string(), b.to_string()])
            })?))
        }
        (BinOp::Sub, Val::Dur(a), Val::Dur(b)) => {
            Ok(Val::Dur(a.checked_sub(b).ok_or_else(|| {
                ovf(span, vec![a.to_string(), b.to_string()])
            })?))
        }
        (BinOp::Mul, Val::Dur(d), Val::Int(n)) | (BinOp::Mul, Val::Int(n), Val::Dur(d)) => {
            Ok(Val::Dur(d.checked_mul(n).ok_or_else(|| {
                ovf(span, vec![d.to_string(), n.to_string()])
            })?))
        }
        (op, a, b) => Err(type_err(span, a).0.into_eval(b, op)),
    }
}

trait IntoEval {
    fn into_eval(self, _b: Val, _op: BinOp) -> EvalErr;
}

impl IntoEval for ExprError {
    fn into_eval(self, _b: Val, _op: BinOp) -> EvalErr {
        let span = self.span;
        (self, Some(err_node(span, "expr/type_mismatch", vec![])))
    }
}

pub(crate) fn cmp_vals(op: CmpOp, lv: &Val, rv: &Val) -> Result<bool, ExprError> {
    let dummy = Span::new(0, 0);
    let ord = match (lv, rv) {
        (Val::Int(a), Val::Int(b)) => a.cmp(b),
        (Val::Dec(a), Val::Dec(b)) => a.cmp(*b),
        (Val::Ts(a), Val::Ts(b)) | (Val::Dur(a), Val::Dur(b)) => a.cmp(b),
        (Val::Str(a), Val::Str(b)) => a.cmp(b),
        (Val::Bool(a), Val::Bool(b)) => a.cmp(b),
        (
            Val::Enum {
                ty: t1,
                variant: v1,
            },
            Val::Enum {
                ty: t2,
                variant: v2,
            },
        ) if t1 == t2 => v1.cmp(v2),
        _ => {
            return Err(ExprError::new(
                "expr/type_mismatch",
                dummy,
                "cannot compare these values",
                "values must be the same class",
            ));
        }
    };
    Ok(match op {
        CmpOp::Eq => ord.is_eq(),
        CmpOp::Ne => ord.is_ne(),
        CmpOp::Lt => ord.is_lt(),
        CmpOp::Le => ord.is_le(),
        CmpOp::Gt => ord.is_gt(),
        CmpOp::Ge => ord.is_ge(),
    })
}

fn ok(v: Val, span: Span, trace: bool, children: Vec<TraceNode>) -> Result<EvalOk, EvalErr> {
    let node = if trace {
        Some(TraceNode {
            span,
            outcome: TraceOutcome::Value(v.canonical_string()),
            children,
        })
    } else {
        None
    };
    Ok((v, node))
}

fn kid(c: Option<TraceNode>) -> Vec<TraceNode> {
    c.into_iter().collect()
}

fn kids(a: Option<TraceNode>, b: Option<TraceNode>) -> Vec<TraceNode> {
    let mut v = Vec::new();
    if let Some(a) = a {
        v.push(a);
    }
    if let Some(b) = b {
        v.push(b);
    }
    v
}

fn err_node(span: Span, code: &'static str, inputs: Vec<String>) -> TraceNode {
    TraceNode {
        span,
        outcome: TraceOutcome::Error { code, inputs },
        children: Vec::new(),
    }
}

fn overflow(span: Span, inputs: Vec<String>) -> ExprError {
    ExprError::new(
        "run/overflow",
        span,
        "arithmetic overflow",
        "the result is not representable",
    )
    .with_details(
        inputs
            .into_iter()
            .enumerate()
            .map(|(i, o)| (format!("op{i}"), o))
            .collect(),
    )
}

fn ovf(span: Span, inputs: Vec<String>) -> EvalErr {
    let err = overflow(span, inputs.clone());
    (err, Some(err_node(span, "run/overflow", inputs)))
}

fn type_err(span: Span, v: Val) -> EvalErr {
    (
        ExprError::new(
            "expr/type_mismatch",
            span,
            format!("unexpected value {}", v.canonical_string()),
            "typecheck should have rejected this",
        ),
        Some(err_node(
            span,
            "expr/type_mismatch",
            vec![v.canonical_string()],
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parser::parse;

    fn empty_ctx() -> BTreeMap<String, Val> {
        BTreeMap::new()
    }

    #[test]
    fn budget_counts_nodes() {
        let e = parse("1 + 2").unwrap();
        // 3 nodes
        let ctx = empty_ctx();
        let b = Bindings {
            ctx: &ctx,
            evt: None,
        };
        let mut bud = Budget::new(3);
        assert!(eval(&e, &b, &mut bud, false).0.is_ok());
        let mut bud = Budget::new(2);
        let err = eval(&e, &b, &mut bud, false).0.unwrap_err();
        assert_eq!(err.code, "internal/budget");
    }

    #[test]
    fn budget_is_shared() {
        let e = parse("1").unwrap();
        let ctx = empty_ctx();
        let b = Bindings {
            ctx: &ctx,
            evt: None,
        };
        let mut bud = Budget::new(2);
        assert!(eval(&e, &b, &mut bud, false).0.is_ok());
        assert!(eval(&e, &b, &mut bud, false).0.is_ok());
        assert_eq!(
            eval(&e, &b, &mut bud, false).0.unwrap_err().code,
            "internal/budget"
        );
    }

    #[test]
    fn canonical_string_every_variant() {
        assert_eq!(Val::Bool(true).canonical_string(), "true");
        assert_eq!(Val::Bool(false).canonical_string(), "false");
        assert_eq!(Val::Int(-3).canonical_string(), "-3");
        assert_eq!(
            Val::Dec(Dec::parse("1.50", 2).unwrap()).canonical_string(),
            "1.50"
        );
        assert_eq!(Val::Str("hi".into()).canonical_string(), "hi");
        assert_eq!(
            Val::Enum {
                ty: "Risk".into(),
                variant: "low".into()
            }
            .canonical_string(),
            "Risk.low"
        );
        assert_eq!(Val::Ts(42).canonical_string(), "42");
        assert_eq!(Val::Dur(7).canonical_string(), "7");
    }
}
