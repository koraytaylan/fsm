//! Static typing for `expr/1`.

use std::collections::BTreeMap;
use std::fmt;

use super::ExprError;
use super::ast::{Arg, BinOp, CmpOp, Expr};
use super::lexer::Span;
use crate::ident::suggest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Bool,
    Int,
    Dec(u8),
    Str,
    Enum(String),
    Ts,
    Dur,
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Bool => write!(f, "bool"),
            Ty::Int => write!(f, "int"),
            Ty::Dec(s) => write!(f, "decimal({s})"),
            Ty::Str => write!(f, "str"),
            Ty::Enum(n) => write!(f, "enum {n}"),
            Ty::Ts => write!(f, "timestamp"),
            Ty::Dur => write!(f, "duration"),
        }
    }
}

impl Ty {
    pub fn class_eq(&self, other: &Ty) -> bool {
        match (self, other) {
            (Ty::Dec(_), Ty::Dec(_)) => true,
            (Ty::Enum(a), Ty::Enum(b)) => a == b,
            _ => self == other,
        }
    }

    pub fn is_dec(&self) -> bool {
        matches!(self, Ty::Dec(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Guard,
    TransitionAction,
    Invariant,
    Block,
}

pub struct Scope<'a> {
    pub kind: ScopeKind,
    pub ctx: &'a BTreeMap<String, Ty>,
    pub evt: Option<&'a BTreeMap<String, Ty>>,
    pub enums: &'a BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeWarning {
    pub code: &'static str,
    pub span: Span,
    pub message: String,
}

pub fn typecheck(e: &Expr, scope: &Scope<'_>) -> Result<(Ty, Vec<TypeWarning>), ExprError> {
    let mut warns = Vec::new();
    let ty = check(e, scope, &mut warns)?;
    Ok((ty, warns))
}

/// Write each `if` node's compile-time decimal result scale onto the AST.
pub fn annotate_if_widening(e: &mut Expr, scope: &Scope<'_>) {
    match e {
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => annotate_if_widening(inner, scope),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => {
            annotate_if_widening(lhs, scope);
            annotate_if_widening(rhs, scope);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            annotate_if_widening(cond, scope);
            annotate_if_widening(then_branch, scope);
            annotate_if_widening(else_branch, scope);
        }
        Expr::Call { args, .. } => {
            for a in args {
                if let Arg::Expr(inner) = a {
                    annotate_if_widening(inner, scope);
                }
            }
        }
        _ => {}
    }
    if matches!(e, Expr::If { .. })
        && let Ok((Ty::Dec(s), _)) = typecheck(e, scope)
        && let Expr::If { widen, .. } = e
    {
        *widen = Some(s);
    }
}

fn check(e: &Expr, scope: &Scope<'_>, warns: &mut Vec<TypeWarning>) -> Result<Ty, ExprError> {
    match e {
        Expr::IntLit { .. } => Ok(Ty::Int),
        Expr::DecLit { scale, .. } => Ok(Ty::Dec(*scale)),
        Expr::StrLit { .. } => Ok(Ty::Str),
        Expr::BoolLit { .. } => Ok(Ty::Bool),
        Expr::CtxRef { name, span } => match scope.ctx.get(name) {
            Some(ty) => Ok(ty.clone()),
            None => {
                let names: Vec<&str> = scope.ctx.keys().map(String::as_str).collect();
                let hint = unknown_hint("variable", name, &names);
                Err(ExprError::new(
                    "expr/unknown_var",
                    *span,
                    format!("unknown ctx.{name}"),
                    hint,
                ))
            }
        },
        Expr::EvtRef { name, span } => {
            match scope.kind {
                ScopeKind::Invariant => {
                    return Err(ExprError::new(
                        "expr/evt_in_invariant",
                        *span,
                        "event fields are not in scope in an invariant",
                        "invariants may only read ctx",
                    ));
                }
                ScopeKind::Block => {
                    return Err(ExprError::new(
                        "expr/evt_in_block",
                        *span,
                        "event fields are not in scope in an entry/exit block",
                        "entry/exit blocks may only read and write ctx",
                    ));
                }
                _ => {}
            }
            let fields = scope.evt.ok_or_else(|| {
                ExprError::new(
                    "expr/evt_in_invariant",
                    *span,
                    "event fields are not in scope",
                    "this scope has no event",
                )
            })?;
            match fields.get(name) {
                Some(ty) => Ok(ty.clone()),
                None => {
                    let names: Vec<&str> = fields.keys().map(String::as_str).collect();
                    let hint = unknown_hint("field", name, &names);
                    Err(ExprError::new(
                        "expr/unknown_field",
                        *span,
                        format!("unknown evt.{name}"),
                        hint,
                    ))
                }
            }
        }
        Expr::EnumLit { ty, variant, span } => {
            let variants = scope.enums.get(ty).ok_or_else(|| {
                let names: Vec<&str> = scope.enums.keys().map(String::as_str).collect();
                ExprError::new(
                    "expr/unknown_enum",
                    *span,
                    format!("unknown enum {ty}"),
                    unknown_hint("enum", ty, &names),
                )
            })?;
            if !variants.iter().any(|v| v == variant) {
                let names: Vec<&str> = variants.iter().map(String::as_str).collect();
                return Err(ExprError::new(
                    "expr/unknown_variant",
                    *span,
                    format!("unknown variant {ty}.{variant}"),
                    unknown_hint("variant", variant, &names),
                ));
            }
            Ok(Ty::Enum(ty.clone()))
        }
        Expr::Not { inner, span } => {
            let t = check(inner, scope, warns)?;
            expect_bool(&t, *span)
        }
        Expr::Neg { inner, span } => {
            let t = check(inner, scope, warns)?;
            match t {
                Ty::Int | Ty::Dec(_) | Ty::Dur => Ok(t),
                other => Err(mismatch(*span, &other, "int, decimal, or duration")),
            }
        }
        Expr::And { lhs, rhs, span } | Expr::Or { lhs, rhs, span } => {
            let lt = check(lhs, scope, warns)?;
            let rt = check(rhs, scope, warns)?;
            expect_bool(&lt, lhs.span())?;
            expect_bool(&rt, rhs.span())?;
            let _ = span;
            Ok(Ty::Bool)
        }
        Expr::Cmp { op, lhs, rhs, span } => {
            let lt = check(lhs, scope, warns)?;
            let rt = check(rhs, scope, warns)?;
            check_cmp(*op, &lt, &rt, *span)
        }
        Expr::Bin { op, lhs, rhs, span } => {
            let lt = check(lhs, scope, warns)?;
            let rt = check(rhs, scope, warns)?;
            check_bin(*op, &lt, &rt, *span)
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            span,
            ..
        } => {
            let ct = check(cond, scope, warns)?;
            expect_bool(&ct, cond.span())?;
            let tt = check(then_branch, scope, warns)?;
            let et = check(else_branch, scope, warns)?;
            unify_branches(&tt, &et, *span)
        }
        Expr::Call {
            name,
            name_span,
            args,
            span,
        } => check_call(name, *name_span, args, *span, scope, warns),
    }
}

fn expect_bool(t: &Ty, span: Span) -> Result<Ty, ExprError> {
    if matches!(t, Ty::Bool) {
        Ok(Ty::Bool)
    } else {
        Err(mismatch(span, t, "bool"))
    }
}

fn mismatch(span: Span, got: &Ty, want: &str) -> ExprError {
    ExprError::new(
        "expr/type_mismatch",
        span,
        format!("type mismatch: have {got}, want {want}"),
        format!("this position expects {want}"),
    )
}

fn mixed(span: Span) -> ExprError {
    ExprError::new(
        "expr/mixed_class",
        span,
        "cannot mix decimal and int",
        "write 0.00-style literals (e.g. 1.00) or dec(1, 2)",
    )
}

fn check_bin(op: BinOp, lt: &Ty, rt: &Ty, span: Span) -> Result<Ty, ExprError> {
    match op {
        BinOp::Add | BinOp::Sub => match (lt, rt, op) {
            (Ty::Int, Ty::Int, _) => Ok(Ty::Int),
            (Ty::Dec(a), Ty::Dec(b), _) => Ok(Ty::Dec((*a).max(*b))),
            (Ty::Int, Ty::Dec(_), _) | (Ty::Dec(_), Ty::Int, _) => Err(mixed(span)),
            (Ty::Ts, Ty::Dur, _) => Ok(Ty::Ts),
            (Ty::Dur, Ty::Ts, BinOp::Add) => Ok(Ty::Ts),
            (Ty::Ts, Ty::Ts, BinOp::Sub) => Ok(Ty::Dur),
            (Ty::Dur, Ty::Dur, _) => Ok(Ty::Dur),
            _ => Err(mismatch(span, lt, "matching numeric class")),
        },
        BinOp::Mul => match (lt, rt) {
            (Ty::Int, Ty::Int) => Ok(Ty::Int),
            (Ty::Dec(a), Ty::Dec(b)) => {
                let s = u16::from(*a) + u16::from(*b);
                if s > 12 {
                    Err(ExprError::new(
                        "expr/scale_cap",
                        span,
                        "decimal multiply exceeds scale 12",
                        "round a value first",
                    ))
                } else {
                    Ok(Ty::Dec(s as u8))
                }
            }
            (Ty::Dec(s), Ty::Int) | (Ty::Int, Ty::Dec(s)) => Ok(Ty::Dec(*s)),
            (Ty::Dur, Ty::Int) | (Ty::Int, Ty::Dur) => Ok(Ty::Dur),
            _ => Err(mismatch(span, lt, "int, decimal, or duration")),
        },
    }
}

fn check_cmp(op: CmpOp, lt: &Ty, rt: &Ty, span: Span) -> Result<Ty, ExprError> {
    let ordered = matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge);
    match (lt, rt) {
        (Ty::Dec(_), Ty::Int) | (Ty::Int, Ty::Dec(_)) => Err(mixed(span)),
        (Ty::Dec(_), Ty::Dec(_)) | (Ty::Int, Ty::Int) | (Ty::Ts, Ty::Ts) | (Ty::Dur, Ty::Dur) => {
            Ok(Ty::Bool)
        }
        (Ty::Str, Ty::Str) | (Ty::Bool, Ty::Bool) => {
            if ordered {
                Err(ExprError::new(
                    "expr/cmp_unordered",
                    span,
                    "this type only supports == and !=",
                    "use == or !=",
                ))
            } else {
                Ok(Ty::Bool)
            }
        }
        (Ty::Enum(a), Ty::Enum(b)) if a == b => {
            if ordered {
                Err(ExprError::new(
                    "expr/cmp_unordered",
                    span,
                    "enums only support == and !=",
                    "use == or !=",
                ))
            } else {
                Ok(Ty::Bool)
            }
        }
        (Ty::Enum(_), Ty::Enum(_)) => Err(mismatch(span, lt, "the same enum type")),
        _ => Err(mismatch(span, lt, "values of the same class")),
    }
}

fn unify_branches(a: &Ty, b: &Ty, span: Span) -> Result<Ty, ExprError> {
    match (a, b) {
        (Ty::Dec(s1), Ty::Dec(s2)) => Ok(Ty::Dec((*s1).max(*s2))),
        _ if a == b => Ok(a.clone()),
        _ => Err(mismatch(
            span,
            a,
            &format!("branches of the same class (else is {b})"),
        )),
    }
}

const BUILTINS: &[&str] = &["min", "max", "abs", "dec", "round", "div", "dur"];

fn builtin_list() -> String {
    BUILTINS.join(" ")
}

fn check_call(
    name: &str,
    name_span: Span,
    args: &[Arg],
    span: Span,
    scope: &Scope<'_>,
    warns: &mut Vec<TypeWarning>,
) -> Result<Ty, ExprError> {
    if !BUILTINS.contains(&name) {
        return Err(ExprError::new(
            "expr/unknown_builtin",
            name_span,
            format!("unknown builtin {name}"),
            format!("legal builtins: {}", builtin_list()),
        ));
    }
    match name {
        "min" | "max" => {
            let [a, b] = two_expr(name, args, span)?;
            let ta = check(a, scope, warns)?;
            let tb = check(b, scope, warns)?;
            match (&ta, &tb) {
                (Ty::Int, Ty::Int) => Ok(Ty::Int),
                (Ty::Dec(s1), Ty::Dec(s2)) => Ok(Ty::Dec((*s1).max(*s2))),
                (Ty::Ts, Ty::Ts) => Ok(Ty::Ts),
                (Ty::Dur, Ty::Dur) => Ok(Ty::Dur),
                _ => Err(mismatch(span, &ta, "matching min/max class")),
            }
        }
        "abs" => {
            let [x] = one_expr(name, args, span)?;
            let t = check(x, scope, warns)?;
            match t {
                Ty::Int | Ty::Dec(_) | Ty::Dur => Ok(t),
                other => Err(mismatch(span, &other, "int, decimal, or duration")),
            }
        }
        "dec" => {
            arity(name, args, 2, span)?;
            let x = expr_arg(&args[0], span)?;
            let s = scale_lit(&args[1], span)?;
            let t = check(x, scope, warns)?;
            match t {
                Ty::Int => Ok(Ty::Dec(s)),
                Ty::Dec(s0) if s0 <= s => Ok(Ty::Dec(s)),
                Ty::Dec(_) => Err(ExprError::new(
                    "expr/scale_narrow",
                    span,
                    "dec cannot narrow scale",
                    "use round to narrow a decimal",
                )),
                other => Err(mismatch(span, &other, "int or decimal")),
            }
        }
        "round" => {
            arity(name, args, 3, span)?;
            let x = expr_arg(&args[0], span)?;
            let s = scale_lit(&args[1], span)?;
            mode_word(&args[2], span)?;
            let t = check(x, scope, warns)?;
            match t {
                Ty::Dec(s0) => {
                    if s >= s0 {
                        warns.push(TypeWarning {
                            code: "expr/round_widens",
                            span,
                            message: "round target scale is >= source scale; use dec".into(),
                        });
                    }
                    Ok(Ty::Dec(s))
                }
                other => Err(mismatch(span, &other, "decimal")),
            }
        }
        "div" => {
            arity(name, args, 4, span)?;
            let a = expr_arg(&args[0], span)?;
            let b = expr_arg(&args[1], span)?;
            let s = scale_lit(&args[2], span)?;
            mode_word(&args[3], span)?;
            let ta = check(a, scope, warns)?;
            let tb = check(b, scope, warns)?;
            if !matches!(ta, Ty::Int | Ty::Dec(_)) || !matches!(tb, Ty::Int | Ty::Dec(_)) {
                return Err(mismatch(span, &ta, "int or decimal"));
            }
            Ok(Ty::Dec(s))
        }
        "dur" => {
            arity(name, args, 2, span)?;
            let n = expr_arg(&args[0], span)?;
            unit_word(&args[1], span)?;
            let t = check(n, scope, warns)?;
            if !matches!(t, Ty::Int) {
                return Err(mismatch(span, &t, "int"));
            }
            Ok(Ty::Dur)
        }
        _ => unreachable!(),
    }
}

fn arity(name: &str, args: &[Arg], n: usize, span: Span) -> Result<(), ExprError> {
    if args.len() == n {
        Ok(())
    } else {
        Err(ExprError::new(
            "expr/arity",
            span,
            format!("{name} expected {n} arguments, found {}", args.len()),
            format!("expected {n} / found {}", args.len()),
        ))
    }
}

fn two_expr<'a>(name: &str, args: &'a [Arg], span: Span) -> Result<[&'a Expr; 2], ExprError> {
    arity(name, args, 2, span)?;
    Ok([expr_arg(&args[0], span)?, expr_arg(&args[1], span)?])
}

