//! Expression language: lex, parse, typecheck, evaluate, partial-evaluate.

pub mod ast;
pub mod eval;
pub mod lexer;
pub mod parser;
pub mod partial;
pub mod typeck;

use crate::expr::lexer::Span;

/// Span-precise expression error. `hint` is mandatory and generated from failure data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub code: &'static str,
    pub span: Span,
    pub message: String,
    pub hint: String,
    pub details: Vec<(String, String)>,
}

impl ExprError {
    pub fn new(
        code: &'static str,
        span: Span,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            code,
            span,
            message: message.into(),
            hint: hint.into(),
            details: Vec::new(),
        }
    }

    pub fn with_details(mut self, details: Vec<(String, String)>) -> Self {
        self.details = details;
        self
    }
}
