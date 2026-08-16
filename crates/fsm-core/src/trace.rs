//! Decision traces for applied and rejected events.

use std::collections::BTreeMap;

use crate::expr::eval::{TraceNode, trace_to_value};
use crate::expr::lexer::Span;
use crate::json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardTrace {
    Evaluated(TraceNode),
    NotConsidered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateTrace {
    pub transition_idx: u32,
    pub guard: GuardTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelTrace {
    pub source_state: String,
    pub transitions: Vec<CandidateTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Exit(String),
    Transition,
    Entry(String),
}

impl BlockKind {
    pub fn as_label(&self) -> String {
        match self {
            BlockKind::Exit(s) => format!("exit({s})"),
            BlockKind::Transition => "transition".into(),
            BlockKind::Entry(s) => format!("entry({s})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTrace {
    pub target: String,
    pub before: String,
    pub after: String,
    pub expr: TraceNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitTrace {
    pub effect: String,
    pub k: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTrace {
    pub block: BlockKind,
    pub sets: Vec<SetTrace>,
    pub emits: Vec<EmitTrace>,
    pub discarded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantTrace {
    pub name: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecisionTrace {
    pub candidates: Vec<LevelTrace>,
    pub pipeline: Vec<BlockTrace>,
    pub invariants: Vec<InvariantTrace>,
}

impl DecisionTrace {
    pub fn to_value(&self) -> Value {
        let mut obj = BTreeMap::new();
        obj.insert(
            "candidates".into(),
            Value::Arr(self.candidates.iter().map(level_value).collect()),
        );
        obj.insert(
            "pipeline".into(),
            Value::Arr(self.pipeline.iter().map(block_value).collect()),
        );
        obj.insert(
            "invariants".into(),
            Value::Arr(
                self.invariants
                    .iter()
                    .map(|i| {
                        let mut m = BTreeMap::new();
                        m.insert("name".into(), Value::Str(i.name.clone()));
                        m.insert("passed".into(), Value::Bool(i.passed));
                        Value::Obj(m)
                    })
                    .collect(),
            ),
        );
        Value::Obj(obj)
    }
}

fn level_value(l: &LevelTrace) -> Value {
    let mut m = BTreeMap::new();
    m.insert("source_state".into(), Value::Str(l.source_state.clone()));
    m.insert(
        "transitions".into(),
        Value::Arr(
            l.transitions
                .iter()
                .map(|c| {
                    let mut cm = BTreeMap::new();
                    cm.insert(
                        "transition_idx".into(),
                        Value::Num(c.transition_idx.to_string()),
                    );
                    match &c.guard {
                        GuardTrace::NotConsidered => {
                            cm.insert("guard".into(), Value::Str("not_considered".into()));
                        }
                        GuardTrace::Evaluated(t) => {
                            cm.insert("guard".into(), trace_to_value(t));
                        }
                    }
                    Value::Obj(cm)
                })
                .collect(),
        ),
    );
    Value::Obj(m)
}

fn block_value(b: &BlockTrace) -> Value {
    let mut m = BTreeMap::new();
    m.insert("block".into(), Value::Str(b.block.as_label()));
    m.insert("discarded".into(), Value::Bool(b.discarded));
    m.insert(
        "sets".into(),
        Value::Arr(
            b.sets
                .iter()
                .map(|s| {
                    let mut sm = BTreeMap::new();
                    sm.insert("target".into(), Value::Str(s.target.clone()));
                    sm.insert("before".into(), Value::Str(s.before.clone()));
                    sm.insert("after".into(), Value::Str(s.after.clone()));
                    sm.insert("expr".into(), trace_to_value(&s.expr));
                    Value::Obj(sm)
                })
                .collect(),
        ),
    );
    m.insert(
        "emits".into(),
        Value::Arr(
            b.emits
                .iter()
                .map(|e| {
                    let mut em = BTreeMap::new();
                    em.insert("effect".into(), Value::Str(e.effect.clone()));
                    em.insert("k".into(), Value::Num(e.k.to_string()));
                    Value::Obj(em)
                })
                .collect(),
        ),
    );
    Value::Obj(m)
}

pub fn span_none() -> Option<Span> {
    None
}
