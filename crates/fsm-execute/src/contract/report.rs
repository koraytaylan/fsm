//! The versioned, sanitized effect-contract report.

use fsm_core::json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The one report shape emitted by this analyzer.
pub const FORMAT: &str = "fsm.executor-check/1";

/// Structural evidence, distinct from workflow progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Compatible,
    Invalid,
    Unknown,
}

impl CheckStatus {
    /// The serialized status tag.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compatible => "compatible",
            Self::Invalid => "invalid",
            Self::Unknown => "unknown",
        }
    }
}

/// Independent work and output ceilings; accepted exact boundaries are inclusive.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub definitions: usize,
    pub sites: usize,
    pub findings: usize,
    pub report_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            definitions: 32,
            sites: 4096,
            findings: 4096,
            report_bytes: 1_048_576,
        }
    }
}

/// One actionable structural finding, containing no private handler configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub severity: &'static str,
    pub machine_id: String,
    pub path: String,
    pub effect: Option<String>,
    pub outcome: Option<String>,
    pub message: String,
    pub hint: String,
    pub cause: Option<Value>,
}

/// One syntactic emit, with authoritative inferred argument types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSite {
    pub machine_id: String,
    pub path: String,
    pub effect: String,
    pub arguments: BTreeMap<String, Option<String>>,
    pub disposition: &'static str,
    pub required_args: BTreeSet<String>,
    pub outcomes: BTreeMap<String, &'static str>,
}

/// Sanitized report whose scope states which structural obligations were checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub status: CheckStatus,
    pub machine_id: Option<String>,
    pub contract_id: Option<String>,
    pub definitions: BTreeSet<String>,
    pub effects_checked: bool,
    pub outcomes_checked: bool,
    pub dynamic_signals: bool,
    pub findings: Vec<Finding>,
    pub effects: Vec<EffectSite>,
    pub progress: BTreeSet<String>,
}

pub(super) fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

pub(super) fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Arr(values.into_iter().map(Value::Str).collect())
}

impl Report {
    /// Serialize the closed envelope without copying private table data.
    pub fn to_value(&self) -> Value {
        let mut value = self.envelope();
        if let Value::Obj(fields) = &mut value {
            fields.insert(
                "findings".into(),
                Value::Arr(self.findings.iter().map(Finding::to_value).collect()),
            );
            fields.insert(
                "effects".into(),
                Value::Arr(self.effects.iter().map(EffectSite::to_value).collect()),
            );
        }
        value
    }

    pub(super) fn envelope(&self) -> Value {
        object([
            ("format", Value::Str(FORMAT.into())),
            ("status", Value::Str(self.status.as_str().into())),
            (
                "machine_id",
                self.machine_id.clone().map_or(Value::Null, Value::Str),
            ),
            (
                "contract_id",
                self.contract_id.clone().map_or(Value::Null, Value::Str),
            ),
            ("definitions", strings(self.definitions.iter().cloned())),
            (
                "scope",
                object([
                    ("effects_checked", Value::Bool(self.effects_checked)),
                    ("outcomes_checked", Value::Bool(self.outcomes_checked)),
                    ("dynamic_signals", Value::Bool(self.dynamic_signals)),
                ]),
            ),
            ("findings", Value::Arr(Vec::new())),
            ("effects", Value::Arr(Vec::new())),
            ("progress", strings(self.progress.iter().cloned())),
        ])
    }
}

impl Finding {
    pub(super) fn to_value(&self) -> Value {
        object([
            ("code", Value::Str(self.code.into())),
            ("severity", Value::Str(self.severity.into())),
            ("machine_id", Value::Str(self.machine_id.clone())),
            ("path", Value::Str(self.path.clone())),
            (
                "effect",
                self.effect.clone().map_or(Value::Null, Value::Str),
            ),
            (
                "outcome",
                self.outcome.clone().map_or(Value::Null, Value::Str),
            ),
            ("message", Value::Str(self.message.clone())),
            ("hint", Value::Str(self.hint.clone())),
            ("cause", self.cause.clone().unwrap_or(Value::Null)),
        ])
    }
}

impl EffectSite {
    pub(super) fn to_value(&self) -> Value {
        object([
            ("machine_id", Value::Str(self.machine_id.clone())),
            ("path", Value::Str(self.path.clone())),
            ("effect", Value::Str(self.effect.clone())),
            (
                "arguments",
                Value::Obj(
                    self.arguments
                        .iter()
                        .map(|(key, value)| {
                            (key.clone(), value.clone().map_or(Value::Null, Value::Str))
                        })
                        .collect(),
                ),
            ),
            ("disposition", Value::Str(self.disposition.into())),
            ("required_args", strings(self.required_args.iter().cloned())),
            (
                "outcomes",
                Value::Obj(
                    self.outcomes
                        .iter()
                        .map(|(key, value)| (key.clone(), Value::Str((*value).into())))
                        .collect(),
                ),
            ),
        ])
    }
}
