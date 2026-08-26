//! Whether creation can succeed on the declared inits.

use std::collections::{BTreeMap, BTreeSet};

use crate::expr::eval::Val;
use crate::expr::parser;
use crate::machine::CompiledMachine;
use crate::spec::{Finding, Severity};
use crate::tree::Tree;

use super::find_machine_node;

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
