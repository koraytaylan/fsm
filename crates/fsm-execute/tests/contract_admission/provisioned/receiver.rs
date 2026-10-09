//! Receiver migration fixtures preserve historical pending work.

use super::*;

// SPEC's bound-entry rule uses the current receiver while the original emitted
// effect and immutable native claim survive both migrations.
pub(super) fn migrate_receiver(
    store: &mut Store,
    clock: &mut FixedClock,
    instance: &str,
    name: &str,
    invalid: bool,
) {
    let has_child = store.state.machines[&store.state.instance_machines[instance]]
        .compiled
        .spec
        .to_value()
        .get("states")
        .unwrap()
        .as_arr()
        .unwrap()
        .iter()
        .any(|state| state.get("name").and_then(Value::as_str) == Some("closure_probe"));
    let child = has_child.then(|| define_child(store, clock, false));
    migrate(store, clock, instance, name, invalid, child.as_deref());
}

pub(super) fn migrate_invoked_receiver(store: &mut Store, clock: &mut FixedClock, instance: &str) {
    // The child is only invoked from an inactive later state: its incompatible
    // outcome must block the already claimed first operation, before child entry.
    let child = define_child(store, clock, true);
    migrate(
        store,
        clock,
        instance,
        "invoked-receiver",
        false,
        Some(&child),
    );
}

