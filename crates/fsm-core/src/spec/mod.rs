//! `fsm.machine/1` parse, structural validation, and expression binding.

#![allow(
    clippy::collapsible_if,
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::ptr_arg
)]

use std::collections::BTreeMap;

use crate::expr::lexer::Span;
use crate::expr::typeck::Ty;
use crate::json::Value;
use crate::machine::EnforceMode;

mod compat;
mod compile;
mod machine_impl;
mod parse;
mod serialize;
mod validate;

pub use compat::{
    accepted_identity, compile_accepted, compile_accepted_historical_unchecked, load_machine_json,
};
pub use compile::compile;
pub use parse::parse_machine;
pub use validate::validate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub path: String,
    pub span: Option<Span>,
    pub hint: String,
}

impl Finding {
    pub fn err(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            path: path.into(),
            span: None,
            hint: hint.into(),
        }
    }

    pub fn warn(
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            path: path.into(),
            span: None,
            hint: hint.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryKind {
    Shallow,
    Deep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TySpec {
    Int,
    Dec { scale: u8 },
    Str,
    Bool,
    Enum { of: String },
    Ts,
    Dur,
}

impl TySpec {
    pub fn to_ty(&self) -> Ty {
        match self {
            TySpec::Int => Ty::Int,
            TySpec::Dec { scale } => Ty::Dec(*scale),
            TySpec::Str => Ty::Str,
            TySpec::Bool => Ty::Bool,
            TySpec::Enum { of } => Ty::Enum(of.clone()),
            TySpec::Ts => Ty::Ts,
            TySpec::Dur => Ty::Dur,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtxVar {
    pub name: String,
    pub ty: TySpec,
    pub init: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TySpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    /// Only the machine may raise it: the external send path refuses it
    /// with `req/event_internal`, and `enabled_events` never lists it.
    pub internal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetSpec {
    pub target: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitSpec {
    pub effect: String,
    pub args: BTreeMap<String, String>,
}

/// A `raise` in a block: an internal event delivered to this instance inside
/// the same macrostep, with its typed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaiseSpec {
    pub event: String,
    /// Declared field name to `expr/1` source, in field-name order.
    pub with: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub sets: Vec<SetSpec>,
    pub emits: Vec<EmitSpec>,
    pub raises: Vec<RaiseSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateNode {
    pub name: String,
    pub terminal: bool,
    pub history: Option<HistoryKind>,
    pub initial: Option<String>,
    pub entry: Option<Block>,
    pub exit: Option<Block>,
    pub states: Vec<StateNode>,
}

/// One top-level orthogonal region in a parallel machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionSpec {
    /// Globally unique region name.
    pub name: String,
    /// This region's independent state hierarchy.
    pub states: Vec<StateNode>,
    /// Top-level state entered when this region starts.
    pub initial: String,
}

/// The mutually exclusive single-region and parallel definition forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Topology {
    /// A conventional machine with exactly one active leaf.
    Sequential {
        /// The machine's single state hierarchy.
        states: Vec<StateNode>,
        /// Top-level state entered when an instance starts.
        initial: String,
    },
    /// A machine with one simultaneously active leaf in each region.
    Parallel {
        /// Orthogonal regions in semantic document order.
        regions: Vec<RegionSpec>,
    },
}

/// Total states in the tree, nested ones included.
///
/// Every summary reports this. It lives here because hand-rolled copies drifted
/// once already: `fsm machine ls` counted only top-level children and reported
/// 4 states for a machine `fsm validate` reported 9 for.
pub fn count_states(nodes: &[StateNode]) -> usize {
    nodes.iter().map(|n| 1 + count_states(&n.states)).sum()
}

/// Terminal states anywhere in the tree, in document order.
pub fn terminal_states(nodes: &[StateNode]) -> Vec<&str> {
    let mut out = Vec::new();
    fn walk<'a>(nodes: &'a [StateNode], out: &mut Vec<&'a str>) {
        for n in nodes {
            if n.terminal {
                out.push(n.name.as_str());
            }
            walk(&n.states, out);
        }
    }
    walk(nodes, &mut out);
    out
}

/// Cell key under which an eventless transition is compiled and selected.
///
/// `def/reserved_ident` forbids `$`-prefixed declared names, so no user event
/// can collide with it; the sentinel never appears in a definition document
/// and never reaches a journal record — an omitted `on` is the whole syntax.
pub const ALWAYS_KEY: &str = "$always";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionSpec {
    pub from: String,
    /// Triggering event, or `None` for an eventless transition, which the
    /// macrostep runs whenever its guard holds on the working configuration.
    pub on: Option<String>,
    pub guard: Option<String>,
    pub sets: Vec<SetSpec>,
    pub emits: Vec<EmitSpec>,
    pub raises: Vec<RaiseSpec>,
    pub to: Option<String>,
}

impl TransitionSpec {
    /// Whether this transition fires without an event.
    pub fn is_eventless(&self) -> bool {
        self.on.is_none()
    }

    /// The `(from, on)` cell key: the event name, or [`ALWAYS_KEY`].
    pub fn cell_key(&self) -> &str {
        self.on.as_deref().unwrap_or(ALWAYS_KEY)
    }
}

/// A timed transition scheduled whenever its source state is entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineSpec {
    /// Globally unique deadline name.
    pub name: String,
    /// Source state whose entry schedules the deadline.
    pub from: String,
    /// Context-only duration expression evaluated on source entry.
    pub after: String,
    /// Context assignments applied when the deadline fires.
    pub sets: Vec<SetSpec>,
    /// Effects emitted when the deadline fires.
    pub emits: Vec<EmitSpec>,
    /// Internal events raised when the deadline fires.
    pub raises: Vec<RaiseSpec>,
    /// Target state in the same region as `from`.
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantSpec {
    pub name: String,
    pub expr: String,
    pub mode: EnforceMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSpec {
    pub format: String,
    pub name: String,
    pub description: Option<String>,
    pub enums: BTreeMap<String, Vec<String>>,
    pub context: Vec<CtxVar>,
    pub events: Vec<EventDecl>,
    pub effects: Vec<EffectDecl>,
    /// Sequential or orthogonal-region state topology.
    pub topology: Topology,
    /// Timed transitions in deterministic document order.
    pub deadlines: Vec<DeadlineSpec>,
    pub on_unhandled: Unhandled,
    pub transitions: Vec<TransitionSpec>,
    pub invariants: Vec<InvariantSpec>,
    /// Accepted document when parsed; not part of the public mutation surface.
    pub(crate) source: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unhandled {
    Reject,
    Ignore,
}
