//! The executor's own failure domain.
//!
//! These codes are deliberately *not* engine codes: nothing here is a
//! statement about statechart semantics, so none of them belongs in
//! `fsm_core::error::ALL_CODES` or in SPEC.md's appendix. They live under the
//! `exec/` namespace and are documented for operators in `docs/EMBEDDING.md`,
//! which a mechanical test pins against [`ALL_CODES`] below.
//!
//! The shape mirrors the store's `ErrorObj` philosophy — a namespaced code, a
//! message, a hint that states the fix — without reusing that type, so the two
//! failure domains stay honestly separate. A store failure the executor cannot
//! act on is wrapped as `exec/store` with the original preserved in `details`.

use fsm_core::json::Value;
use fsm_store::store::ErrorObj;

/// Every code this crate can raise, in the order the architecture lists them.
///
/// Task `4101`'s doc test asserts each entry appears in `docs/EMBEDDING.md`,
/// so adding a code without documenting it fails the suite.
pub const ALL_CODES: &[&str] = &[
    "exec/config",
    "exec/effect_unresolved",
    "exec/unhandled_effect",
    "exec/spawn",
    "exec/timeout",
    "exec/cancelled",
    "exec/store",
    "exec/mode",
    "exec/invoke",
    "exec/signal",
    // Plan 0016 registers its codes in one task, so no later task edits
    // this file.
    "exec/retries_exhausted",
    "exec/mcp_protocol",
    "exec/mcp_tool",
    "exec/inflight_deferred",
];

/// One executor failure, carrying enough to report the fault without the
/// caller reconstructing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecError {
    /// Stable namespaced code from [`ALL_CODES`].
    pub code: &'static str,
    /// What went wrong, in one sentence.
    pub message: String,
    /// What to do about it, when there is a fix to state.
    pub hint: Option<String>,
    /// Structured context: the offending handler index, the underlying store
    /// error, the effect id that could not be resolved.
    pub details: Option<Value>,
}

impl ExecError {
    /// Build an error with a code and a message and nothing else.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            details: None,
        }
    }

    /// Attach the fix.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach structured context.
    pub fn details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Wrap a store failure as `exec/store`, preserving the original.
    ///
    /// The store's own code is the diagnostic that matters — `store/lock`
    /// means back off and retry the next tick, `req/request_id_conflict`
    /// means two writers raced the same effect — so it is kept verbatim in
    /// `details` rather than flattened into a message.
    pub fn store(error: &ErrorObj) -> Self {
        Self::new("exec/store", error.message.clone())
            .hint(error.hint.clone())
            .details(error.to_value())
    }

    /// Wrap a store failure under a directive-specific code, preserving the
    /// original exactly as [`ExecError::store`] does.
    ///
    /// A composition directive says which half of the plan failed —
    /// `exec/invoke` for creating or returning a child, `exec/signal` for a
    /// delivery — while the store's own code stays in `details`, where the
    /// operator reads it.
    pub fn from_store(code: &'static str, error: &ErrorObj) -> Self {
        Self::new(code, error.message.clone())
            .hint(error.hint.clone())
            .details(error.to_value())
    }

    /// The store code this error wraps, for callers that must distinguish a
    /// benign store outcome from a real one.
    pub fn store_code(&self) -> Option<&str> {
        if !matches!(self.code, "exec/store" | "exec/invoke" | "exec/signal") {
            return None;
        }
        self.details.as_ref()?.get("code")?.as_str()
    }
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ExecError {}

#[cfg(test)]
mod tests {
    use super::{ALL_CODES, ExecError};
    use fsm_core::json::Value;
    use fsm_store::store::ErrorObj;

    #[test]
    fn all_codes_are_unique_namespaced_and_non_empty() {
        let mut seen = std::collections::BTreeSet::new();
        for code in ALL_CODES {
            assert!(!code.is_empty());
            assert!(
                code.starts_with("exec/"),
                "{code} is not in the exec namespace"
            );
            assert!(code.len() > "exec/".len(), "{code} has an empty suffix");
            assert!(seen.insert(*code), "{code} is listed twice");
        }
        assert_eq!(
            seen.len(),
            14,
            "the closed set is fourteen codes since plan 0016"
        );
    }

    #[test]
    fn constructors_retain_every_field() {
        let error = ExecError::new("exec/timeout", "handler exceeded 120000 ms")
            .hint("raise timeout_ms or make the handler faster")
            .details(Value::Str("order-1/3/0".into()));
        assert_eq!(error.code, "exec/timeout");
        assert_eq!(error.message, "handler exceeded 120000 ms");
        assert_eq!(
            error.hint.as_deref(),
            Some("raise timeout_ms or make the handler faster")
        );
        assert_eq!(error.details, Some(Value::Str("order-1/3/0".into())));
        assert_eq!(
            error.to_string(),
            "exec/timeout: handler exceeded 120000 ms"
        );
    }

    #[test]
    fn a_wrapped_store_error_keeps_its_own_code() {
        let wrapped = ExecError::store(&ErrorObj::new("store/lock", "data dir is locked"));
        assert_eq!(wrapped.code, "exec/store");
        assert_eq!(wrapped.message, "data dir is locked");
        assert_eq!(wrapped.store_code(), Some("store/lock"));
        assert_eq!(
            ExecError::new("exec/mode", "another process holds the writer").store_code(),
            None
        );
    }
}
