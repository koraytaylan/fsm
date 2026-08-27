//! Structured logging, at a level the client sets.
//!
//! A client sets a level and gets what it asked for; everything also keeps
//! going to stderr, because an operator reading a terminal must not lose
//! output because a client attached. The two audiences are different, and
//! the duplication is deliberate.
//!
//! Plan 0012 tasks 5702 (the levels) and 6001 (the notifications).

use std::collections::BTreeMap;

use fsm_core::json::Value;

use super::notify::Notifier;

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

impl Level {
    /// Severity order for the threshold test. `emergency` is 0 and `debug`
    /// is 7, matching RFC 5424, so "at least as severe as" is `<=`.
    fn severity(self) -> u8 {
        match self {
            Level::Emergency => 0,
            Level::Alert => 1,
            Level::Critical => 2,
            Level::Error => 3,
            Level::Warning => 4,
            Level::Notice => 5,
            Level::Info => 6,
            Level::Debug => 7,
        }
    }

    /// Every level, for a refusal that lists what it would have accepted.
    pub fn all() -> [Level; 8] {
        [
            Level::Debug,
            Level::Info,
            Level::Notice,
            Level::Warning,
            Level::Error,
            Level::Critical,
            Level::Alert,
            Level::Emergency,
        ]
    }

    /// The eight names, joined for a hint.
    pub fn names() -> String {
        Level::all()
            .iter()
            .map(|level| level.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The default a session starts at, before any `logging/setLevel`.
pub const DEFAULT_LEVEL: Level = Level::Info;

/// The parameters of a `notifications/message`.
pub fn message_params(level: Level, logger: &str, data: Value) -> Value {
    Value::Obj(BTreeMap::from([
        ("level".to_string(), Value::Str(level.as_str().into())),
        ("logger".into(), Value::Str(logger.into())),
        ("data".into(), data),
    ]))
}

/// Send one message, if the session asked for messages this severe.
///
/// The threshold is checked **before** serialization: a debug message on an
/// info session should cost nothing, and a server that rendered first would
/// pay for every message nobody wanted.
///
/// Nothing is sent before `initialize` completes — a notification to a
/// client that has not negotiated the capability is a protocol error — and
/// the only pre-initialize producer is the startup line, which stderr
/// already has.
pub fn message(
    out: &Notifier,
    session_level: Option<Level>,
    initialized: bool,
    level: Level,
    logger: &str,
    data: impl FnOnce() -> Value,
) {
    if !initialized {
        return;
    }
    let threshold = session_level.unwrap_or(DEFAULT_LEVEL);
    if level.severity() > threshold.severity() {
        return;
    }
    let _ = out.notify(
        "notifications/message",
        message_params(level, logger, data()),
    );
}
