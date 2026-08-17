//! Recursive-descent parser for grammar `expr/1`.

use super::ExprError;
use super::ast::{Arg, BinOp, CmpOp, Expr};
use super::lexer::{Span, Tok, lex};

const MAX_NODES: u32 = 512;
const MAX_DEPTH: u32 = 32;

pub fn parse(src: &str) -> Result<Expr, ExprError> {
    let tokens = lex(src)?;
    let mut p = Parser {
        tokens,
        i: 0,
        src_len: src.len(),
    };
    let e = p.parse_if(0)?;
    if p.peek().is_some() {
        let span = p.peek_span();
        return Err(ExprError::new(
            "expr/parse",
            span,
            "trailing tokens after expression",
            "remove the extra tokens",
        ));
    }
    if super::ast::node_count(&e) > MAX_NODES {
        return Err(ExprError::new(
            "expr/too_long",
            e.span(),
            "expression exceeds 512 AST nodes",
            "split the expression into smaller pieces",
        ));
    }
    if super::ast::depth(&e) > MAX_DEPTH {
        return Err(ExprError::new(
            "expr/too_deep",
            e.span(),
            "expression nesting exceeds depth 32",
            "flatten the expression",
        ));
    }
    Ok(e)
}

struct Parser {
    tokens: Vec<(Tok, Span)>,
    i: usize,
    src_len: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.i).map(|(t, _)| t)
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.i)
            .map(|(_, s)| *s)
            .unwrap_or(Span::point(self.src_len))
    }

    fn bump(&mut self) -> Option<(Tok, Span)> {
        if self.i < self.tokens.len() {
            let t = self.tokens[self.i].clone();
            self.i += 1;
            Some(t)
        } else {
            None
        }
    }

    fn eof_span(&self) -> Span {
        Span::point(self.src_len)
    }

    fn expected(&self, set: &str) -> ExprError {
        let span = self.peek_span();
        ExprError::new(
            "expr/parse",
            span,
            format!("unexpected token, expected {set}"),
            format!("expected {set}"),
        )
    }

    fn check_depth(&self, depth: u32) -> Result<(), ExprError> {
        if depth > MAX_DEPTH {
            Err(ExprError::new(
                "expr/too_deep",
                self.peek_span(),
                "expression nesting exceeds depth 32",
                "flatten the expression",
            ))
        } else {
            Ok(())
        }
    }

    fn parse_if(&mut self, depth: u32) -> Result<Expr, ExprError> {
        self.check_depth(depth)?;
        if matches!(self.peek(), Some(Tok::KwIf)) {
            let (_, start) = self.bump().unwrap();
            let cond = self.parse_or(depth + 1)?;
            match self.bump() {
                Some((Tok::KwThen, _)) => {}
                _ => return Err(self.expected("`then`")),
            }
            let then_branch = self.parse_if(depth + 1)?;
            match self.bump() {
                Some((Tok::KwElse, _)) => {}
                _ => return Err(self.expected("`else`")),
            }
            let else_branch = self.parse_if(depth + 1)?;
            let span = Span::new(start.start as usize, else_branch.span().end as usize);
            return Ok(Expr::If {
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
                widen: None,
                span,
            });
        }
        self.parse_or(depth)
    }

    fn parse_or(&mut self, depth: u32) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_and(depth)?;
        while matches!(self.peek(), Some(Tok::KwOr)) {
            self.bump();
            let rhs = self.parse_and(depth)?;
            let span = Span::new(lhs.span().start as usize, rhs.span().end as usize);
            lhs = Expr::Or {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self, depth: u32) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_not(depth)?;
        while matches!(self.peek(), Some(Tok::KwAnd)) {
            self.bump();
            let rhs = self.parse_not(depth)?;
            let span = Span::new(lhs.span().start as usize, rhs.span().end as usize);
            lhs = Expr::And {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_not(&mut self, depth: u32) -> Result<Expr, ExprError> {
        self.check_depth(depth)?;
        if matches!(self.peek(), Some(Tok::KwNot)) {
            let (_, start) = self.bump().unwrap();
            let inner = self.parse_not(depth + 1)?;
            let span = Span::new(start.start as usize, inner.span().end as usize);
            return Ok(Expr::Not {
                inner: Box::new(inner),
                span,
            });
        }
        self.parse_cmp(depth)
    }

    fn parse_cmp(&mut self, depth: u32) -> Result<Expr, ExprError> {
        let lhs = self.parse_add(depth)?;
        let op = match self.peek() {
            Some(Tok::EqEq) => CmpOp::Eq,
            Some(Tok::BangEq) => CmpOp::Ne,
            Some(Tok::Lt) => CmpOp::Lt,
            Some(Tok::Le) => CmpOp::Le,
            Some(Tok::Gt) => CmpOp::Gt,
            Some(Tok::Ge) => CmpOp::Ge,
            _ => return Ok(lhs),
        };
        let (_, op_span) = self.bump().unwrap();
        let rhs = self.parse_add(depth)?;
        if matches!(
            self.peek(),
            Some(Tok::EqEq | Tok::BangEq | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge)
        ) {
            let span = self.peek_span();
            return Err(ExprError::new(
                "expr/chained_cmp",
                span,
                "comparisons do not chain",
                "use `and` to combine comparisons",
            ));
        }
        let _ = op_span;
        let span = Span::new(lhs.span().start as usize, rhs.span().end as usize);
        Ok(Expr::Cmp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        })
    }

    fn parse_add(&mut self, depth: u32) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_mul(depth)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_mul(depth)?;
            let span = Span::new(lhs.span().start as usize, rhs.span().end as usize);
            lhs = Expr::Bin {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self, depth: u32) -> Result<Expr, ExprError> {
        let mut lhs = self.parse_unary(depth)?;
        while matches!(self.peek(), Some(Tok::Star)) {
            self.bump();
            let rhs = self.parse_unary(depth)?;
            let span = Span::new(lhs.span().start as usize, rhs.span().end as usize);
            lhs = Expr::Bin {
                op: BinOp::Mul,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self, depth: u32) -> Result<Expr, ExprError> {
        self.check_depth(depth)?;
        if matches!(self.peek(), Some(Tok::Minus)) {
            let (_, start) = self.bump().unwrap();
            let inner = self.parse_unary(depth + 1)?;
            let span = Span::new(start.start as usize, inner.span().end as usize);
            return Ok(Expr::Neg {
                inner: Box::new(inner),
                span,
            });
        }
        self.parse_primary(depth)
    }

    fn parse_primary(&mut self, depth: u32) -> Result<Expr, ExprError> {
        match self.bump() {
            Some((Tok::Int(digits), span)) => {
                if digits.parse::<i64>().is_err() {
                    return Err(ExprError::new(
                        "expr/int_range",
                        span,
                        "integer literal does not fit i64",
                        "use a smaller integer",
                    ));
                }
                Ok(Expr::IntLit { digits, span })
            }
            Some((Tok::Dec(digits), span)) => {
                let frac = digits.split_once('.').map(|(_, f)| f.len()).unwrap_or(0);
                let digit_count = digits.bytes().filter(|b| b.is_ascii_digit()).count();
                if frac > 12 || digit_count > 38 {
                    return Err(ExprError::new(
                        "expr/dec_range",
                        span,
                        "decimal literal exceeds scale or digit limits",
                        "use at most 38 digits and 12 fraction digits",
                    ));
                }
                Ok(Expr::DecLit {
                    digits,
                    scale: frac as u8,
                    span,
                })
            }
            Some((Tok::Str(value), span)) => Ok(Expr::StrLit { value, span }),
            Some((Tok::KwTrue, span)) => Ok(Expr::BoolLit { value: true, span }),
            Some((Tok::KwFalse, span)) => Ok(Expr::BoolLit { value: false, span }),
            Some((Tok::KwCtx, start)) => {
                self.expect_dot()?;
                let (name, end) = self.expect_ident()?;
                Ok(Expr::CtxRef {
                    name,
                    span: Span::new(start.start as usize, end.end as usize),
                })
            }
            Some((Tok::KwEvt, start)) => {
                self.expect_dot()?;
                let (name, end) = self.expect_ident()?;
                Ok(Expr::EvtRef {
                    name,
                    span: Span::new(start.start as usize, end.end as usize),
                })
            }
            Some((Tok::TypeIdent(ty), start)) => {
                self.expect_dot()?;
                let (variant, end) = self.expect_ident()?;
                Ok(Expr::EnumLit {
                    ty,
                    variant,
                    span: Span::new(start.start as usize, end.end as usize),
                })
            }
            Some((Tok::Ident(name), name_span)) => match self.peek() {
                Some(Tok::LParen) => {
                    self.bump();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            args.push(self.parse_arg(depth)?);
                            match self.peek() {
                                Some(Tok::Comma) => {
                                    self.bump();
                                }
                                Some(Tok::RParen) => break,
                                _ => return Err(self.expected("`,` or `)`")),
                            }
                        }
                    }
                    let end = match self.bump() {
                        Some((Tok::RParen, s)) => s,
                        _ => return Err(self.expected("`)`")),
                    };
                    Ok(Expr::Call {
                        name,
                        name_span,
                        args,
                        span: Span::new(name_span.start as usize, end.end as usize),
                    })
                }
                _ => Err(ExprError::new(
                    "expr/parse",
                    name_span,
                    "bare identifier is not an expression",
                    "use ctx.name, evt.name, or a call",
                )),
            },
            Some((Tok::LParen, _)) => {
                self.check_depth(depth + 1)?;
                let inner = self.parse_if(depth + 1)?;
                match self.bump() {
                    Some((Tok::RParen, _)) => Ok(inner),
                    _ => Err(self.expected("`)`")),
                }
            }
            Some((_, span)) => Err(ExprError::new(
                "expr/parse",
                span,
                "unexpected token",
                "expected a literal, ctx./evt. reference, call, or `(`",
            )),
            None => Err(ExprError::new(
                "expr/parse",
                self.eof_span(),
                "unexpected end of input",
                "expected a literal, ctx./evt. reference, call, or `(`",
            )),
        }
    }

    fn parse_arg(&mut self, depth: u32) -> Result<Arg, ExprError> {
        if let Some(Tok::Ident(name)) = self.peek().cloned() {
            // bare ident in arg position is a Word unless followed by '(' (a nested call)
            let next = self.tokens.get(self.i + 1).map(|(t, _)| t);
            if !matches!(next, Some(Tok::LParen)) {
                let (_, span) = self.bump().unwrap();
                return Ok(Arg::Word { name, span });
            }
        }
        Ok(Arg::Expr(self.parse_if(depth + 1)?))
    }

    fn expect_dot(&mut self) -> Result<(), ExprError> {
        match self.peek() {
            Some(Tok::Dot) => {
                self.bump();
                Ok(())
            }
            _ => Err(self.expected("`.`")),
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), ExprError> {
        match self.peek() {
            Some(Tok::Ident(_)) => {
                if let Some((Tok::Ident(name), span)) = self.bump() {
                    Ok((name, span))
                } else {
                    Err(self.expected("identifier"))
                }
            }
            _ => Err(ExprError::new(
                "expr/parse",
                self.peek_span(),
                "expected identifier",
                "expected an identifier",
            )),
        }
    }
}
