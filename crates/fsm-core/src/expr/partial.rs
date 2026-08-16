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

/// `EvtRef` is Unknown; concrete subtrees go through `eval`. Errors → Unknown.
pub fn partial_eval_bool(e: &Expr, ctx: &BTreeMap<String, Val>, budget: &mut Budget) -> Truth {
    match e {
        Expr::And { lhs, rhs, .. } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            let l = partial_eval_bool(lhs, ctx, budget);
            if l == Truth::False {
                return Truth::False;
            }
            kleene_and(l, partial_eval_bool(rhs, ctx, budget))
        }
        Expr::Or { lhs, rhs, .. } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            let l = partial_eval_bool(lhs, ctx, budget);
            if l == Truth::True {
                return Truth::True;
            }
            kleene_or(l, partial_eval_bool(rhs, ctx, budget))
        }
        Expr::Not { inner, .. } => {
            if budget.tick(e.span()).is_err() {
                return Truth::Unknown;
            }
            kleene_not(partial_eval_bool(inner, ctx, budget))
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
            match partial_eval_bool(cond, ctx, budget) {
                Truth::True => partial_eval_bool(then_branch, ctx, budget),
                Truth::False => partial_eval_bool(else_branch, ctx, budget),
                Truth::Unknown => Truth::Unknown,
            }
        }
        Expr::EvtRef { .. } => Truth::Unknown,
        _ if has_evt(e) => Truth::Unknown,
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
        assert_eq!(partial_eval_bool(&e, &ctx, &mut bud), Truth::Unknown);
    }

    #[test]
    fn conservative_error_budget_is_unknown() {
        use crate::expr::parser::parse;
        let e = parse("1 + 2 > 0").unwrap();
        let ctx = BTreeMap::new();
        let mut bud = Budget::new(1);
        assert_eq!(partial_eval_bool(&e, &ctx, &mut bud), Truth::Unknown);
    }

    #[test]
    fn budget_sharing_with_eval() {
        use crate::expr::parser::parse;
        let e = parse("true").unwrap();
        let ctx = BTreeMap::new();
        let mut bud = Budget::new(2);
        assert_eq!(partial_eval_bool(&e, &ctx, &mut bud), Truth::True);
        let b = Bindings {
            ctx: &ctx,
            evt: None,
        };
        assert!(eval(&e, &b, &mut bud, false).0.is_ok());
        assert!(eval(&e, &b, &mut bud, false).0.is_err());
    }
}
