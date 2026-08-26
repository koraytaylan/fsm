//! Compiled machine and instance state.

use std::collections::BTreeMap;

use crate::expr::ast::Expr;
use crate::expr::typeck::Ty;
use crate::spec::MachineSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Running,
    Completed,
    Cancelled,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Completed => "completed",
            Status::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforceMode {
    Enforce,
    Monitor,
}

/// The complete active state configuration of an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveConfiguration {
    /// One active leaf in a sequential machine.
    Sequential {
        /// The globally unique name of the active leaf state.
        leaf: String,
    },
    /// One active leaf for every orthogonal region in a parallel machine.
    Parallel {
        /// Region name to active leaf name, ordered lexicographically by region.
        leaves: BTreeMap<String, String>,
    },
}

impl ActiveConfiguration {
    /// Return the sole leaf for a sequential configuration.
    pub fn sequential_leaf(&self) -> Option<&str> {
        match self {
            Self::Sequential { leaf } => Some(leaf),
            Self::Parallel { .. } => None,
        }
    }

    /// Return the sequential leaf, or the named region's leaf for a parallel
    /// configuration.
    pub fn leaf(&self, region: Option<&str>) -> Option<&str> {
        match (self, region) {
            (Self::Sequential { leaf }, None) => Some(leaf),
            (Self::Parallel { leaves }, Some(region)) => leaves.get(region).map(String::as_str),
            _ => None,
        }
    }

    /// Return a copy with exactly one sequential or named regional leaf changed.
    pub fn with_leaf(&self, region: Option<&str>, leaf: String) -> Option<Self> {
        match (self, region) {
            (Self::Sequential { .. }, None) => Some(Self::Sequential { leaf }),
            (Self::Parallel { leaves }, Some(region)) if leaves.contains_key(region) => {
                let mut leaves = leaves.clone();
                leaves.insert(region.to_string(), leaf);
                Some(Self::Parallel { leaves })
            }
            _ => None,
        }
    }
}

/// How far one invocation slot has got.
///
/// The core moves a slot to `Pending` and off the state entirely; only the
/// store, which can perform I/O, moves it through `Running` and `Returned`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokeStatus {
    /// Declared by an entered state; no child exists yet.
    Pending,
    /// The child exists and has not settled.
    Running,
    /// The child settled and its result reached the parent.
    Returned,
}

impl InvokeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            InvokeStatus::Pending => "pending",
            InvokeStatus::Running => "running",
            InvokeStatus::Returned => "returned",
        }
    }
}

/// One invocation slot on the active configuration: which machine to run,
/// with which overrides, and how far it got.
///
/// `overrides` is the slot's `with` projection evaluated once, when the state
/// was entered, so the child sees the values that entry pipeline computed and
/// not whatever the context holds when the store gets round to enacting it.
///
/// The child's instance id is deliberately **not** here. It is
/// [`crate::hashes::child_instance_id`] of the parent's id and the slot key,
/// and the pure core never learns an instance id — `InstanceState` has none,
/// because the id is the caller's handle on the state, not part of it. Every
/// reader that knows the parent's id can derive the child's, and the state
/// hash commits both inputs, so nothing is lost by not storing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub child_machine_id: String,
    pub status: InvokeStatus,
    pub overrides: BTreeMap<String, crate::expr::eval::Val>,
}

/// A child an exiting state left running: the parent stopped waiting, so the
/// store cancels it. The core records the fact and takes no action, because
/// cancelling is I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelledChild {
    pub slot: String,
    pub child_instance_id: String,
}

/// One signal a block emitted and nothing has delivered yet.
///
/// A signal is fire-and-forget: the sender's state carries it until the store
/// delivers it, and delivery does not change the sender except to remove it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSignal {
    pub target_instance_id: String,
    pub event: String,
    pub payload: BTreeMap<String, crate::expr::eval::Val>,
}

/// The complete durable state needed to evaluate an instance deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceState {
    /// Current lifecycle status.
    pub status: Status,
    /// Active leaf, or active leaf per orthogonal region.
    pub configuration: ActiveConfiguration,
    /// Typed context variables.
    pub ctx: BTreeMap<String, crate::expr::eval::Val>,
    /// History-node owner to its remembered state binding.
    pub history: BTreeMap<String, String>,
    /// Active deadline name to absolute caller-time due timestamp in milliseconds.
    pub deadlines: BTreeMap<String, i64>,
    /// Host-owned identifiers for effects that have not been acknowledged.
    pub pending: Vec<String>,
    /// Invocation slots of the active configuration, by slot id.
    pub invocations: BTreeMap<String, Invocation>,
    /// Signals emitted and not yet delivered, by signal id.
    pub signals: BTreeMap<String, PendingSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExprSlot {
    TransitionGuard(usize),
    TransitionSet(usize, usize),
    TransitionEmitArg(usize, usize, String),
    StateEntrySet(String, usize),
    StateExitSet(String, usize),
    StateEntryEmitArg(String, usize, String),
    StateExitEmitArg(String, usize, String),
    /// A `with` field of the indexed raise in the indexed transition's block.
    TransitionRaiseArg(usize, usize, String),
    /// A `with` field of the indexed raise in the named state's entry block.
    StateEntryRaiseArg(String, usize, String),
    /// A `with` field of the indexed raise in the named state's exit block.
    StateExitRaiseArg(String, usize, String),
    /// A `with` field of the indexed invoke slot on the named state.
    InvokeWith(String, usize, String),
    /// The `after` expression of the indexed deadline definition.
    DeadlineAfter(usize),
    /// The indexed context assignment of the indexed deadline definition.
    DeadlineSet(usize, usize),
    /// An effect argument of the indexed emit in the indexed deadline definition.
    DeadlineEmitArg(usize, usize, String),
    /// A `with` field of the indexed raise in the indexed deadline definition.
    DeadlineRaiseArg(usize, usize, String),
    Invariant(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledExpr {
    pub source: String,
    pub ty: Ty,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct CompiledMachine {
    pub machine_id: String,
    pub spec: MachineSpec,
    pub canonical: Vec<u8>,
    pub transitions_by: BTreeMap<(String, String), Vec<usize>>,
    pub compiled_exprs: BTreeMap<ExprSlot, CompiledExpr>,
    pub compile_warnings: Vec<crate::spec::Finding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_state_empty_history_pending() {
        let st = InstanceState {
            status: Status::Running,
            configuration: ActiveConfiguration::Sequential {
                leaf: "intake".into(),
            },
            ctx: BTreeMap::new(),
            history: BTreeMap::new(),
            deadlines: BTreeMap::new(),
            pending: Vec::new(),
            invocations: BTreeMap::new(),
            signals: BTreeMap::new(),
        };
        assert!(st.history.is_empty());
        assert!(st.pending.is_empty());
        assert_eq!(Status::Running.as_str(), "running");
        assert_eq!(Status::Completed.as_str(), "completed");
        assert_eq!(Status::Cancelled.as_str(), "cancelled");
    }
}
