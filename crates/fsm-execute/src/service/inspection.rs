//! Journal-derived unresolved ownership; no native requests or handler disclosure.

use std::collections::BTreeMap;

use fsm_core::{json::Value, record::execution::NativeDomain};
use fsm_store::store::Store;

/// Sanitized unresolved runs from one verified journal observation, in run order.
/// Native liveness is deliberately unverified; a stopped record is durable state.
pub fn inspect_runs(store: &Store) -> Value {
    let mut entries: Vec<_> = store.state.execution.unresolved().collect();
    entries.sort_by_key(|(claim, _)| claim.run_id());
    let runs = entries
        .into_iter()
        .map(|(claim, stopped)| {
            let (instance, effect) = claim.effect();
            Value::Obj(BTreeMap::from([
                ("run_id".into(), Value::Num(claim.run_id().to_string())),
                ("instance_id".into(), Value::Str(instance.into())),
                ("effect_id".into(), Value::Str(effect.into())),
                ("backend".into(), Value::Str(NativeDomain::BACKEND.into())),
                (
                    "phase".into(),
                    Value::Str(if stopped.is_some() { "stopped" } else { "unresolved" }.into()),
                ),
                ("native_evidence".into(), Value::Str("unverified".into())),
                (
                    "next".into(),
                    Value::Str(
                        if stopped.is_some() {
                            "Recover the original authenticated result under the original writer; do not rerun the handler."
                        } else {
                            "Verify the original owner and native closure before settlement; absence or elapsed time cannot clear this claim."
                        }
                        .into(),
                    ),
                ),
            ]))
        })
        .collect();
    Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.execution-runs/1".into())),
        (
            "observed_seq".into(),
            Value::Num(store.journal.last_seq.to_string()),
        ),
        ("inventory_complete".into(), Value::Bool(true)),
        ("inventory_limit".into(), Value::Num("4096".into())),
        ("runs".into(), Value::Arr(runs)),
    ]))
}
