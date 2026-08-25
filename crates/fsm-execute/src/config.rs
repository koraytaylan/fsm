//! The operator-owned handler table: the executor's security boundary.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::json::Value;

use crate::error::ExecError;

/// The domain event to send after an outcome, exactly as the table declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advance {
    /// An event name the machine's own definition declares.
    pub event: String,
    /// Payload object sent with the event; `{}` when the table omits it.
    pub payload: Value,
    /// Fields the store fills from the injected clock.
    pub stamps: Vec<String>,
}

/// One effect name bound to exactly one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerSpec {
    /// The emitted effect name this handler answers.
    pub effect: String,
    /// The argv template, `argv[0]` first; `{placeholder}` names an effect arg.
    pub argv: Vec<String>,
    /// Milliseconds after which an in-flight run is killed.
    pub timeout_ms: i64,
    /// What to send when the handler exits zero.
    pub on_ok: Option<Advance>,
    /// What to send when it does not.
    pub on_failed: Option<Advance>,
}

/// The closed set of commands the executor can ever run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerTable {
    /// Effect name to its single handler.
    pub handlers: BTreeMap<String, HandlerSpec>,
}

impl HandlerTable {
    /// Parse and fully validate an `fsm.handlers/1` document.
    pub fn parse(src: &str) -> Result<HandlerTable, ExecError> {
        let _ = src;
        unimplemented!("task 3602")
    }
}

/// Replace each `{name}` in `argv` with the string form of that effect arg.
pub fn substitute(argv: &[String], args: &BTreeMap<String, Val>) -> Result<Vec<String>, ExecError> {
    let _ = (argv, args);
    unimplemented!("task 3602")
}
