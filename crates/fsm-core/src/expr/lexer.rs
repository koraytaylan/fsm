//! Expression lexer: keywords, identifiers, numbers, strings, operators.

use super::ExprError;
use crate::json::unescape_string;

pub const SOURCE_CAP: usize = 4096;

/// Byte offsets into the verbatim source, half-open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
        }
    }

    pub fn point(at: usize) -> Self {
        Self {
            start: at as u32,
            end: at as u32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Int(String),
    Dec(String),
    Str(String),
    Ident(String),
    TypeIdent(String),
    KwIf,
    KwThen,
    KwElse,
    KwAnd,
    KwOr,
    KwNot,
    KwTrue,
    KwFalse,
    KwCtx,
    KwEvt,
    Dot,
    Comma,
    LParen,
    RParen,
    EqEq,
    BangEq,
    Le,
    Lt,
    Ge,
    Gt,
    Plus,
    Minus,
    Star,
}

pub fn lex(src: &str) -> Result<Vec<(Tok, Span)>, ExprError> {
    if src.len() > SOURCE_CAP {
        return Err(ExprError::new(
            "expr/too_long",
            Span::new(0, src.len().min(SOURCE_CAP + 1)),
            "expression source exceeds 4096 bytes",
            "split the expression or shorten identifiers and literals",
        ));
    }
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            i += 1;
            continue;
        }
        let start = i;
        if b.is_ascii_lowercase() || b == b'_' {
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() || bytes[i] == b'_')
            {
                i += 1;
            }
            if i - start > 64 {
                return Err(lex_err(
                    start,
                    i,
                    "identifier exceeds 64 characters",
                    "shorten the identifier",
                ));
            }
            let word = &src[start..i];
            let tok = match word {
                "if" => Tok::KwIf,
                "then" => Tok::KwThen,
                "else" => Tok::KwElse,
                "and" => Tok::KwAnd,
                "or" => Tok::KwOr,
                "not" => Tok::KwNot,
                "true" => Tok::KwTrue,
                "false" => Tok::KwFalse,
                "ctx" => Tok::KwCtx,
                "evt" => Tok::KwEvt,
                _ => Tok::Ident(word.to_string()),
            };
            out.push((tok, Span::new(start, i)));
            continue;
        }
        if b.is_ascii_uppercase() {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i - start > 64 {
                return Err(lex_err(
                    start,
                    i,
                    "type identifier exceeds 64 characters",
                    "shorten the type name",
                ));
            }
            out.push((
                Tok::TypeIdent(src[start..i].to_string()),
                Span::new(start, i),
            ));
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                let dot = i;
                i += 1;
                if i < bytes.len() && bytes[i] == b'.' {
                    return Err(lex_err(
                        i,
                        i + 1,
                        "unexpected second decimal point",
                        "use a single fraction part",
                    ));
                }
                if i >= bytes.len() || !bytes[i].is_ascii_digit() {
                    return Err(lex_err(
                        dot,
                        i,
                        "decimal needs a digit on each side of the dot",
                        "write 1.0 not 1.",
                    ));
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'.' {
                    return Err(lex_err(
                        i,
                        i + 1,
                        "unexpected second decimal point",
                        "use a single fraction part",
                    ));
                }
                out.push((Tok::Dec(src[start..i].to_string()), Span::new(start, i)));
            } else {
                out.push((Tok::Int(src[start..i].to_string()), Span::new(start, i)));
            }
            continue;
        }
        if b == b'.' {
            if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                return Err(lex_err(
                    i,
                    i + 1,
                    "decimal needs a digit on each side of the dot",
                    "write 0.5 not .5",
                ));
            }
            out.push((Tok::Dot, Span::new(i, i + 1)));
            i += 1;
            continue;
        }
        if b == b'"' {
            i += 1;
            let content_start = i;
            let mut escaped = false;
            loop {
                if i >= bytes.len() {
                    return Err(lex_err(
                        src.len(),
                        src.len(),
                        "unterminated string",
                        "close the string literal",
                    ));
                }
                let c = bytes[i];
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if c == b'\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                if c == b'"' {
                    let raw = &src[content_start..i];
                    i += 1;
                    let value = unescape_string(raw).map_err(|_| {
                        lex_err(start, i, "invalid string escape", "use JSON-style escapes")
                    })?;
                    out.push((Tok::Str(value), Span::new(start, i)));
                    break;
                }
                if c < 0x20 {
                    return Err(lex_err(
                        i,
                        i + 1,
                        "raw control character in string",
                        "escape control characters",
                    ));
                }
                i += 1;
            }
            continue;
        }
        match b {
            b'(' => {
                out.push((Tok::LParen, Span::new(i, i + 1)));
                i += 1;
            }
            b')' => {
                out.push((Tok::RParen, Span::new(i, i + 1)));
                i += 1;
            }
            b',' => {
                out.push((Tok::Comma, Span::new(i, i + 1)));
                i += 1;
            }
            b'+' => {
                out.push((Tok::Plus, Span::new(i, i + 1)));
                i += 1;
            }
            b'-' => {
                out.push((Tok::Minus, Span::new(i, i + 1)));
                i += 1;
            }
            b'*' => {
                out.push((Tok::Star, Span::new(i, i + 1)));
                i += 1;
            }
            b'<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    out.push((Tok::Le, Span::new(i, i + 2)));
                    i += 2;
                } else {
                    out.push((Tok::Lt, Span::new(i, i + 1)));
                    i += 1;
                }
            }
            b'>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    out.push((Tok::Ge, Span::new(i, i + 2)));
                    i += 2;
                } else {
                    out.push((Tok::Gt, Span::new(i, i + 1)));
                    i += 1;
                }
            }
            b'=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    out.push((Tok::EqEq, Span::new(i, i + 2)));
                    i += 2;
                } else {
                    return Err(lex_err(i, i + 1, "unexpected '='", "use == for equality"));
                }
            }
            b'!' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    out.push((Tok::BangEq, Span::new(i, i + 2)));
                    i += 2;
                } else {
                    return Err(lex_err(i, i + 1, "unexpected '!'", "use != for inequality"));
                }
            }
            b'/' => {
                return Err(ExprError::new(
                    "expr/lex",
                    Span::new(i, i + 1),
                    "unexpected '/'",
                    "use div(a, b, scale, mode)",
                ));
            }
            b'%' => {
                return Err(lex_err(
                    i,
                    i + 1,
                    "unexpected '%'",
                    "the language has no remainder operator",
                ));
            }
            _ => {
                return Err(lex_err(
                    i,
                    i + 1,
                    &format!("unexpected byte {b:?}"),
                    "check for an illegal character",
                ));
            }
        }
    }
    Ok(out)
}

