//! Bound-entry refusal uses real transport, without claiming native authority.

use super::*;
use crate::config::{Advance, HandlerTable};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::clock::FixedClock;

fn bound_owner() -> (Store, Owner, HandlerTable) {
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1000, 1);
    let machine = parse(
        br#"{"format":"fsm.machine/1","name":"entry","context":[],
        "events":[],"effects":[{"name":"notify","fields":[]}],
        "states":[{"name":"ready","entry":{"emit":[{"effect":"notify"}]}}],
        "initial":"ready","transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store
        .define_machine_on(&mut clock, machine, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "entry",
            "entry-instance",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let table = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[
        {"effect":"notify","argv":["/operator/original"],"timeout_ms":1000}
    ]}"#,
    )
    .unwrap();
    let effect = &store.state.instances["entry-instance"].pending[0];
    let fixture = parse(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fsm-core/tests/fixtures/execution-handoff.json"
        )),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let mut claim = fixture.get("claim").unwrap().as_obj().unwrap().clone();
    let (fingerprint, contract) = table.handlers["notify"].checked_contract().unwrap();
    claim.insert("instance_id".into(), Value::Str("entry-instance".into()));
    claim.insert("effect_id".into(), Value::Str(effect.clone()));
    claim.insert("handler_fingerprint".into(), Value::Str(fingerprint));
    claim.insert("retry".into(), contract.get("retry").unwrap().clone());
    let claim = Claim::from_value(&Value::Obj(claim)).unwrap();
    let mut owners = NativeOwners::default();
    owners.retain(&claim, None).unwrap();
    let mut owner = owners.owners.remove(&claim.run_id()).unwrap();
    owner.execution = NativeExecution::bound_transport_fixture(&claim);
    owner.locally_admitted = true;
    owner.entry_requested = false;
    (store, owner, table)
}

fn refuses_bound_entry(change: impl FnOnce(&mut HandlerTable), code: &str) {
    let (mut store, mut owner, mut table) = bound_owner();
    change(&mut table);
    let state = store.state.clone();
    let records = store.records.clone();
    let mut clock = FixedClock::new(2000, 1);
    let error = owner
        .apply(
            &mut store,
            &mut clock,
            &mut Pipeline,
            &AtomicBool::new(false),
            &table,
            None,
        )
        .unwrap_err();
    assert_eq!(error.code, code);
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
    assert_eq!(store.records, records);
    assert!(!owner.entry_requested);
    assert!(owner.execution.progress().retained);
    assert_eq!(owner.execution.progress().phase, NativeRunPhase::Bound);
    // Repair reaches the writer/ownership guard; the metadata fixture still
    // cannot authenticate a durable claim or authorize physical execution.
    let original = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[
        {"effect":"notify","argv":["/operator/original"],"timeout_ms":1000}
    ]}"#,
    )
    .unwrap();
    let repaired = owner
        .apply(
            &mut store,
            &mut clock,
            &mut Pipeline,
            &AtomicBool::new(false),
            &original,
            None,
        )
        .unwrap_err();
    assert_eq!(repaired.code, "exec/mode");
    assert!(!owner.entry_requested);
}

#[test]
fn bound_service_entry_refuses_changed_outcome_before_consuming_permission() {
    refuses_bound_entry(
        |table| {
            table.handlers.get_mut("notify").unwrap().on_ok = Some(Advance {
                event: "undeclared".into(),
                payload: Value::Obj(BTreeMap::new()),
                stamps: Vec::new(),
            });
        },
        "exec/contract_invalid",
    );
}

#[test]
fn bound_service_entry_refuses_changed_private_handler_before_consuming_permission() {
    refuses_bound_entry(
        |table| {
            table.handlers.get_mut("notify").unwrap().argv = vec!["/operator/replacement".into()];
        },
        "exec/contract_unknown",
    );
}

#[test]
fn bound_service_entry_refuses_new_manual_disposition_before_consuming_permission() {
    refuses_bound_entry(
        |table| {
            table.handlers.remove("notify");
            table.manual_effects.insert("notify".into());
        },
        "exec/contract_unknown",
    );
}

#[test]
fn bound_service_entry_refuses_cancelled_or_acknowledged_work_without_consuming_permission() {
    for acknowledged in [false, true] {
        let (mut store, mut owner, table) = bound_owner();
        let mut clock = FixedClock::new(2000, 1);
        if acknowledged {
            let effect = owner.claim.effect().1.to_owned();
            store
                .ack_effect_outcome_on(
                    &mut clock,
                    "entry-instance",
                    &effect,
                    &crate::rid::ack_rid(&effect),
                    "ok",
                    None,
                )
                .unwrap();
        } else {
            store
                .cancel_instance_reason_on(
                    &mut clock,
                    "entry-instance",
                    "cancel",
                    "operator cancelled before entry",
                )
                .unwrap();
        }
        let state = store.state.clone();
        let records = store.records.clone();
        for _ in 0..3 {
            let error = owner
                .apply(
                    &mut store,
                    &mut clock,
                    &mut Pipeline,
                    &AtomicBool::new(false),
                    &table,
                    None,
                )
                .unwrap_err();
            assert_eq!(error.code, "exec/contract_unknown");
            assert_eq!(store.records, records);
            assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
            assert!(!owner.entry_requested);
            assert!(owner.execution.progress().retained);
            assert_eq!(owner.execution.progress().phase, NativeRunPhase::Bound);
        }
    }
}