fn one_expr<'a>(name: &str, args: &'a [Arg], span: Span) -> Result<[&'a Expr; 1], ExprError> {
    arity(name, args, 1, span)?;
    Ok([expr_arg(&args[0], span)?])
}

fn expr_arg(arg: &Arg, span: Span) -> Result<&Expr, ExprError> {
    match arg {
        Arg::Expr(e) => Ok(e),
        Arg::Word { .. } => Err(ExprError::new(
            "expr/type_mismatch",
            span,
            "expected an expression argument",
            "pass a value, not a mode/unit word",
        )),
    }
}

fn scale_lit(arg: &Arg, span: Span) -> Result<u8, ExprError> {
    match arg {
        Arg::Expr(Expr::IntLit { digits, .. }) => {
            let n: i64 = digits.parse().unwrap_or(-1);
            if (0..=12).contains(&n) {
                Ok(n as u8)
            } else {
                Err(ExprError::new(
                    "expr/scale_not_literal",
                    span,
                    "scale must be an integer literal 0..=12",
                    "the expression's type cannot depend on a runtime value",
                ))
            }
        }
        _ => Err(ExprError::new(
            "expr/scale_not_literal",
            span,
            "scale must be an integer literal 0..=12",
            "the expression's type cannot depend on a runtime value",
        )),
    }
}

