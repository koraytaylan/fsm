//! Kleene three-valued evaluation with event fields unknown.

use std::collections::BTreeMap;

use super::ast::Expr;
use super::eval::{
    Bindings, Budget, Val, apply_builtin, apply_compiled_dec, bin_vals, cmp_vals, eval, neg_val,
};
use super::lexer::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    Unknown,
}

fn kleene_and(a: Truth, b: Truth) -> Truth {
    match (a, b) {
        (Truth::False, _) | (_, Truth::False) => Truth::False,
        (Truth::True, Truth::True) => Truth::True,
        _ => Truth::Unknown,
    }
}

fn kleene_or(a: Truth, b: Truth) -> Truth {
    match (a, b) {
        (Truth::True, _) | (_, Truth::True) => Truth::True,
        (Truth::False, Truth::False) => Truth::False,
        _ => Truth::Unknown,
    }
}

fn kleene_not(a: Truth) -> Truth {
    match a {
        Truth::True => Truth::False,
        Truth::False => Truth::True,
        Truth::Unknown => Truth::Unknown,
    }
}

fn has_evt(e: &Expr) -> bool {
    match e {
        Expr::EvtRef { .. } => true,
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => has_evt(inner),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => has_evt(lhs) || has_evt(rhs),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => has_evt(cond) || has_evt(then_branch) || has_evt(else_branch),
        Expr::Call { args, .. } => args.iter().any(|a| match a {
            super::ast::Arg::Expr(e) => has_evt(e),
            super::ast::Arg::Word { .. } => false,
        }),
        _ => false,
    }
}

/// `EvtRef` is Unknown; concrete subtrees go through `eval`. Errors → Unknown.
/// `scope` supplies declared enums and event-field types so annotation is sound.
/// Lazy `and`/`or`/`if` is applied in this walk after annotation, so decimal
/// `if` widening is kept on the selected branch and unvisited operands are not charged.
pub fn partial_eval_bool(
    e: &Expr,
    ctx: &BTreeMap<String, Val>,
    scope: &super::typeck::Scope<'_>,
    budget: &mut Budget,
) -> Truth {
    let mut annotated = e.clone();
    super::typeck::annotate_if_widening(&mut annotated, scope);
    partial_eval_bool_inner(&annotated, ctx, budget)
}

fn partial_eval_bool_inner(e: &Expr, ctx: &BTreeMap<String, Val>, budget: &mut Budget) -> Truth {
    match e {
        Expr::And { lhs, rhs, .. } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            let l = partial_eval_bool_inner(lhs, ctx, budget);
            if l == Truth::False {
                return Truth::False;
            }
            kleene_and(l, partial_eval_bool_inner(rhs, ctx, budget))
        }
        Expr::Or { lhs, rhs, .. } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            let l = partial_eval_bool_inner(lhs, ctx, budget);
            if l == Truth::True {
                return Truth::True;
            }
            kleene_or(l, partial_eval_bool_inner(rhs, ctx, budget))
        }
        Expr::Not { inner, .. } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            kleene_not(partial_eval_bool_inner(inner, ctx, budget))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            match partial_eval_bool_inner(cond, ctx, budget) {
                Truth::True => partial_eval_bool_inner(then_branch, ctx, budget),
                Truth::False => partial_eval_bool_inner(else_branch, ctx, budget),
                Truth::Unknown => Truth::Unknown,
            }
        }
        Expr::EvtRef { .. } => {
            let _ = budget.tick(e.span());
            Truth::Unknown
        }
        Expr::Cmp { op, lhs, rhs, span } => {
            if budget.tick(*span).is_err() {
                return Truth::Unknown;
            }
            match (
                partial_eval_val(lhs, ctx, budget),
                partial_eval_val(rhs, ctx, budget),
            ) {
                (Some(l), Some(r)) => match cmp_vals(*op, &l, &r) {
                    Ok(true) => Truth::True,
                    Ok(false) => Truth::False,
                    Err(_) => Truth::Unknown,
                },
                _ => Truth::Unknown,
            }
        }
        _ => match partial_eval_val(e, ctx, budget) {
            Some(Val::Bool(true)) => Truth::True,
            Some(Val::Bool(false)) => Truth::False,
            _ => Truth::Unknown,
        },
    }
}

