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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceState {
    pub status: Status,
    pub leaf: String,
    pub ctx: BTreeMap<String, crate::expr::eval::Val>,
    pub history: BTreeMap<String, String>,
    pub pending: Vec<String>,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_state_empty_history_pending() {
        let st = InstanceState {
            status: Status::Running,
            leaf: "intake".into(),
            ctx: BTreeMap::new(),
            history: BTreeMap::new(),
            pending: Vec::new(),
        };
        assert!(st.history.is_empty());
        assert!(st.pending.is_empty());
        assert_eq!(Status::Running.as_str(), "running");
        assert_eq!(Status::Completed.as_str(), "completed");
        assert_eq!(Status::Cancelled.as_str(), "cancelled");
    }
}