fn mode_word(arg: &Arg, span: Span) -> Result<(), ExprError> {
    const MODES: &[&str] = &[
        "down",
        "up",
        "floor",
        "ceiling",
        "half_up",
        "half_down",
        "half_even",
    ];
    match arg {
        Arg::Word { name, .. } if MODES.contains(&name.as_str()) => Ok(()),
        Arg::Word { name, .. } => Err(ExprError::new(
            "expr/mode_invalid",
            span,
            format!("unknown mode {name}"),
            format!("legal modes: {}", MODES.join(" ")),
        )),
        _ => Err(ExprError::new(
            "expr/mode_invalid",
            span,
            "mode must be a literal word",
            format!("legal modes: {}", MODES.join(" ")),
        )),
    }
}

fn unit_word(arg: &Arg, span: Span) -> Result<(), ExprError> {
    const UNITS: &[&str] = &["ms", "s", "min", "h", "d"];
    match arg {
        Arg::Word { name, .. } if UNITS.contains(&name.as_str()) => Ok(()),
        Arg::Word { name, .. } => Err(ExprError::new(
            "expr/mode_invalid",
            span,
            format!("unknown unit {name}"),
            format!("legal units: {}", UNITS.join(" ")),
        )),
        _ => Err(ExprError::new(
            "expr/mode_invalid",
            span,
            "unit must be a literal word",
            format!("legal units: {}", UNITS.join(" ")),
        )),
    }
}

fn unknown_hint(kind: &str, name: &str, names: &[&str]) -> String {
    let mut hint = format!("legal {kind}s: {}", names.join(", "));
    if let Some(s) = suggest(name, names.iter().copied()) {
        hint = format!("did you mean `{s}`? {hint}");
    }
    hint
}

/// Parse a fixture type rendering such as `decimal(2)` or `enum Risk`.
pub fn parse_ty(s: &str) -> Option<Ty> {
    if let Some(rest) = s.strip_prefix("decimal(").and_then(|r| r.strip_suffix(')')) {
        return Some(Ty::Dec(rest.parse().ok()?));
    }
    if let Some(rest) = s.strip_prefix("enum ") {
        return Some(Ty::Enum(rest.to_string()));
    }
    Some(match s {
        "bool" => Ty::Bool,
        "int" => Ty::Int,
        "str" => Ty::Str,
        "timestamp" => Ty::Ts,
        "duration" => Ty::Dur,
        _ => return None,
    })
}
