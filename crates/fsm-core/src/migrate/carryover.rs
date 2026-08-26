//! What an instance holds besides its active state: history, deadlines,
//! pending effects, pending signals, and invocation slots.
//!
//! A seam at this stage — it carries nothing and reschedules nothing — with
//! the four explicit rulings landing in task 5402. It exists now so the
//! migration's seven-step order is written once and never restructured.
//!
//! Plan 0011 task 5401 (seam), 5402 (rules).

use std::collections::BTreeMap;

use crate::machine::{
    ActiveConfiguration, CompiledMachine, InstanceState, Invocation, PendingSignal,
};
use crate::tree::Tree;

/// Everything a migrated instance keeps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Carried {
    pub history: BTreeMap<String, String>,
    pub deadlines: BTreeMap<String, i64>,
    pub pending: Vec<String>,
    pub invocations: BTreeMap<String, Invocation>,
    pub signals: BTreeMap<String, PendingSignal>,
}

/// Carry an instance's holdings onto the new definition.
pub fn carry_over(
    from: &CompiledMachine,
    to: &CompiledMachine,
    tree_to: &Tree,
    st: &InstanceState,
    configuration: &ActiveConfiguration,
    now_ms: i64,
) -> Carried {
    let _ = (from, to, tree_to, st, configuration, now_ms);
    Carried::default()
}
