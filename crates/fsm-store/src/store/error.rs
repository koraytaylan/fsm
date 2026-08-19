use std::collections::BTreeMap;

use fsm_core::error::{FsmError, retryable};
use fsm_core::json::Value;
use fsm_core::spec::Finding;
use fsm_core::step::Rejection;

use super::Store;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorObj {
    pub code: String,
    pub message: String,
    pub path: String,
    pub span: Option<(u32, u32)>,
    pub source: Option<String>,
    pub hint: String,
    pub retryable: bool,
    pub details: Value,
    pub docs: String,
    pub duplicate: bool,
}

impl ErrorObj {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            code: code.into(),
            message: message.clone(),
            path: String::new(),
            span: None,
            source: None,
            hint: message,
            retryable: retryable(code),
            details: Value::Obj(BTreeMap::new()),
            docs: format!("fsm://docs/spec#{code}"),
            duplicate: false,
        }
    }

    pub fn mark_duplicate(mut self) -> Self {
        self.duplicate = true;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn request_id(mut self, rid: &str) -> Self {
        if let Value::Obj(d) = &mut self.details {
            d.insert("request_id".into(), Value::Str(rid.into()));
        }
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
        m.insert("duplicate".into(), Value::Bool(self.duplicate));
        m.insert("details".into(), self.details.clone());
        m.insert("docs".into(), Value::Str(self.docs.clone()));
        Value::Obj(m)
    }

    pub fn from_fsm(e: FsmError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            path: e.path,
            span: e.span,
            source: None,
            hint: e.hint,
            retryable: e.retryable,
            details: e.details,
            docs: e.docs,
            duplicate: false,
        }
    }

    pub fn from_rejection(r: &Rejection) -> Self {
        let mut e = Self::new(r.code, r.message.clone()).hint(r.hint.clone());
        e.span = r.span;
        if let Value::Obj(d) = &mut e.details {
            if let Some(b) = &r.block {
                d.insert("block".into(), Value::Str(b.clone()));
            }
            if let Some(c) = r.cause {
                d.insert("cause".into(), Value::Str(c.into()));
            }
            if let Some(s) = r.source_state.as_ref() {
                d.insert("source_state".into(), Value::Str(s.clone()));
            }
            if let Some(idx) = r.transition_idx {
                d.insert("transition_idx".into(), Value::Num(idx.to_string()));
            }
            d.insert("trace".into(), r.trace.to_value());
        }
        e
    }

    pub fn with_store_catalog(mut self, store: &Store) -> Self {
        if let Value::Obj(d) = &mut self.details {
            let machines: Vec<Value> = store
                .state
                .machines
                .keys()
                .cloned()
                .map(Value::Str)
                .collect();
            let instances: Vec<Value> = store
                .state
                .instances
                .keys()
                .cloned()
                .map(Value::Str)
                .collect();
            if !machines.is_empty() {
                d.insert("known_machines".into(), Value::Arr(machines));
            }
            if !instances.is_empty() {
                d.insert("known_instances".into(), Value::Arr(instances));
            }
        }
        self
    }

    pub fn from_findings(fs: Vec<Finding>) -> Self {
        let first = fs.first().map(|f| f.code).unwrap_or("def/shape");
        let mut details = BTreeMap::new();
        details.insert(
            "findings".into(),
            Value::Arr(
                fs.iter()
                    .map(|f| {
                        let mut m = BTreeMap::new();
                        m.insert("code".into(), Value::Str(f.code.into()));
                        m.insert("message".into(), Value::Str(f.message.clone()));
                        m.insert("path".into(), Value::Str(f.path.clone()));
                        m.insert("hint".into(), Value::Str(f.hint.clone()));
                        Value::Obj(m)
                    })
                    .collect(),
            ),
        );
        let hint = fs.first().map(|f| f.hint.clone()).unwrap_or_default();
        let path = fs.first().map(|f| f.path.clone()).unwrap_or_default();
        Self::new(
            first,
            fs.iter().map(|f| f.code).collect::<Vec<_>>().join(","),
        )
        .hint(hint)
        .path(path)
        .details(Value::Obj(details))
    }
}
