//! Read-only pending-contract evidence; physical admission remains a service duty.

use super::{CheckStatus, Limits, outcomes};
use crate::{
    config::{HandlerKind, HandlerTable, substitute, substitute_arguments},
    effect::{PendingEffect, resolve},
    error::ExecError,
};
use fsm_core::machine::Status;
use fsm_store::store::Store;
use std::{
    collections::BTreeSet,
    sync::{Mutex, MutexGuard},
};

/// Successful structural evidence only; never a pending-work authorization.
#[derive(Default)]
pub(crate) struct AdmissionCache(
    Mutex<BTreeSet<[u8; 32]>>,
    #[cfg(test)] std::sync::atomic::AtomicUsize,
);

impl AdmissionCache {
    const CAPACITY: usize = 128;

    fn entries(&self) -> MutexGuard<'_, BTreeSet<[u8; 32]>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn remember(&self, key: [u8; 32]) {
        let mut entries = self.entries();
        if entries.len() == Self::CAPACITY && !entries.contains(&key) {
            entries.clear();
        }
        entries.insert(key);
    }
}

fn structural_key(store: &Store, effect: &PendingEffect, table: &HandlerTable) -> Option<[u8; 32]> {
    let mut hash = fsm_core::sha256::Sha256::new();
    let mut field = |value: &str| {
        hash.update(&(value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    };
    field(super::FORMAT);
    field(&effect.emitting_machine_id);
    field(&store.state.instance_machines[&effect.instance_id]);
    // Conservative catalogue identity includes every closure member and missing
    // child availability; unrelated definition changes also invalidate evidence.
    field(&store.state.machines.len().to_string());
    for machine in store.state.machines.values() {
        field(&machine.compiled.machine_id);
    }
    field(&table.handlers.len().to_string());
    for (effect, handler) in &table.handlers {
        field(effect);
        field(&handler.checked_contract().ok()?.0);
    }
    field(&table.manual_effects.len().to_string());
    for effect in &table.manual_effects {
        field(effect);
    }
    field(&table.max_inflight.to_string());
    field(&table.max_inflight_per_instance.to_string());
    Some(hash.finalize())
}

/// Recheck concrete pending work and its current executable definition closure.
/// This never writes, spawns, consumes an attempt or reserves a request key;
/// callers must still validate their writer and generation before dispatch.
pub fn check_pending(
    store: &Store,
    effect: &PendingEffect,
    table: &HandlerTable,
) -> Result<(), ExecError> {
    check_pending_cached(store, effect, table, None)
}

pub(crate) fn check_pending_cached(
    store: &Store,
    effect: &PendingEffect,
    table: &HandlerTable,
    cache: Option<&AdmissionCache>,
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
    let key = cache.and_then(|_| structural_key(store, effect, table));
    let cached = cache
        .zip(key)
        .is_some_and(|(cache, key)| cache.entries().contains(&key));
    if !cached {
        #[cfg(test)]
        if let Some(cache) = cache {
            cache.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let definitions = super::definition_index(
            store
                .state
                .machines
                .values()
                .map(|machine| &machine.compiled),
        );
        let report = outcomes::analyze_resolved(
            &receiver.compiled,
            &|identity| definitions.get(identity).copied(),
            table,
            Limits::default(),
        )?;
        if report.status != CheckStatus::Compatible {
            return Err(refusal(report.status).details(report.to_value()));
        }
        if let Some((cache, key)) = cache.zip(key) {
            cache.remember(key);
        }
    }
    let Some(handler) = table.handlers.get(&effect.effect_name) else {
        if table.manual_effects.contains(&effect.effect_name) {
            // Manual compatibility is operator policy, never spawn permission.
            return Ok(());
        }
        return Err(refusal(CheckStatus::Invalid));
    };
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

/// Recheck service entry against the current closure and the immutable claim.
#[cfg(target_os = "linux")]
pub(crate) fn check_claimed(
    store: &Store,
    claim: &fsm_core::record::execution::Claim,
    table: &HandlerTable,
    cache: Option<&AdmissionCache>,
) -> Result<(), ExecError> {
    let (instance, effect_id) = claim.effect();
    let effect = resolve(store, effect_id).map_err(|_| stale())?;
    if effect.instance_id != instance {
        return Err(stale());
    }
    check_pending_cached(store, &effect, table, cache)?;
    let handler = table.handlers.get(&effect.effect_name).ok_or_else(stale)?;
    let (fingerprint, _) = handler.checked_contract()?;
    if claim.to_value().get("handler_fingerprint") != Some(&fsm_core::json::Value::Str(fingerprint))
    {
        return Err(stale());
    }
    Ok(())
}
fn unknown_definition() -> ExecError {
    ExecError::new(
        "exec/contract_definition_unknown",
        "an emitting or receiving definition is unavailable",
    )
    .hint("restore the verified definition evidence before attempting admission")
}

/// Validate the selected recovered outcome without requiring a pending effect.
pub(crate) fn check_outcome(
    store: &Store,
    instance_id: &str,
    advance: &crate::config::Advance,
) -> Result<(), ExecError> {
    let instance = store.state.instances.get(instance_id).ok_or_else(stale)?;
    // Lifecycle suppression wins over event admission for cancelled/completed work.
    if instance.status != Status::Running {
        return Ok(());
    }
    let receiver = store
        .state
        .instance_machines
        .get(instance_id)
        .and_then(|identity| store.state.machines.get(identity))
        .ok_or_else(unknown_definition)?;
    let (status, _) = outcomes::check(&receiver.compiled, advance);
    if status != CheckStatus::Compatible {
        return Err(refusal(status));
    }
    Ok(())
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

#[cfg(test)]
mod cache_tests {
    use super::*;
    use fsm_core::{
        expr::eval::Val,
        json::{JsonLimits, parse},
    };
    use fsm_store::clock::FixedClock;
    use std::collections::BTreeMap;

    fn fixture() -> (Store, PendingEffect, HandlerTable) {
        let mut store = Store::open_memory().unwrap();
        let mut clock = FixedClock::new(1000, 1);
        let machine = parse(br#"{
          "format":"fsm.machine/1","name":"cache",
          "context":[{"name":"resource","ty":"str","init":"original"}],
          "events":[],"effects":[{"name":"work","fields":[]}],
          "states":[{"name":"ready","entry":{"emit":[{"effect":"work","args":{"resource":"ctx.resource"}}]}}],
          "initial":"ready","transitions":[]
        }"#, &JsonLimits::DEFAULT).unwrap();
        store
            .define_machine_on(&mut clock, machine, false, false)
            .unwrap();
        store
            .create_instance_ctx_on(
                &mut clock,
                "cache",
                "case",
                "create",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        let effect = resolve(&store, &store.state.instances["case"].pending[0]).unwrap();
        let table = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","argv":["/operator/work","{resource}"],"timeout_ms":1000}]}"#).unwrap();
        (store, effect, table)
    }

    #[test]
    fn structural_hit_still_refuses_changed_arguments_and_membership() {
        let (mut store, effect, table) = fixture();
        let cache = AdmissionCache::default();
        check_pending_cached(&store, &effect, &table, Some(&cache)).unwrap();
        let key = structural_key(&store, &effect, &table).unwrap();
        assert!(cache.entries().contains(&key));
        check_pending_cached(&store, &effect, &table, Some(&cache)).unwrap();
        assert_eq!(cache.entries().len(), 1);
        assert_eq!(cache.1.load(std::sync::atomic::Ordering::Relaxed), 1);
        let mut changed = effect.clone();
        changed
            .args
            .insert("resource".into(), Val::Str("replacement".into()));
        assert_eq!(
            check_pending_cached(&store, &changed, &table, Some(&cache))
                .unwrap_err()
                .code,
            "exec/contract_unknown"
        );
        store
            .state
            .instances
            .get_mut("case")
            .unwrap()
            .pending
            .clear();
        assert_eq!(
            check_pending_cached(&store, &effect, &table, Some(&cache))
                .unwrap_err()
                .code,
            "exec/contract_unknown"
        );
    }

    #[test]
    fn private_table_changes_invalidate_and_refusals_are_not_cached() {
        let (store, effect, mut table) = fixture();
        let cache = AdmissionCache::default();
        check_pending_cached(&store, &effect, &table, Some(&cache)).unwrap();
        let original = structural_key(&store, &effect, &table);
        table.handlers.get_mut("work").unwrap().argv[0] = "/private/replacement".into();
        assert_ne!(structural_key(&store, &effect, &table), original);
        check_pending_cached(&store, &effect, &table, Some(&cache)).unwrap();
        assert_eq!(cache.entries().len(), 2);
        table
            .handlers
            .get_mut("work")
            .unwrap()
            .argv
            .push("{absent}".into());
        let invalid = structural_key(&store, &effect, &table).unwrap();
        assert_eq!(
            check_pending_cached(&store, &effect, &table, Some(&cache))
                .unwrap_err()
                .code,
            "exec/contract_invalid"
        );
        assert!(!cache.entries().contains(&invalid));
        assert!(AdmissionCache::default().entries().is_empty());
    }

    #[test]
    fn catalogue_addition_invalidates_successful_structure() {
        let (mut store, effect, table) = fixture();
        let original = structural_key(&store, &effect, &table);
        let machine = parse(br#"{"format":"fsm.machine/1","name":"child","context":[],"events":[],"states":[{"name":"ready"}],"initial":"ready","transitions":[]}"#, &JsonLimits::DEFAULT).unwrap();
        store
            .define_machine_on(&mut FixedClock::new(2000, 1), machine, false, false)
            .unwrap();
        assert_ne!(structural_key(&store, &effect, &table), original);
        let catalogue_key = structural_key(&store, &effect, &table);
        let receiver = store
            .state
            .machines
            .keys()
            .find(|id| id.starts_with("child@"))
            .unwrap()
            .clone();
        store
            .state
            .instance_machines
            .insert(effect.instance_id.clone(), receiver);
        assert_ne!(structural_key(&store, &effect, &table), catalogue_key);
    }

    #[test]
    fn private_mcp_arguments_and_outcome_values_change_cache_identity() {
        let (store, effect, mut table) = fixture();
        table.handlers.get_mut("work").unwrap().kind = HandlerKind::Mcp {
            tool: "perform".into(),
            arguments: parse(br#"{"secret":"original"}"#, &JsonLimits::DEFAULT).unwrap(),
        };
        let original = structural_key(&store, &effect, &table).unwrap();
        if let HandlerKind::Mcp { arguments, .. } =
            &mut table.handlers.get_mut("work").unwrap().kind
        {
            *arguments = parse(br#"{"secret":"replacement"}"#, &JsonLimits::DEFAULT).unwrap();
        }
        assert_ne!(structural_key(&store, &effect, &table).unwrap(), original);
        table.handlers.get_mut("work").unwrap().on_ok = Some(crate::config::Advance {
            event: "done".into(),
            payload: parse(br#"{"secret":"original"}"#, &JsonLimits::DEFAULT).unwrap(),
            stamps: Vec::new(),
        });
        let original = structural_key(&store, &effect, &table).unwrap();
        table
            .handlers
            .get_mut("work")
            .unwrap()
            .on_ok
            .as_mut()
            .unwrap()
            .payload = parse(br#"{"secret":"replacement"}"#, &JsonLimits::DEFAULT).unwrap();
        assert_ne!(structural_key(&store, &effect, &table).unwrap(), original);
    }

    #[test]
    fn cache_bound_is_inclusive_and_evicts_without_refusing_work() {
        let cache = AdmissionCache::default();
        for index in 0..AdmissionCache::CAPACITY {
            let mut key = [0; 32];
            key[..8].copy_from_slice(&(index as u64).to_be_bytes());
            cache.remember(key);
        }
        assert_eq!(cache.entries().len(), AdmissionCache::CAPACITY);
        cache.remember([0; 32]);
        assert_eq!(cache.entries().len(), AdmissionCache::CAPACITY);
        cache.remember([255; 32]);
        assert_eq!(cache.entries().len(), 1);
        assert!(cache.entries().contains(&[255; 32]));
    }
}