fn define_child(store: &mut Store, clock: &mut FixedClock, invalid: bool) -> String {
    let mut child = parse(br#"{
      "format":"fsm.machine/1","name":"late-incompatible-child",
      "context":[],"events":[],"effects":[{"name":"notify","fields":[]}],
      "states":[{"name":"ready"},{"name":"later","entry":{"emit":[{"effect":"notify","args":{"resource":"\"child\""}}]}}],
      "initial":"ready","transitions":[]
    }"#, &JsonLimits::DEFAULT).unwrap();
    if !invalid {
        let Value::Obj(fields) = &mut child else {
            unreachable!()
        };
        fields.insert("name".into(), Value::Str("late-compatible-child".into()));
        fields.insert(
            "events".into(),
            parse(br#"[{"name":"done","fields":[]}]"#, &JsonLimits::DEFAULT).unwrap(),
        );
    }
    let identity = fsm_core::hashes::machine_id(&child);
    if !store.state.machines.contains_key(&identity) {
        store.define_machine_on(clock, child, false, false).unwrap();
    }
    fsm_core::hashes::digest_of(&identity).unwrap().into()
}

fn migrate(
    store: &mut Store,
    clock: &mut FixedClock,
    instance: &str,
    name: &str,
    invalid: bool,
    child: Option<&str>,
) {
    let identity = &store.state.instance_machines[instance];
    let mut replacement = store.state.machines[identity].compiled.spec.to_value();
    let digest = fsm_core::hashes::digest_of(identity).unwrap();
    let Value::Obj(fields) = &mut replacement else {
        unreachable!()
    };
    fields.insert("name".into(), Value::Str(name.into()));
    let states = fields["states"]
        .as_arr()
        .unwrap()
        .iter()
        .filter(|state| state.get("terminal") != Some(&Value::Bool(true)))
        .map(|state| {
            let name = state.get("name").unwrap().as_str().unwrap();
            (name.into(), Value::Str(name.into()))
        })
        .collect();
    fields.insert(
        "supersedes".into(),
        Value::Obj(BTreeMap::from([
            ("machine".into(), Value::Str(digest.into())),
            ("states".into(), Value::Obj(states)),
            (
                "context".into(),
                Value::Obj(BTreeMap::from([(
                    "resource".into(),
                    Value::Str("\"replacement\"".into()),
                )])),
            ),
        ])),
    );
    let Value::Arr(states) = fields.get_mut("states").unwrap() else {
        unreachable!()
    };
    if let Some(state) = states
        .iter_mut()
        .find(|state| state.get("name").and_then(Value::as_str) == Some("closure_probe"))
    {
        let Value::Obj(state) = state else {
            unreachable!()
        };
        let Value::Arr(invokes) = state.get_mut("invoke").unwrap() else {
            unreachable!()
        };
        let Value::Obj(invoke) = &mut invokes[0] else {
            unreachable!()
        };
        invoke.insert("machine".into(), Value::Str(child.unwrap().into()));
    } else if let Some(child) = child {
        states.push(Value::Obj(BTreeMap::from([
            ("name".into(), Value::Str("closure_probe".into())),
            (
                "invoke".into(),
                Value::Arr(vec![Value::Obj(BTreeMap::from([
                    ("id".into(), Value::Str("late-child".into())),
                    ("machine".into(), Value::Str(child.into())),
                ]))]),
            ),
        ])));
    }
    let events = fields.get_mut("events").unwrap();
    let Value::Arr(events) = events else {
        unreachable!()
    };
    let event = events
        .iter_mut()
        .find(|event| event.get("name").and_then(Value::as_str) == Some("done"))
        .unwrap();
    let Value::Obj(event) = event else {
        unreachable!()
    };
    event.insert(
        "fields".into(),
        if invalid {
            parse(
                br#"[{"name":"approved","ty":"bool"}]"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap()
        } else {
            Value::Arr(Vec::new())
        },
    );
    store
        .define_machine_on(clock, replacement, false, false)
        .unwrap();
    store
        .migrate_instance_on(clock, instance, name, name)
        .unwrap();
    assert_eq!(
        store.state.instances[instance].ctx["resource"],
        fsm_core::expr::eval::Val::Str("replacement".into())
    );
}

#[test]
fn receiver_migration_fixture_preserves_pending_identity_and_repairs_contract() {
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(2000, 0);
    let machine = parse(br#"{
      "format":"fsm.machine/1","name":"migration-fixture","context":[{"name":"resource","ty":"str","init":"historical"}],
      "events":[{"name":"done","fields":[]}],"effects":[{"name":"notify","fields":[]}],
      "states":[{"name":"running","entry":{"emit":[{"effect":"notify","args":{"resource":"ctx.resource"}}]}},{"name":"finished","terminal":true}],
      "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
    }"#, &JsonLimits::DEFAULT).unwrap();
    store
        .define_machine_on(&mut clock, machine, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "migration-fixture",
            "original",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let historical = resolve(&store, &store.state.instances["original"].pending[0]).unwrap();
    assert_eq!(
        historical.args["resource"],
        fsm_core::expr::eval::Val::Str("historical".into())
    );
    let table = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/operator/notify","{resource}"],"timeout_ms":1000,"on_ok":{"event":"done","payload":{}}}]}"#).unwrap();
    check_pending(&store, &historical, &table).unwrap();
    migrate_invoked_receiver(&mut store, &mut clock, "original");
    assert_eq!(resolve(&store, &historical.effect_id).unwrap(), historical);
    let state = store.state.clone();
    let records = store.records.clone();
    for _ in 0..3 {
        assert_eq!(
            check_pending(&store, &historical, &table).unwrap_err().code,
            "exec/contract_invalid"
        );
        assert_eq!(store.records, records);
        assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
    }
    migrate_receiver(
        &mut store,
        &mut clock,
        "original",
        "closure-repaired",
        false,
    );
    assert_eq!(resolve(&store, &historical.effect_id).unwrap(), historical);
    check_pending(&store, &historical, &table).unwrap();
    assert_eq!(
        store.state.instances.len(),
        1,
        "inactive invoke started a child"
    );
    migrate_receiver(&mut store, &mut clock, "original", "invalid-receiver", true);
    assert_eq!(resolve(&store, &historical.effect_id).unwrap(), historical);
    assert_eq!(
        check_pending(&store, &historical, &table).unwrap_err().code,
        "exec/contract_invalid"
    );
    migrate_receiver(
        &mut store,
        &mut clock,
        "original",
        "repaired-receiver",
        false,
    );
    assert_eq!(resolve(&store, &historical.effect_id).unwrap(), historical);
    check_pending(&store, &historical, &table).unwrap();
}
