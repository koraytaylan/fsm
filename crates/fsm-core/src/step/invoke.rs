//! The invocation outbox: the pure core records that a child should exist.
//!
//! The core performs no I/O and cannot create an instance, so it does for
//! invocations exactly what it already does for effects — it writes the
//! intent into the state and lets the shell enact it. Plan 0010 task 4802.

use std::collections::BTreeMap;

use crate::expr::eval::{Bindings, Budget, Val, eval};
use crate::expr::typeck::{ScopeKind, Ty};
use crate::machine::{CancelledChild, CompiledMachine, ExprSlot, Invocation, InvokeStatus};
use crate::trace::BlockKind;
use crate::tree::Tree;

use super::Rejection;
use super::block::action_err;
use super::block::find_node;
use super::guard::compiled_or_annotate;

/// Apply one microstep's exit and entry sets to the invocation slots.
///
/// Slots of exited states go entirely, at whatever status they held; a slot
/// that was `Running` is additionally reported as a child the store must
/// cancel, because the parent has stopped waiting for it. Slots of entered
/// states are inserted as `Pending`, with their `with` projection evaluated
/// against the context the pipeline produced — once, here, so the child sees
/// what the entry blocks computed rather than whatever the context holds when
/// the store gets round to enacting it.
///
/// A state entered and exited inside one macrostep therefore leaves nothing
/// behind: the later microstep's exit removes what the earlier one inserted.
pub(super) fn settle_invocations(
    machine: &CompiledMachine,
    tree: &Tree,
    prior: &BTreeMap<String, Invocation>,
    exited: &[u16],
    entered: &[u16],
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<(BTreeMap<String, Invocation>, Vec<CancelledChild>), Rejection> {
    let mut invocations = prior.clone();
    let mut cancelled = Vec::new();
    for &id in exited {
        let name = &tree.names[id as usize];
        let Some(node) = find_node(&machine.spec, name) else {
            continue;
        };
        for invoke in &node.invokes {
            if let Some(gone) = invocations.remove(&invoke.id) {
                if gone.status == InvokeStatus::Running {
                    cancelled.push(CancelledChild {
                        slot: invoke.id.clone(),
                        child_instance_id: String::new(),
                    });
                }
            }
        }
    }
    let bindings = Bindings {
        ctx,
        evt: None,
        active: None,
    };
    let ctx_tys: BTreeMap<String, Ty> = machine
        .spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let state_names = machine.spec.state_names();
    for &id in entered {
        let name = &tree.names[id as usize];
        let Some(node) = find_node(&machine.spec, name) else {
            continue;
        };
        for (index, invoke) in node.invokes.iter().enumerate() {
            let kind = BlockKind::Entry(name.clone());
            let mut overrides = BTreeMap::new();
            for (field, source) in &invoke.with {
                let expression = compiled_or_annotate(
                    source,
                    &ExprSlot::InvokeWith(name.clone(), index, field.clone()),
                    &machine.compiled_exprs,
                    &machine.spec,
                    ScopeKind::Block,
                    &ctx_tys,
                    None,
                    &state_names,
                    &kind,
                )?;
                match eval(&expression, &bindings, budget, false).0 {
                    Ok(value) => {
                        overrides.insert(field.clone(), value);
                    }
                    // The same shape a failing `do` takes: the public code is
                    // the wrapper, the evaluator's code is the cause.
                    Err(err) => {
                        let mut rejection = action_err(&kind, err.message, err.hint);
                        rejection.span = Some((err.span.start, err.span.end));
                        rejection.cause = Some(err.code);
                        return Err(rejection);
                    }
                }
            }
            invocations.insert(
                invoke.id.clone(),
                Invocation {
                    child_machine_id: invoke.machine.clone(),
                    status: InvokeStatus::Pending,
                    overrides,
                },
            );
        }
    }
    Ok((invocations, cancelled))
}
