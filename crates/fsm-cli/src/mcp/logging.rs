//! Structured logging, at a level the client sets.
//!
//! The level itself lands with the routing (task 5702); `6001` adds the
//! notifications that respect it.

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
        match self {
            Level::Emergency => "emergency",
            Level::Alert => "alert",
            Level::Critical => "critical",
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Notice => "notice",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }

    /// Parse a client's level, or `None` if it names no level this server
    /// knows.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "emergency" => Level::Emergency,
            "alert" => Level::Alert,
            "critical" => Level::Critical,
            "error" => Level::Error,
            "warning" => Level::Warning,
            "notice" => Level::Notice,
            "info" => Level::Info,
            "debug" => Level::Debug,
            _ => return None,
        })
    }
}

/// The parameters of a `notifications/message`.
pub fn message_params(_level: Level, _logger: &str, _data: Value) -> Value {
    unimplemented!("plan 0012 task 6001")
}
