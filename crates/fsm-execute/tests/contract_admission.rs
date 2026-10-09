//! Independent pending-contract checks; service dispatch acceptance is separate.
use fsm_core::{
    expr::eval::Val,
    json::{JsonLimits, Value, parse},
};
use fsm_execute::{
    config::HandlerTable,
    contract::check_pending,
    effect::{PendingEffect, resolve},
};
use fsm_store::{clock::FixedClock, store::Store};
use std::collections::BTreeMap;

#[cfg(target_os = "linux")]
#[path = "contract_admission/service.rs"]
mod service;

#[cfg(target_os = "linux")]
#[path = "contract_admission/provisioned.rs"]
mod provisioned;

fn fixture() -> (Store, PendingEffect, HandlerTable) {
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1000, 1);
    let machine = parse(br#"{
      "format":"fsm.machine/1","name":"admission","context":[{"name":"resource","ty":"str","init":"original"}],
      "events":[{"name":"next","fields":[]}],"effects":[{"name":"work","fields":[]},{"name":"restore","fields":[]}],
      "states":[{"name":"first","entry":{"emit":[{"effect":"work","args":{"resource":"ctx.resource"}}]}},
                {"name":"later","entry":{"emit":[{"effect":"restore"}]}}],"initial":"first",
      "transitions":[{"from":"first","on":"next","to":"later"}]
    }"#, &JsonLimits::DEFAULT).unwrap();
    store
        .define_machine_on(&mut clock, machine, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "admission",
            "case-1",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let effect = resolve(&store, &store.state.instances["case-1"].pending[0]).unwrap();
    let table = HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[
      {"effect":"work","argv":["/operator/work","{resource}"],"timeout_ms":1000},
      {"effect":"restore","argv":["/operator/restore"],"timeout_ms":1000}
    ]}"#,
    )
    .unwrap();
    (store, effect, table)
}

#[test]
fn compatible_pending_contract_check_preserves_all_store_state() {
    let (store, effect, table) = fixture();
    let state = store.state.clone();
    let records = store.records.clone();
    check_pending(&store, &effect, &table).unwrap();
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
    assert_eq!(store.records, records);
}

#[test]
fn incompatible_later_restore_blocks_the_pending_contract_until_repaired() {
    let (store, effect, mut table) = fixture();
    let state = store.state.clone();
    let records = store.records.clone();
    table.handlers.get_mut("restore").unwrap().on_ok = Some(fsm_execute::config::Advance {
        event: "undeclared".into(),
        payload: Value::Obj(BTreeMap::new()),
        stamps: Vec::new(),
    });
    let refusal = check_pending(&store, &effect, &table).unwrap_err();
    assert_eq!(refusal.code, "exec/contract_invalid");
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
    assert_eq!(store.records, records);
    table.handlers.get_mut("restore").unwrap().on_ok = None;
    check_pending(&store, &effect, &table).unwrap();
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
}

#[test]
fn pending_contract_check_refuses_substituted_concrete_arguments() {
    let (store, mut effect, table) = fixture();
    let state = store.state.clone();
    effect
        .args
        .insert("resource".into(), Val::Str("substituted".into()));
    assert_eq!(
        check_pending(&store, &effect, &table).unwrap_err().code,
        "exec/contract_unknown"
    );
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
}

#[test]
fn explicitly_manual_pending_effect_remains_compatible_and_unacknowledged() {
    let (store, effect, mut table) = fixture();
    table.handlers.remove("work");
    table.manual_effects.insert("work".into());
    let state = store.state.clone();
    let records = store.records.clone();
    check_pending(&store, &effect, &table).unwrap();
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
    assert_eq!(store.records, records);
    assert!(
        store.state.instances["case-1"]
            .pending
            .contains(&effect.effect_id)
    );
}