fn partial_eval_val(e: &Expr, ctx: &BTreeMap<String, Val>, budget: &mut Budget) -> Option<Val> {
    match e {
        Expr::EvtRef { .. } => {
            let _ = budget.tick(e.span());
            None
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            widen,
            span,
        } => {
            if budget.tick(*span).is_err() {
                return None;
            }
            let flag = match partial_eval_val(cond, ctx, budget)? {
                Val::Bool(b) => b,
                _ => return None,
            };
            let selected = if flag { then_branch } else { else_branch };
            let v = partial_eval_val(selected, ctx, budget)?;
            apply_compiled_dec(v, *widen, *span).ok()
        }
        Expr::And { .. } | Expr::Or { .. } | Expr::Not { .. } => {
            match partial_eval_bool_inner(e, ctx, budget) {
                Truth::True => Some(Val::Bool(true)),
                Truth::False => Some(Val::Bool(false)),
                Truth::Unknown => None,
            }
        }
        Expr::Cmp { op, lhs, rhs, span } => {
            if budget.tick(*span).is_err() {
                return None;
            }
            let l = partial_eval_val(lhs, ctx, budget)?;
            let r = partial_eval_val(rhs, ctx, budget)?;
            cmp_vals(*op, &l, &r).ok().map(Val::Bool)
        }
        Expr::Bin { op, lhs, rhs, span } => {
            if budget.tick(*span).is_err() {
                return None;
            }
            let l = partial_eval_val(lhs, ctx, budget);
            let r = partial_eval_val(rhs, ctx, budget);
            match (l, r) {
                (Some(l), Some(r)) => bin_vals(*op, l, r, *span).ok(),
                _ => None,
            }
        }
        Expr::Neg { inner, span } => {
            if budget.tick(*span).is_err() {
                return None;
            }
            let v = partial_eval_val(inner, ctx, budget)?;
            neg_val(v, *span).ok()
        }
        Expr::Call {
            name, args, span, ..
        } => {
            if budget.tick(*span).is_err() {
                return None;
            }
            let mut vals = Vec::new();
            let mut unknown = false;
            for a in args {
                match a {
                    super::ast::Arg::Expr(inner) => match partial_eval_val(inner, ctx, budget) {
                        Some(v) => vals.push(v),
                        None => unknown = true,
                    },
                    super::ast::Arg::Word { name, .. } => vals.push(Val::Str(name.clone())),
                }
            }
            if unknown {
                return None;
            }
            apply_builtin(name, &vals, *span).ok()
        }
        _ if has_evt(e) => None,
        _ => {
            let b = Bindings { ctx, evt: None };
            eval(e, &b, budget, false).0.ok()
        }
    }
}

#[allow(dead_code)]
fn unused_span() -> Span {
    Span::new(0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::typeck::{Scope, ScopeKind, Ty};

    fn pe(e: &Expr, ctx: &BTreeMap<String, Val>, bud: &mut Budget) -> Truth {
        let ctx_tys: BTreeMap<String, Ty> = BTreeMap::new();
        let enums: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let scope = Scope {
            kind: ScopeKind::Guard,
            ctx: &ctx_tys,
            evt: None,
            enums: &enums,
        };
        partial_eval_bool(e, ctx, &scope, bud)
    }

    #[test]
    fn kleene_tables() {
        let t = [Truth::True, Truth::False, Truth::Unknown];
        for &a in &t {
            for &b in &t {
                let and = kleene_and(a, b);
                let want_and = match (a, b) {
                    (Truth::False, _) | (_, Truth::False) => Truth::False,
                    (Truth::True, Truth::True) => Truth::True,
                    _ => Truth::Unknown,
                };
                assert_eq!(and, want_and, "and {a:?} {b:?}");
                let or = kleene_or(a, b);
                let want_or = match (a, b) {
                    (Truth::True, _) | (_, Truth::True) => Truth::True,
                    (Truth::False, Truth::False) => Truth::False,
                    _ => Truth::Unknown,
                };
                assert_eq!(or, want_or, "or {a:?} {b:?}");
            }
            let n = kleene_not(a);
            let want = match a {
                Truth::True => Truth::False,
                Truth::False => Truth::True,
                Truth::Unknown => Truth::Unknown,
            };
            assert_eq!(n, want, "not {a:?}");
        }
    }

    #[test]
    fn conservative_error_overflow_is_unknown() {
        use crate::expr::parser::parse;
        let e = parse("9223372036854775807 + 1 > 0").unwrap();
        let ctx = BTreeMap::new();
        let mut bud = Budget::new(64);
        assert_eq!(pe(&e, &ctx, &mut bud), Truth::Unknown);
    }

    #[test]
    fn conservative_error_budget_is_unknown() {
        use crate::expr::parser::parse;
        let e = parse("1 + 2 > 0").unwrap();
        let ctx = BTreeMap::new();
        let mut bud = Budget::new(1);
        assert_eq!(pe(&e, &ctx, &mut bud), Truth::Unknown);
    }

    #[test]
    fn budget_sharing_with_eval() {
        use crate::expr::parser::parse;
        let e = parse("true").unwrap();
        let ctx = BTreeMap::new();
        let mut bud = Budget::new(2);
        assert_eq!(pe(&e, &ctx, &mut bud), Truth::True);
        let b = Bindings {
            ctx: &ctx,
            evt: None,
        };
        assert!(eval(&e, &b, &mut bud, false).0.is_ok());
        assert!(eval(&e, &b, &mut bud, false).0.is_err());
    }
}