fn lex_err(start: usize, end: usize, message: &str, hint: &str) -> ExprError {
    ExprError::new("expr/lex", Span::new(start, end), message, hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|(t, _)| t).collect()
    }

    #[test]
    fn every_token_kind() {
        assert_eq!(kinds("123"), vec![Tok::Int("123".into())]);
        assert_eq!(kinds("1.50"), vec![Tok::Dec("1.50".into())]);
        assert_eq!(kinds("\"hi\""), vec![Tok::Str("hi".into())]);
        assert_eq!(kinds("foo"), vec![Tok::Ident("foo".into())]);
        assert_eq!(kinds("Risk"), vec![Tok::TypeIdent("Risk".into())]);
        assert_eq!(kinds("if"), vec![Tok::KwIf]);
        assert_eq!(kinds("then"), vec![Tok::KwThen]);
        assert_eq!(kinds("else"), vec![Tok::KwElse]);
        assert_eq!(kinds("and"), vec![Tok::KwAnd]);
        assert_eq!(kinds("or"), vec![Tok::KwOr]);
        assert_eq!(kinds("not"), vec![Tok::KwNot]);
        assert_eq!(kinds("true"), vec![Tok::KwTrue]);
        assert_eq!(kinds("false"), vec![Tok::KwFalse]);
        assert_eq!(kinds("ctx"), vec![Tok::KwCtx]);
        assert_eq!(kinds("evt"), vec![Tok::KwEvt]);
        assert_eq!(kinds("."), vec![Tok::Dot]);
        assert_eq!(kinds(","), vec![Tok::Comma]);
        assert_eq!(kinds("("), vec![Tok::LParen]);
        assert_eq!(kinds(")"), vec![Tok::RParen]);
        assert_eq!(kinds("=="), vec![Tok::EqEq]);
        assert_eq!(kinds("!="), vec![Tok::BangEq]);
        assert_eq!(kinds("<="), vec![Tok::Le]);
        assert_eq!(kinds("<"), vec![Tok::Lt]);
        assert_eq!(kinds(">="), vec![Tok::Ge]);
        assert_eq!(kinds(">"), vec![Tok::Gt]);
        assert_eq!(kinds("+"), vec![Tok::Plus]);
        assert_eq!(kinds("-"), vec![Tok::Minus]);
        assert_eq!(kinds("*"), vec![Tok::Star]);
    }

    #[test]
    fn keywords_vs_idents() {
        assert_eq!(kinds("if"), vec![Tok::KwIf]);
        assert_eq!(kinds("iff"), vec![Tok::Ident("iff".into())]);
        assert_eq!(kinds("if_"), vec![Tok::Ident("if_".into())]);
        assert_eq!(kinds("ifx"), vec![Tok::Ident("ifx".into())]);
        assert_eq!(kinds("ctx"), vec![Tok::KwCtx]);
        assert_eq!(kinds("evt"), vec![Tok::KwEvt]);
        assert_eq!(kinds("Risk"), vec![Tok::TypeIdent("Risk".into())]);
        assert_eq!(kinds("half_even"), vec![Tok::Ident("half_even".into())]);
        assert_eq!(kinds("ms"), vec![Tok::Ident("ms".into())]);
    }

    #[test]
    fn operator_adjacency() {
        assert_eq!(kinds(">="), vec![Tok::Ge]);
        let err = lex("> =").unwrap_err();
        assert_eq!(err.code, "expr/lex");
        assert_eq!(err.span, Span::new(2, 3));
        assert!(!err.message.is_empty() && !err.hint.is_empty());
        assert_eq!(kinds("=="), vec![Tok::EqEq]);
        assert_eq!(lex("=").unwrap_err().code, "expr/lex");
        assert_eq!(kinds("!="), vec![Tok::BangEq]);
        assert_eq!(lex("!").unwrap_err().code, "expr/lex");
    }

    #[test]
    fn rejected_div_and_rem() {
        let err = lex("a / b").unwrap_err();
        assert_eq!(err.code, "expr/lex");
        assert!(err.hint.contains("div(a, b, scale, mode)"), "{}", err.hint);
        let err = lex("%").unwrap_err();
        assert_eq!(err.code, "expr/lex");
        assert!(!err.hint.is_empty());
    }

    #[test]
    fn number_forms() {
        assert_eq!(lex("1.").unwrap_err().code, "expr/lex");
        assert_eq!(lex(".5").unwrap_err().code, "expr/lex");
        let err = lex("1..2").unwrap_err();
        assert_eq!(err.code, "expr/lex");
        assert_eq!(err.span, Span::new(2, 3));
    }

    #[test]
    fn strings() {
        assert_eq!(kinds(r#""a\"b""#), vec![Tok::Str("a\"b".into())]);
        assert_eq!(kinds(r#""\n\t\\""#), vec![Tok::Str("\n\t\\".into())]);
        assert_eq!(lex("\"abc").unwrap_err().code, "expr/lex");
        assert_eq!(lex(r#""\q""#).unwrap_err().code, "expr/lex");
    }

    #[test]
    fn span_exactness_multibyte() {
        // Multibyte UTF-8 (é = two bytes) lives inside a string literal.
        let src = "\"é\" + ctx.x";
        let toks = lex(src).unwrap();
        assert_eq!(toks[0], (Tok::Str("é".into()), Span::new(0, 4))); // " + 2-byte é + "
        assert_eq!(toks[1], (Tok::Plus, Span::new(5, 6)));
        assert_eq!(toks[2], (Tok::KwCtx, Span::new(7, 10)));
        assert_eq!(toks[3], (Tok::Dot, Span::new(10, 11)));
        assert_eq!(toks[4], (Tok::Ident("x".into()), Span::new(11, 12)));
    }

    #[test]
    fn source_cap() {
        let ok = format!("{}1", " ".repeat(4095));
        assert_eq!(ok.len(), 4096);
        assert!(lex(&ok).is_ok());
        let bad = "a".repeat(4097);
        let err = lex(&bad).unwrap_err();
        assert_eq!(err.code, "expr/too_long");
        assert!(!err.message.is_empty() && !err.hint.is_empty());
    }
}
