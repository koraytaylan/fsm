//! Progress on a call that takes long enough to say something about.
//!
//! A call that takes a while and says nothing is indistinguishable from a
//! hung server. A call that says something a thousand times is worse, so
//! reports are rate-limited — and the final one always arrives, so a slow
//! call ends cleanly rather than trailing off.
//!
//! Time comes from the caller's injected clock, never `Instant::now()`, so a
//! test can decide exactly how many reports a run produces.
//!
//! Plan 0012 task 6002.

use std::cell::Cell;
use std::collections::BTreeMap;

use fsm_core::json::Value;

use super::notify::Notifier;

/// At most one report per this many milliseconds of wall time.
pub const MIN_INTERVAL_MS: i64 = 100;

/// Reports progress for one call, or discards it.
///
/// A call with no `progressToken` gets a discarding reporter rather than an
/// `Option`, so every call site reports unconditionally and no handler needs
/// an `if` — and a discarding reporter emits **zero** notifications, which is
/// what keeps every existing golden byte-identical.
pub struct ProgressReporter {
    token: Option<Value>,
    notifier: Option<Notifier>,
    last_ms: Cell<Option<i64>>,
}

impl ProgressReporter {
    /// A reporter that says nothing.
    pub fn discarding() -> Self {
        Self {
            token: None,
            notifier: None,
            last_ms: Cell::new(None),
        }
    }

    /// A reporter for a request that carried a token.
    pub fn new(token: Value, notifier: Notifier) -> Self {
        Self {
            token: Some(token),
            notifier: Some(notifier),
            last_ms: Cell::new(None),
        }
    }

    /// Build one from a request's `_meta`, discarding when there is no token.
    pub fn from_meta(meta: Option<&Value>, notifier: Option<&Notifier>) -> Self {
        match (meta.and_then(token_of), notifier) {
            (Some(token), Some(notifier)) => Self::new(token, notifier.clone_handle()),
            _ => Self::discarding(),
        }
    }

    /// Whether this reporter would say anything at all.
    pub fn is_live(&self) -> bool {
        self.token.is_some()
    }

    /// Report progress, subject to the rate limit.
    ///
    /// `final_report` is emitted regardless of the limit: a fast call
    /// produces one notification rather than a thousand, and a slow one still
    /// ends on a report whose `progress` equals its `total`.
    pub fn report(
        &self,
        now_ms: i64,
        progress: u64,
        total: Option<u64>,
        message: Option<&str>,
        final_report: bool,
    ) {
        let (Some(token), Some(notifier)) = (&self.token, &self.notifier) else {
            return;
        };
        if !final_report
            && let Some(last) = self.last_ms.get()
            && now_ms - last < MIN_INTERVAL_MS
        {
            return;
        }
        self.last_ms.set(Some(now_ms));
        let mut params = BTreeMap::from([
            ("progressToken".to_string(), token.clone()),
            ("progress".to_string(), Value::Num(progress.to_string())),
        ]);
        if let Some(total) = total {
            // A progress bar without a denominator is barely better than
            // silence, so `total` is always sent when the size is known.
            params.insert("total".into(), Value::Num(total.to_string()));
        }
        if let Some(message) = message {
            params.insert("message".into(), Value::Str(message.into()));
        }
        let _ = notifier.notify("notifications/progress", Value::Obj(params));
    }
}

/// The progress token a request's `_meta` carried, if any.
pub fn token_of(meta: &Value) -> Option<Value> {
    meta.get("progressToken").cloned()
}

/// The parameters of a `notifications/progress`, for a caller assembling one
/// directly.
pub fn progress_params(token: &Value, progress: u64, total: Option<u64>) -> Value {
    let mut params = BTreeMap::from([
        ("progressToken".to_string(), token.clone()),
        ("progress".to_string(), Value::Num(progress.to_string())),
    ]);
    if let Some(total) = total {
        params.insert("total".into(), Value::Num(total.to_string()));
    }
    Value::Obj(params)
}
