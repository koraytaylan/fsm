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
