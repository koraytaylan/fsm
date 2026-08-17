//! Stable error codes, retryability, and the shared error object.

use crate::json::Value;
use std::collections::BTreeMap;

/// Every stable `code` the engine and shell may emit, sorted and unique.
pub const ALL_CODES: &[&str] = &[
    "def/ancestor_shadowed",
    "def/assign_type",
    "def/create_always_fails",
    "def/dup_name",
    "def/dup_set",
    "def/duplicate_guard",
    "def/from_history",
    "def/history_target_from_inside",
    "def/initial_is_history",
    "def/initial_not_child",
    "def/initial_terminal",
    "def/limit_bytes",
    "def/limit_cell",
    "def/limit_ctx",
    "def/limit_depth",
    "def/limit_emits",
    "def/limit_enums",
    "def/limit_events",
    "def/limit_fields",
    "def/limit_history",
    "def/limit_invariants",
    "def/limit_sets",
    "def/limit_states",
    "def/limit_transitions",
    "def/limit_variants",
    "def/multiple_history",
    "def/not_supported",
    "def/one_initial",
    "def/reserved_ident",
    "def/shadowed",
    "def/shape",
    "def/terminal_has_transitions",
    "def/terminal_not_leaf",
    "def/unknown_effect",
    "def/unknown_enum",
    "def/unknown_event",
    "def/unknown_key",
    "def/unknown_state",
    "def/unreachable_state",
    "expr/arity",
    "expr/chained_cmp",
    "expr/cmp_unordered",
    "expr/dec_range",
    "expr/evt_in_block",
    "expr/evt_in_invariant",
    "expr/int_range",
    "expr/lex",
    "expr/mixed_class",
    "expr/mode_invalid",
    "expr/parse",
    "expr/round_widens",
    "expr/scale_cap",
    "expr/scale_narrow",
    "expr/scale_not_literal",
    "expr/too_deep",
    "expr/too_long",
    "expr/type_mismatch",
    "expr/unknown_builtin",
    "expr/unknown_enum",
    "expr/unknown_field",
    "expr/unknown_var",
    "expr/unknown_variant",
    "internal/budget",
    "internal/unimplemented",
    "io/read",
    "io/write",
    "req/args_invalid",
    "req/event_unknown",
    "req/field_missing",
    "req/field_scale",
    "req/field_type",
    "req/field_unknown",
    "req/instance_not_found",
    "req/machine_ambiguous",
    "req/machine_exists",
    "req/machine_not_found",
    "req/number_token",
    "req/seq_mismatch",
    "run/action_error",
    "run/create_failed",
    "run/div_zero",
    "run/guard_error",
    "run/instance_cancelled",
    "run/instance_completed",
    "run/invariant",
    "run/not_enabled",
    "run/overflow",
    "run/unhandled",
    "store/chain_broken",
    "store/lock",
    "store/non_canonical",
    "store/state_hash_mismatch",
    "store/torn_tail",
    "store/version_mismatch",
];

/// Retryable solely from the code namespace. `req/seq_mismatch` is the
/// load-bearing true: the request_id was not consumed.
pub fn retryable(code: &str) -> bool {
    matches!(
        code,
        "req/seq_mismatch" | "io/read" | "io/write" | "store/lock" | "internal/budget"
    ) || code.starts_with("io/")
        || code.starts_with("store/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsmError {
    pub code: String,
    pub message: String,
    pub path: String,
    pub span: Option<(u32, u32)>,
    pub hint: String,
    pub retryable: bool,
    pub details: Value,
    pub docs: String,
}

impl FsmError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: String::new(),
            span: None,
            hint: String::new(),
            retryable: retryable(code),
            details: Value::Obj(BTreeMap::new()),
            docs: format!("fsm://docs/spec#{code}"),
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn to_value(&self) -> Value {
        let mut m = BTreeMap::new();
        m.insert("code".into(), Value::Str(self.code.clone()));
        m.insert("message".into(), Value::Str(self.message.clone()));
        m.insert("path".into(), Value::Str(self.path.clone()));
        if let Some((s, e)) = self.span {
            let mut sp = BTreeMap::new();
            sp.insert("start".into(), Value::Num(s.to_string()));
            sp.insert("end".into(), Value::Num(e.to_string()));
            m.insert("span".into(), Value::Obj(sp));
        }
        m.insert("hint".into(), Value::Str(self.hint.clone()));
        m.insert("retryable".into(), Value::Bool(self.retryable));
        m.insert("details".into(), self.details.clone());
        m.insert("docs".into(), Value::Str(self.docs.clone()));
        Value::Obj(m)
    }
}
