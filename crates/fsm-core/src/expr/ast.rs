//! Spanned expression AST and span-free S-expression rendering.

use super::lexer::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurUnit {
    Ms,
    S,
    Min,
    H,
    D,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    IntLit {
        digits: String,
        span: Span,
    },
    DecLit {
        digits: String,
        scale: u8,
        span: Span,
    },
    StrLit {
        value: String,
        span: Span,
    },
    BoolLit {
        value: bool,
        span: Span,
    },
    CtxRef {
        name: String,
        span: Span,
    },
    EvtRef {
        name: String,
        span: Span,
    },
    EnumLit {
        ty: String,
        variant: String,
        span: Span,
    },
    Not {
        inner: Box<Expr>,
        span: Span,
    },
    Neg {
        inner: Box<Expr>,
        span: Span,
    },
    And {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Or {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Cmp {
        op: CmpOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Bin {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        /// Decimal scale bound at compile time; `None` until annotated.
        widen: Option<u8>,
        span: Span,
    },
    Call {
        name: String,
        name_span: Span,
        args: Vec<Arg>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Expr(Expr),
    Word { name: String, span: Span },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::IntLit { span, .. }
            | Expr::DecLit { span, .. }
            | Expr::StrLit { span, .. }
            | Expr::BoolLit { span, .. }
            | Expr::CtxRef { span, .. }
            | Expr::EvtRef { span, .. }
            | Expr::EnumLit { span, .. }
            | Expr::Not { span, .. }
            | Expr::Neg { span, .. }
            | Expr::And { span, .. }
            | Expr::Or { span, .. }
            | Expr::Cmp { span, .. }
            | Expr::Bin { span, .. }
            | Expr::If { span, .. }
            | Expr::Call { span, .. } => *span,
        }
    }
}

/// Stable, span-free S-expression rendering used by parse goldens.
pub fn render_ast(e: &Expr) -> String {
    match e {
        Expr::IntLit { digits, .. } => digits.clone(),
        Expr::DecLit { digits, .. } => digits.clone(),
        Expr::StrLit { value, .. } => format!("\"{value}\""),
        Expr::BoolLit { value, .. } => {
            if *value {
                "true".into()
            } else {
                "false".into()
            }
        }
        Expr::CtxRef { name, .. } => format!("(ctx {name})"),
        Expr::EvtRef { name, .. } => format!("(evt {name})"),
        Expr::EnumLit { ty, variant, .. } => format!("(enum {ty} {variant})"),
        Expr::Not { inner, .. } => format!("(not {})", render_ast(inner)),
        Expr::Neg { inner, .. } => format!("(neg {})", render_ast(inner)),
        Expr::And { lhs, rhs, .. } => format!("(and {} {})", render_ast(lhs), render_ast(rhs)),
        Expr::Or { lhs, rhs, .. } => format!("(or {} {})", render_ast(lhs), render_ast(rhs)),
        Expr::Cmp { op, lhs, rhs, .. } => {
            format!(
                "(cmp {} {} {})",
                op.as_str(),
                render_ast(lhs),
                render_ast(rhs)
            )
        }
        Expr::Bin { op, lhs, rhs, .. } => {
            format!("({} {} {})", op.as_str(), render_ast(lhs), render_ast(rhs))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            format!(
                "(if {} {} {})",
                render_ast(cond),
                render_ast(then_branch),
                render_ast(else_branch)
            )
        }
        Expr::Call { name, args, .. } => {
            let mut s = format!("(call {name}");
            for a in args {
                s.push(' ');
                s.push_str(&render_arg(a));
            }
            s.push(')');
            s
        }
    }
}

fn render_arg(a: &Arg) -> String {
    match a {
        Arg::Expr(e) => render_ast(e),
        Arg::Word { name, .. } => format!("(word {name})"),
    }
}

pub fn node_count(e: &Expr) -> u32 {
    1 + match e {
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => node_count(inner),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => node_count(lhs) + node_count(rhs),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => node_count(cond) + node_count(then_branch) + node_count(else_branch),
        Expr::Call { args, .. } => args
            .iter()
            .map(|a| match a {
                Arg::Expr(e) => node_count(e),
                Arg::Word { .. } => 0,
            })
            .sum(),
        _ => 0,
    }
}

pub fn depth(e: &Expr) -> u32 {
    match e {
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => 1 + depth(inner),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => 1 + depth(lhs).max(depth(rhs)),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => 1 + depth(cond).max(depth(then_branch)).max(depth(else_branch)),
        Expr::Call { args, .. } => {
            1 + args
                .iter()
                .map(|a| match a {
                    Arg::Expr(e) => depth(e),
                    Arg::Word { .. } => 0,
                })
                .max()
                .unwrap_or(0)
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parser::parse;

    #[test]
    fn node_count_and_depth_hand_counted() {
        let e = parse("1 + 2").unwrap();
        assert_eq!(node_count(&e), 3);
        assert_eq!(depth(&e), 2);
        let e = parse("not not true").unwrap();
        assert_eq!(node_count(&e), 3);
        assert_eq!(depth(&e), 3);
    }

    #[test]
    fn render_ast_span_free() {
        let a = render_ast(&parse("ctx.a+ctx.b").unwrap());
        let b = render_ast(&parse("ctx.a + ctx.b").unwrap());
        assert_eq!(a, b);
        assert_eq!(a, "(add (ctx a) (ctx b))");
    }
}
