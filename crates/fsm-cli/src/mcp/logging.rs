//! Structured logging, at a level the client sets.
//!
//! A skeleton: plan 0012 task 6001 fills it.

use fsm_core::json::Value;

/// The RFC 5424 levels MCP names, most severe first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Emergency,
    Alert,
    Critical,
    Error,
    Warning,
    Notice,
    Info,
    Debug,
}

impl Level {
    /// The wire name a client sends and this server echoes.
    pub fn as_str(&self) -> &'static str {
        unimplemented!("plan 0012 task 6001")
    }

    /// Parse a client's level, or `None` if it names no level this server
    /// knows.
    pub fn parse(_name: &str) -> Option<Self> {
        unimplemented!("plan 0012 task 6001")
    }
}

/// The parameters of a `notifications/message`.
pub fn message_params(_level: Level, _logger: &str, _data: Value) -> Value {
    unimplemented!("plan 0012 task 6001")
}
