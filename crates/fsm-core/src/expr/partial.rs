//! Kleene three-valued evaluation with event fields unknown.

use std::collections::BTreeMap;

use super::ast::Expr;
use super::eval::{Bindings, Budget, Val, eval};
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

fn charge_tree(e: &Expr, budget: &mut Budget) {
    let _ = budget.tick(e.span());
    match e {
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => charge_tree(inner, budget),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => {
            charge_tree(lhs, budget);
            charge_tree(rhs, budget);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            charge_tree(cond, budget);
            charge_tree(then_branch, budget);
            charge_tree(else_branch, budget);
        }
        Expr::Call { args, .. } => {
            for a in args {
                if let super::ast::Arg::Expr(inner) = a {
                    charge_tree(inner, budget);
                }
            }
        }
        _ => {}
    }
}

fn reduce_lazy(e: &Expr, ctx: &BTreeMap<String, Val>, budget: &mut Budget) -> Expr {
    match e {
        Expr::Not { inner, span } => Expr::Not {
            inner: Box::new(reduce_lazy(inner, ctx, budget)),
            span: *span,
        },
        Expr::Neg { inner, span } => Expr::Neg {
            inner: Box::new(reduce_lazy(inner, ctx, budget)),
            span: *span,
        },
        Expr::And { lhs, rhs, span } => Expr::And {
            lhs: Box::new(reduce_lazy(lhs, ctx, budget)),
            rhs: Box::new(reduce_lazy(rhs, ctx, budget)),
            span: *span,
        },
        Expr::Or { lhs, rhs, span } => Expr::Or {
            lhs: Box::new(reduce_lazy(lhs, ctx, budget)),
            rhs: Box::new(reduce_lazy(rhs, ctx, budget)),
            span: *span,
        },
        Expr::Cmp { op, lhs, rhs, span } => Expr::Cmp {
            op: *op,
            lhs: Box::new(reduce_lazy(lhs, ctx, budget)),
            rhs: Box::new(reduce_lazy(rhs, ctx, budget)),
            span: *span,
        },
        Expr::Bin { op, lhs, rhs, span } => Expr::Bin {
            op: *op,
            lhs: Box::new(reduce_lazy(lhs, ctx, budget)),
            rhs: Box::new(reduce_lazy(rhs, ctx, budget)),
            span: *span,
        },
        Expr::If {
            cond,
            then_branch,
            else_branch,
            widen,
            span,
        } => {
            let cond = reduce_lazy(cond, ctx, budget);
            match partial_eval_bool_inner(&cond, ctx, budget) {
                Truth::True => reduce_lazy(then_branch, ctx, budget),
                Truth::False => reduce_lazy(else_branch, ctx, budget),
                Truth::Unknown => Expr::If {
                    cond: Box::new(cond),
                    then_branch: Box::new(reduce_lazy(then_branch, ctx, budget)),
                    else_branch: Box::new(reduce_lazy(else_branch, ctx, budget)),
                    widen: *widen,
                    span: *span,
                },
            }
        }
        Expr::Call {
            name,
            name_span,
            args,
            span,
        } => Expr::Call {
            name: name.clone(),
            name_span: *name_span,
            args: args
                .iter()
                .map(|a| match a {
                    super::ast::Arg::Expr(inner) => {
                        super::ast::Arg::Expr(reduce_lazy(inner, ctx, budget))
                    }
                    other => other.clone(),
                })
                .collect(),
            span: *span,
        },
        other => other.clone(),
    }
}

/// `EvtRef` is Unknown; concrete subtrees go through `eval`. Errors → Unknown.
/// `scope` supplies declared enums and event-field types so annotation is sound.
/// Lazy `if` reduces unreachable branches before `evt` dependence is decided.
pub fn partial_eval_bool(
    e: &Expr,
    ctx: &BTreeMap<String, Val>,
    scope: &super::typeck::Scope<'_>,
    budget: &mut Budget,
) -> Truth {
    let reduced = reduce_lazy(e, ctx, budget);
    let mut annotated = reduced;
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
        _ if has_evt(e) => {
            charge_tree(e, budget);
            Truth::Unknown
        }
        _ => {
            let b = Bindings { ctx, evt: None };
            match eval(e, &b, budget, false).0 {
                Ok(Val::Bool(true)) => Truth::True,
                Ok(Val::Bool(false)) => Truth::False,
                // Conservative-error rule (SPEC.md): a concrete sub-evaluation
                // error — including budget exhaustion — yields Unknown, never
                // a loud failure. The authoritative error happens at send time.
                _ => Truth::Unknown,
            }
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
