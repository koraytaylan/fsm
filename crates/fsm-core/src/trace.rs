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
    /// Action block belonging to the named deadline definition.
    Deadline(String),
    Entry(String),
}

impl BlockKind {
    pub fn as_label(&self) -> String {
        match self {
            BlockKind::Exit(s) => format!("exit({s})"),
            BlockKind::Transition => "transition".into(),
            BlockKind::Deadline(s) => format!("deadline({s})"),
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
    pub expr: Option<TraceNode>,
}

/// One `raise` a block evaluated: the event and its computed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaiseTrace {
    pub event: String,
    /// Field name to canonical value string.
    pub with: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTrace {
    pub block: BlockKind,
    pub sets: Vec<SetTrace>,
    pub emits: Vec<EmitTrace>,
    /// Internal events this block raised; empty for every block of a machine
    /// that raises nothing, and then absent from the rendered trace.
    pub raises: Vec<RaiseTrace>,
    pub discarded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantTrace {
    pub name: String,
    pub passed: bool,
    pub expr: Option<TraceNode>,
    pub error: Option<InvariantEvalError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantEvalError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<(u32, u32)>,
    pub expr: Option<TraceNode>,
}

/// How a reaction microstep was selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicrostepTrigger {
    /// An eventless transition was enabled on the working configuration.
    Eventless,
    /// The named internal event was popped from the macrostep's queue.
    Internal(String),
}

impl MicrostepTrigger {
    /// The `trigger` discriminator a record or trace carries.
    pub fn as_str(&self) -> &'static str {
        match self {
            MicrostepTrigger::Eventless => "eventless",
            MicrostepTrigger::Internal(_) => "internal",
        }
    }
}

/// One reaction microstep of a macrostep.
///
/// The trigger microstep is index 0 and is described by the trace's own
/// `candidates` and `pipeline`; entries here start at index 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrostepTrace {
    /// Position within the macrostep, starting at 1.
    pub index: u32,
    /// What selected this microstep.
    pub trigger: MicrostepTrigger,
    /// State that owned the selected transition.
    pub source_state: String,
    /// Document index of the selected transition.
    pub transition_idx: u32,
    /// Winning region for a parallel machine.
    pub region: Option<String>,
    /// States exited in leaf-to-root execution order.
    pub exited: Vec<String>,
    /// States entered in root-to-leaf execution order.
    pub entered: Vec<String>,
    /// The candidate scan that selected this microstep.
    pub candidates: Vec<LevelTrace>,
    /// This microstep's own block pipeline.
    pub pipeline: Vec<BlockTrace>,
}

/// An internal event the macrostep popped and no transition handled.
///
/// A raised event nobody listens for is a design smell worth surfacing in
/// the audit trail, not noise worth hiding; it is never a rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnhandledInternalTrace {
    /// The discarded event's name.
    pub event: String,
    /// Index of the last microstep applied before the discard; 0 is the
    /// trigger.
    pub after_microstep: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecisionTrace {
    pub candidates: Vec<LevelTrace>,
    pub pipeline: Vec<BlockTrace>,
    pub invariants: Vec<InvariantTrace>,
    /// Reaction microsteps after the trigger, in execution order.
    pub microsteps: Vec<MicrostepTrace>,
    /// Internal events popped during the macrostep that nothing handled.
    pub internal_unhandled: Vec<UnhandledInternalTrace>,
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
            Value::Arr(self.invariants.iter().map(invariant_value).collect()),
        );
        // Absent, never empty: a non-reactive machine's trace must serialize
        // to the bytes it always did, and every trace golden in the
        // workspace depends on that.
        if !self.microsteps.is_empty() {
            obj.insert(
                "microsteps".into(),
                Value::Arr(self.microsteps.iter().map(microstep_value).collect()),
            );
        }
        if !self.internal_unhandled.is_empty() {
            obj.insert(
                "internal_unhandled".into(),
                Value::Arr(
                    self.internal_unhandled
                        .iter()
                        .map(|unhandled| {
                            Value::Obj(BTreeMap::from([
                                ("event".into(), Value::Str(unhandled.event.clone())),
                                (
                                    "after_microstep".into(),
                                    Value::Num(unhandled.after_microstep.to_string()),
                                ),
                            ]))
                        })
                        .collect(),
                ),
            );
        }
        Value::Obj(obj)
    }
}

fn invariant_value(i: &InvariantTrace) -> Value {
    let mut m = BTreeMap::new();
    m.insert("name".into(), Value::Str(i.name.clone()));
    m.insert("passed".into(), Value::Bool(i.passed));
    if let Some(n) = &i.expr {
        m.insert("expr".into(), trace_to_value(n));
    }
    if let Some(err) = &i.error {
        m.insert("error".into(), Value::Str(err.code.into()));
        m.insert("message".into(), Value::Str(err.message.clone()));
        if let Some((s, e)) = err.span {
            let mut sp = BTreeMap::new();
            sp.insert("start".into(), Value::Num(s.to_string()));
            sp.insert("end".into(), Value::Num(e.to_string()));
            m.insert("span".into(), Value::Obj(sp));
        }
    }
    Value::Obj(m)
}

fn microstep_value(microstep: &MicrostepTrace) -> Value {
    let mut m = BTreeMap::new();
    m.insert("index".into(), Value::Num(microstep.index.to_string()));
    m.insert(
        "trigger".into(),
        Value::Str(microstep.trigger.as_str().into()),
    );
    if let MicrostepTrigger::Internal(event) = &microstep.trigger {
        m.insert("event".into(), Value::Str(event.clone()));
    }
    m.insert(
        "source_state".into(),
        Value::Str(microstep.source_state.clone()),
    );
    m.insert(
        "transition_idx".into(),
        Value::Num(microstep.transition_idx.to_string()),
    );
    if let Some(region) = &microstep.region {
        m.insert("region".into(), Value::Str(region.clone()));
    }
    m.insert(
        "exited".into(),
        Value::Arr(microstep.exited.iter().cloned().map(Value::Str).collect()),
    );
    m.insert(
        "entered".into(),
        Value::Arr(microstep.entered.iter().cloned().map(Value::Str).collect()),
    );
    m.insert(
        "candidates".into(),
        Value::Arr(microstep.candidates.iter().map(level_value).collect()),
    );
    m.insert(
        "pipeline".into(),
        Value::Arr(microstep.pipeline.iter().map(block_value).collect()),
    );
    Value::Obj(m)
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
                    if let Some(t) = &e.expr {
                        em.insert("expr".into(), trace_to_value(t));
                    }
                    Value::Obj(em)
                })
                .collect(),
        ),
    );
    if !b.raises.is_empty() {
        m.insert(
            "raises".into(),
            Value::Arr(
                b.raises
                    .iter()
                    .map(|raise| {
                        Value::Obj(BTreeMap::from([
                            ("event".into(), Value::Str(raise.event.clone())),
                            (
                                "with".into(),
                                Value::Obj(
                                    raise
                                        .with
                                        .iter()
                                        .map(|(k, v)| (k.clone(), Value::Str(v.clone())))
                                        .collect(),
                                ),
                            ),
                        ]))
                    })
                    .collect(),
            ),
        );
    }
    Value::Obj(m)
}

pub fn span_none() -> Option<Span> {
    None
}
