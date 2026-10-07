//! Read-only pending-contract evidence; physical admission remains a service duty.

use super::{CheckStatus, Limits, outcomes};
use crate::{
    config::{HandlerKind, HandlerTable, substitute, substitute_arguments},
    effect::{PendingEffect, resolve},
    error::ExecError,
};
use fsm_core::{hashes::digest_of, machine::Status};
use fsm_store::store::Store;

/// Recheck concrete pending work and its current executable definition closure.
/// This never writes, spawns, consumes an attempt or reserves a request key;
/// callers must still validate their writer and generation before dispatch.
pub fn check_pending(
    store: &Store,
    effect: &PendingEffect,
    table: &HandlerTable,
) -> Result<(), ExecError> {
    if !store
        .state
        .instances
        .get(&effect.instance_id)
        .is_some_and(|instance| {
            instance.status == Status::Running && instance.pending.contains(&effect.effect_id)
        })
    {
        return Err(stale());
    }
    if !store
        .state
        .machines
        .contains_key(&effect.emitting_machine_id)
    {
        return Err(unknown_definition());
    }
    if resolve(store, &effect.effect_id).map_err(|_| stale())? != *effect {
        return Err(stale());
    }
    let receiver = store
        .state
        .instance_machines
        .get(&effect.instance_id)
        .and_then(|identity| store.state.machines.get(identity))
        .ok_or_else(unknown_definition)?;
    let report =
        outcomes::analyze_resolved(
            &receiver.compiled,
            &|identity| {
                store
                    .state
                    .machines
                    .get(identity)
                    .or_else(|| {
                        store.state.machines.values().find(|machine| {
                            digest_of(&machine.compiled.machine_id) == Some(identity)
                        })
                    })
                    .map(|machine| &machine.compiled)
            },
            table,
            Limits::default(),
        )?;
    if report.status != CheckStatus::Compatible {
        return Err(refusal(report.status).details(report.to_value()));
    }
    let handler = table
        .handlers
        .get(&effect.effect_name)
        .ok_or_else(|| refusal(CheckStatus::Invalid))?;
    substitute(&handler.argv, &effect.args).map_err(|_| refusal(CheckStatus::Invalid))?;
    if let HandlerKind::Mcp { arguments, .. } = &handler.kind {
        substitute_arguments(arguments, &effect.args).map_err(|_| refusal(CheckStatus::Invalid))?;
    }
    for advance in [&handler.on_ok, &handler.on_failed].into_iter().flatten() {
        let (status, _) = outcomes::check(&receiver.compiled, advance);
        if status != CheckStatus::Compatible {
            return Err(refusal(status));
        }
    }
    Ok(())
}

fn stale() -> ExecError {
    ExecError::new(
        "exec/contract_unknown",
        "pending execution evidence is stale or unavailable",
    )
    .hint("observe the original pending effect again before attempting admission")
}
fn unknown_definition() -> ExecError {
    ExecError::new(
        "exec/contract_definition_unknown",
        "an emitting or receiving definition is unavailable",
    )
    .hint("restore the verified definition evidence before attempting admission")
}
fn refusal(status: CheckStatus) -> ExecError {
    let code = if status == CheckStatus::Invalid {
        "exec/contract_invalid"
    } else {
        "exec/contract_unknown"
    };
    ExecError::new(code, "pending execution contract is not proven compatible")
        .hint("repair the machine or operator handler table, then recheck pending work")
}
