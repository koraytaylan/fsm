//! What an instance holds besides its active state, and what happens to each
//! of the five collections when it moves onto a new definition.
//!
//! Every ruling here is a choice with a cost, and the cost is stated where
//! the rule is. The collections are *destructured* rather than accessed by
//! field, so a sixth collection added by a later plan cannot ship without a
//! ruling: the compiler refuses until somebody writes one.
//!
//! Plan 0011 task 5402.

// A `Rejection` carries a decision trace by design, and this function takes
// the machine, the tree, the instance, both mappings, the timestamp, and the
// budget — every one of which it needs. `step` sets the same allowances for
// the same reasons.
#![allow(clippy::result_large_err, clippy::too_many_arguments)]

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::machine::{
    ActiveConfiguration, CompiledMachine, InstanceState, Invocation, InvokeStatus, PendingSignal,
};
use crate::step::Rejection;
use crate::tree::Tree;

/// Everything a migrated instance keeps, and what it lost on the way.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Carried {
    pub history: BTreeMap<String, String>,
    pub deadlines: BTreeMap<String, i64>,
    pub pending: Vec<String>,
    pub invocations: BTreeMap<String, Invocation>,
    pub signals: BTreeMap<String, PendingSignal>,
    /// History bindings whose owner or child the mapping does not cover.
    pub dropped_history: Vec<String>,
    /// Deadline name, the due time it had, and the due time it now has.
    /// `None` on either side means it did not exist then or does not now.
    pub rescheduled_deadlines: Vec<(String, Option<i64>, Option<i64>)>,
    /// Effect ids carried verbatim.
    pub retained_effects: Vec<String>,
    /// Signal ids carried verbatim.
    pub retained_signals: Vec<String>,
    /// Slots dropped because their result was already delivered.
    pub dropped_slots: Vec<String>,
}

/// Carry an instance's holdings onto the new definition.
pub fn carry_over(
    to: &CompiledMachine,
    tree_to: &Tree,
    st: &InstanceState,
    states: &BTreeMap<&str, &str>,
    configuration: &ActiveConfiguration,
    ctx: &BTreeMap<String, crate::expr::eval::Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<Carried, Rejection> {
    // Exhaustive by construction: a field added to `InstanceState` fails to
    // compile here until it gets a ruling of its own.
    let InstanceState {
        status: _,
        configuration: _,
        ctx: _,
        history,
        deadlines,
        pending,
        invocations,
        signals,
    } = st;

    let mut carried = Carried::default();

    // History — remap, drop on miss. A history binding concerns a state the
    // instance is *not* currently in, so losing one degrades a future
    // re-entry rather than corrupting the present; refusing a whole migration
    // over that is disproportionate. Every drop is listed.
    for (owner, child) in history {
        match (states.get(owner.as_str()), states.get(child.as_str())) {
            (Some(new_owner), Some(new_child)) => {
                carried
                    .history
                    .insert((*new_owner).to_string(), (*new_child).to_string());
            }
            _ => carried.dropped_history.push(format!("{owner}/{child}")),
        }
    }

    // Deadlines — recompute, never carry. Carrying an old due time would keep
    // a promise the new definition never made, so every schedule is dropped
    // and the new machine's are computed from this migration's `now_ms`.
    // **Migration restarts the clock on every timer**, which is the one
    // carry-over consequence an operator must see.
    carried.deadlines = crate::step::schedule_for(to, tree_to, configuration, ctx, now_ms, budget)?;
    let mut names: Vec<&String> = deadlines.keys().collect();
    names.extend(carried.deadlines.keys());
    names.sort();
    names.dedup();
    for name in names {
        let (before, after) = (deadlines.get(name), carried.deadlines.get(name));
        if before != after {
            carried
                .rescheduled_deadlines
                .push((name.clone(), before.copied(), after.copied()));
        }
    }

    // Pending effects — retain verbatim. An effect id names the record that
    // emitted it, and that record's machine is still in the catalogue, so the
    // name and arguments still re-derive. Dropping one would strand a handler
    // that is already running against the outside world.
    carried.pending = pending.clone();
    carried.retained_effects = pending.clone();

    // Invocation slots — carry or refuse. A `Running` child is a live
    // instance doing work: it cannot be dropped the way a history binding
    // can. A `Returned` slot whose id is gone *is* dropped, since its result
    // was already delivered.
    let declared: BTreeMap<&str, &str> = to
        .spec
        .walk_states()
        .into_iter()
        .flat_map(|(node, _)| {
            node.invokes
                .iter()
                .map(|invoke| (invoke.id.as_str(), invoke.machine.as_str()))
        })
        .collect();
    for (slot, invocation) in invocations {
        let same = declared.get(slot.as_str()).is_some_and(|machine| {
            crate::hashes::digest_of(&invocation.child_machine_id) == Some(*machine)
                || invocation.child_machine_id == **machine
        });
        if same {
            carried.invocations.insert(slot.clone(), invocation.clone());
            continue;
        }
        if invocation.status == InvokeStatus::Returned {
            carried.dropped_slots.push(slot.clone());
            continue;
        }
        return Err(Rejection {
            code: "req/migrate_slot",
            message: format!(
                "slot {slot} is {} and the new definition does not declare it the same way",
                invocation.status.as_str()
            ),
            hint: "declare the slot with the same child machine in the new definition, or let \
                   the invocation return before migrating"
                .into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
            cause: None,
        });
    }

    // Pending signals — retain verbatim. A signal names a target instance and
    // an event the *target's* machine declares, so the sender's own
    // definition cannot invalidate one: neither mapping has any bearing on
    // deliverability, which is decided at delivery.
    carried.signals = signals.clone();
    carried.retained_signals = signals.keys().cloned().collect();

    Ok(carried)
}
