//! Genuine marker acceptance; executed only by the protected native coordinator.

use super::*;

#[path = "provisioned/cancellation.rs"]
mod cancellation;
#[path = "provisioned/receiver.rs"]
mod receiver;
#[path = "provisioned/retry.rs"]
mod retry;
#[path = "provisioned/sensitivity.rs"]
mod sensitivity;
#[path = "provisioned/settlement.rs"]
mod settlement;
use fsm_execute::{
    config::Advance,
    run::{Pipeline, Runner},
    sched::Scheduler,
    service::{tick_reporting, tick_with},
    watch::Watcher,
};
use receiver::migrate_receiver;
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_refusal_preserves_work_and_repair_starts_original_handler() {
    observe(false, Scenario::Repair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_refusal_preserves_work_and_repair_starts_original_handler() {
    observe(true, Scenario::Repair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_incompatible_machine_cannot_starve_compatible_work() {
    observe(false, Scenario::Fairness);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_incompatible_machine_cannot_starve_compatible_work() {
    observe(true, Scenario::Fairness);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_unknown_outcome_preserves_work_until_repair() {
    observe(false, Scenario::UnknownRepair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_unknown_outcome_preserves_work_until_repair() {
    observe(true, Scenario::UnknownRepair);
}

#[derive(Clone, Copy)]
enum Scenario {
    Repair,
    MissingArgumentRepair,
    UnknownRepair,
    Fairness,
    Contention,
    ManualRepair,
    AckOnly,
    Recovery,
    BoundRecheck,
    BoundCancel,
    GuardStructure,
    GuardBound,
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_cancellation_retires_bound_claim_without_entry() {
    observe(false, Scenario::BoundCancel);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_cancellation_retires_bound_claim_without_entry() {
    observe(true, Scenario::BoundCancel);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_missing_argument_preserves_work_until_repair() {
    observe(false, Scenario::MissingArgumentRepair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_missing_argument_preserves_work_until_repair() {
    observe(true, Scenario::MissingArgumentRepair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_timeout_reaps_original_tree_while_writer_is_held() {
    observe(false, Scenario::Contention);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_timeout_reaps_original_tree_while_writer_is_held() {
    observe(true, Scenario::Contention);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_manual_work_stays_pending_until_handler_is_supplied() {
    observe(false, Scenario::ManualRepair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_no_outcome_handler_acks_without_synthetic_event() {
    observe(false, Scenario::AckOnly);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_manual_work_stays_pending_until_handler_is_supplied() {
    observe(true, Scenario::ManualRepair);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_no_outcome_handler_acks_without_synthetic_event() {
    observe(true, Scenario::AckOnly);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_acknowledged_recovery_repairs_without_handler_restart() {
    observe(false, Scenario::Recovery);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_acknowledged_recovery_repairs_without_handler_restart() {
    observe(true, Scenario::Recovery);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_bound_entry_refuses_stale_table_until_original_is_restored() {
    observe(false, Scenario::BoundRecheck);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_bound_entry_refuses_stale_table_until_original_is_restored() {
    observe(true, Scenario::BoundRecheck);
}

fn observe(borrowed: bool, scenario: Scenario) {
    let manifest = manifest();
    let store_path = PathBuf::from(manifest.get("store").unwrap().as_str().unwrap());
    let resource = PathBuf::from(manifest.get("resource").unwrap().as_str().unwrap());
    if matches!(scenario, Scenario::Contention) {
        assert!(
            fs::read(resource.join("root-candidate"))
                .unwrap()
                .is_empty()
        );
    } else {
        assert!(!resource.join("root-candidate").exists());
    }
    assert!(!resource.join("root-release").exists());
    let mut table =
        HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
            .unwrap();
    table.max_inflight = 1;
    table.max_inflight_per_instance = 1;
    let original_notify = table.handlers["notify"].clone();
    let mut restore = original_notify.clone();
    restore.effect = "restore".into();
    restore.on_ok = Some(Advance {
        event: "undeclared".into(),
        payload: Value::Obj(BTreeMap::new()),
        stamps: Vec::new(),
    });
    if matches!(scenario, Scenario::UnknownRepair) {
        restore.on_ok = Some(Advance {
            event: "stamped".into(),
            payload: Value::Obj(BTreeMap::new()),
            stamps: vec!["at".into()],
        });
    }
    if matches!(scenario, Scenario::MissingArgumentRepair) {
        restore.on_ok = None;
        table
            .handlers
            .get_mut("notify")
            .unwrap()
            .argv
            .push("{absent}".into());
    }
    if matches!(scenario, Scenario::Contention) {
        restore.on_ok = None;
        table.max_inflight = 2;
        let notify = table.handlers.get_mut("notify").unwrap();
        notify.timeout_ms = 5000;
        notify.retry.attempts = 2;
        notify.on_failed = notify.on_ok.clone();
    }
    if matches!(
        scenario,
        Scenario::ManualRepair
            | Scenario::AckOnly
            | Scenario::Recovery
            | Scenario::BoundRecheck
            | Scenario::BoundCancel
            | Scenario::GuardBound
    ) {
        restore.on_ok = None;
    }
    if matches!(scenario, Scenario::ManualRepair) {
        table.handlers.remove("notify");
        table.manual_effects.insert("notify".into());
    }
    if matches!(scenario, Scenario::AckOnly) {
        let notify = table.handlers.get_mut("notify").unwrap();
        notify.on_ok = None;
        notify.on_failed = None;
        notify.retry.attempts = 1;
    }
    if matches!(scenario, Scenario::Recovery) {
        table.handlers.get_mut("notify").unwrap().on_ok = Some(Advance {
            event: "undeclared".into(),
            payload: Value::Obj(BTreeMap::new()),
            stamps: Vec::new(),
        });
    }
    table.handlers.insert("restore".into(), restore);
    let mut clock = FixedClock::new(2000, 0);
    let mut store = Store::open(&store_path).unwrap();
    store
        .define_machine_on(
            &mut clock,
            parse(
                br#"{
      "format":"fsm.machine/1","name":"native-admission","context":[{"name":"resource","ty":"str","init":"historical"}],
      "enums":{"StampRange":["0","2000"]},
      "events":[{"name":"done","fields":[]},{"name":"stamped","fields":[{"name":"at","ty":{"enum":"StampRange"}}]}],
      "effects":[{"name":"notify","fields":[]},{"name":"restore","fields":[]}],
      "states":[{"name":"running","entry":{"emit":[{"effect":"notify","args":{"resource":"ctx.resource"}}]}},
                {"name":"compensating","entry":{"emit":[{"effect":"restore"}]}},
                {"name":"finished","terminal":true}],
      "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
    }"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
            false,
            false,
        )
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "native-admission",
            "original",
            "create-original",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let effect_id = store.state.instances["original"].pending[0].clone();
    if matches!(scenario, Scenario::Recovery) {
        store
            .ack_effect_outcome_on(
                &mut clock,
                "original",
                &effect_id,
                &fsm_execute::rid::ack_rid(&effect_id),
                "ok",
                None,
            )
            .unwrap();
    }
    let records = store.records.clone();
    let state = store.state.clone();
    drop(store);
    let mut watcher = Watcher::with_handlers(store_path.clone(), &table);
    let mut scheduler = Scheduler::new(table.clone());
    let mut runner = Runner::new_native().unwrap();
    let tick = |watcher: &mut Watcher,
                scheduler: &mut Scheduler,
                runner: &mut Runner,
                clock: &mut FixedClock| {
        let now = fsm_store::clock::Clock::reserve_ms(clock);
        if borrowed {
            let mut store = Store::open(&store_path).unwrap();
            tick_with(
                watcher,
                scheduler,
                runner,
                &mut Pipeline,
                &mut store,
                clock,
                now,
            )
        } else {
            let outcome = tick_reporting(
                watcher,
                scheduler,
                runner,
                &mut Pipeline,
                &store_path,
                clock,
                now,
            );
            assert!(!outcome.writer_unavailable);
            outcome.lines
        }
    };
    let guard_fixture = sensitivity::Fixture::new(&store_path, &resource, scenario);
    if sensitivity::observe(
        guard_fixture,
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut clock,
        tick,
    ) {
        return;
    }
    let diagnostic = if matches!(scenario, Scenario::UnknownRepair) {
        "error exec/contract_unknown"
    } else {
        "error exec/contract_invalid"
    };
    let mut manual_stalls = 0;
    for _ in 0..if matches!(
        scenario,
        Scenario::Contention | Scenario::AckOnly | Scenario::BoundRecheck | Scenario::BoundCancel
    ) {
        0
    } else {
        3
    } {
        let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        if matches!(scenario, Scenario::ManualRepair) {
            let stall = format!("error exec/unhandled_effect {effect_id}");
            manual_stalls += lines.iter().filter(|line| *line == &stall).count();
            assert!(
                lines
                    .iter()
                    .all(|line| !line.starts_with("error ") || line == &stall),
                "{lines:?}"
            );
        } else {
            assert!(lines.iter().any(|line| line == diagnostic), "{lines:?}");
        }
        assert!(
            !resource.join("root-candidate").exists(),
            "incompatible later step started the first real handler"
        );
        let current = Store::open_read_only(&store_path).unwrap();
        assert_eq!(current.records, records);
        assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
        assert!(scheduler.inflight_effect(&effect_id).is_none());
    }
    if matches!(scenario, Scenario::ManualRepair) {
        // EMBEDDING: absent automatic handlers report one deliberate stall;
        // explicit manual policy supplies compatibility, never spawn permission.
        assert_eq!(manual_stalls, 1);
    }
    let completed_instance = match scenario {
        Scenario::GuardStructure | Scenario::GuardBound => unreachable!("guard probe returned"),
        Scenario::Contention
        | Scenario::AckOnly
        | Scenario::BoundRecheck
        | Scenario::BoundCancel => "original",
        Scenario::Recovery => {
            table.handlers.insert("notify".into(), original_notify);
            scheduler = Scheduler::new(table.clone());
            watcher = Watcher::with_handlers(store_path.clone(), &table);
            "original"
        }
        Scenario::ManualRepair => {
            assert!(table.manual_effects.remove("notify"));
            table.handlers.insert("notify".into(), original_notify);
            scheduler = Scheduler::new(table.clone());
            watcher = Watcher::with_handlers(store_path.clone(), &table);
            "original"
        }
        Scenario::MissingArgumentRepair => {
            assert_eq!(
                table
                    .handlers
                    .get_mut("notify")
                    .unwrap()
                    .argv
                    .pop()
                    .as_deref(),
                Some("{absent}")
            );
            scheduler = Scheduler::new(table.clone());
            watcher = Watcher::with_handlers(store_path.clone(), &table);
            "original"
        }
        Scenario::Repair | Scenario::UnknownRepair => {
            table.handlers.get_mut("restore").unwrap().on_ok = None;
            scheduler = Scheduler::new(table.clone());
            watcher = Watcher::with_handlers(store_path.clone(), &table);
            "original"
        }
        Scenario::Fairness => {
            let mut store = Store::open(&store_path).unwrap();
            store
                .define_machine_on(
                    &mut clock,
                    parse(
                        br#"{
              "format":"fsm.machine/1","name":"compatible-admission","context":[],
              "events":[{"name":"done","fields":[]}],"effects":[{"name":"notify","fields":[]}],
              "states":[{"name":"running","entry":{"emit":[{"effect":"notify"}]}},
                        {"name":"finished","terminal":true}],
              "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
            }"#,
                        &JsonLimits::DEFAULT,
                    )
                    .unwrap(),
                    false,
                    false,
                )
                .unwrap();
            store
                .create_instance_ctx_on(
                    &mut clock,
                    "compatible-admission",
                    "z-compatible",
                    "create-compatible",
                    None,
                    &BTreeMap::new(),
                    &[],
                )
                .unwrap();
            "z-compatible"
        }
    };
    if matches!(scenario, Scenario::Recovery) {
        let derived = fsm_execute::rid::event_rid(&effect_id, "done");
        assert!(!state.dedup.contains_key(&derived));
        tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        let current = Store::open_read_only(&store_path).unwrap();
        assert_eq!(
            current.state.instances["original"].status,
            fsm_core::machine::Status::Completed
        );
        assert!(current.state.dedup.contains_key(&derived));
        for (kind, expected) in [
            (fsm_core::record::RecordKind::EffectAcked, 1),
            (fsm_core::record::RecordKind::EventApplied, 1),
            (fsm_core::record::RecordKind::ExecutionClaimed, 0),
        ] {
            assert_eq!(
                current
                    .records
                    .iter()
                    .filter(|record| record.kind == kind)
                    .count(),
                expected
            );
        }
        let recovered = current.records.clone();
        drop(current);
        for _ in 0..3 {
            tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
            assert_eq!(
                Store::open_read_only(&store_path).unwrap().records,
                recovered
            );
            assert!(scheduler.inflight_effect(&effect_id).is_none());
            assert!(runner.local_native_claims().next().is_none());
            assert_no_entry(&resource);
        }
        assert_eq!(
            fsm_store::journal_io::verify(&store_path).health,
            fsm_store::journal_io::JournalHealth::Ok
        );
        return;
    }
    let deadline = Instant::now()
        + Duration::from_secs(if matches!(scenario, Scenario::Contention) {
            30
        } else {
            20
        });
    if matches!(scenario, Scenario::BoundCancel) {
        cancellation::observe(
            cancellation::Fixture {
                store_path: &store_path,
                resource: &resource,
            },
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut clock,
            tick,
        );
        return;
    }
    if matches!(scenario, Scenario::BoundRecheck) {
        // A claim tick cannot also authorize entry; the next decision must
        // validate the actual table again under the original healthy writer.
        while runner.local_native_claims().next().is_none() {
            tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
            assert_no_entry(&resource);
            assert!(
                Instant::now() < deadline,
                "native claim was never published"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let original_claim = runner.local_native_claims().next().unwrap().clone();
        let original_table = table.clone();
        let competing = Store::open(&store_path).unwrap();
        let claimed_records = competing.records.clone();
        let claimed_state = competing.state.clone();
        table.handlers.get_mut("restore").unwrap().on_ok = Some(Advance {
            event: "undeclared".into(),
            payload: Value::Obj(BTreeMap::new()),
            stamps: Vec::new(),
        });
        scheduler = Scheduler::new(table.clone());
        watcher = Watcher::with_handlers(store_path.clone(), &table);
        let mut blocked = 0;
        while blocked < 3 {
            let outcome = tick_reporting(
                &mut watcher,
                &mut scheduler,
                &mut runner,
                &mut Pipeline,
                &store_path,
                &mut clock,
                2000,
            );
            blocked += usize::from(outcome.writer_unavailable);
            assert_no_entry(&resource);
            assert_eq!(
                Store::open_read_only(&store_path).unwrap().records,
                claimed_records
            );
            assert!(
                Instant::now() < deadline,
                "bound owner never reached writer backpressure"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(competing);
        // Structurally compatible private changes still cannot replace the
        // original claim's handler fingerprint or consume entry permission.
        for (code, private_arguments) in [
            ("error exec/contract_invalid", false),
            ("error exec/contract_unknown", false),
            ("error exec/contract_unknown", true),
        ] {
            if private_arguments
                && !matches!(
                    original_table.handlers["notify"].kind,
                    fsm_execute::config::HandlerKind::Mcp { .. }
                )
            {
                continue;
            }
            if code.ends_with("unknown") {
                table = original_table.clone();
                let handler = table.handlers.get_mut("notify").unwrap();
                if private_arguments {
                    let fsm_execute::config::HandlerKind::Mcp { arguments, .. } = &mut handler.kind
                    else {
                        unreachable!()
                    };
                    let Value::Obj(arguments) = arguments else {
                        unreachable!()
                    };
                    arguments.insert(
                        "private_contract_probe".into(),
                        Value::Str("PRIVATE_STALE_MCP_LITERAL".into()),
                    );
                } else {
                    handler.argv.push("PRIVATE_STALE_NATIVE_ARGUMENT".into());
                }
                scheduler = Scheduler::new(table.clone());
                watcher = Watcher::with_handlers(store_path.clone(), &table);
            }
            let mut refusals = 0;
            while refusals < 3 {
                let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
                refusals += usize::from(lines.iter().any(|line| line == code));
                assert_no_entry(&resource);
                assert!(!resource.join("root-candidate").exists());
                assert_eq!(runner.local_native_claims().next(), Some(&original_claim));
                let current = Store::open_read_only(&store_path).unwrap();
                assert_eq!(current.records, claimed_records);
                assert!(fsm_store::snapshot::store_states_eq(
                    &claimed_state,
                    &current.state
                ));
                assert!(
                    Instant::now() < deadline,
                    "bound entry did not report {code}: {lines:?}"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        table = original_table;
        scheduler = Scheduler::new(table.clone());
        watcher = Watcher::with_handlers(store_path.clone(), &table);
        assert_eq!(runner.local_native_claims().next(), Some(&original_claim));
        let mut writer = Store::open(&store_path).unwrap();
        let historical = resolve(&writer, &effect_id).unwrap();
        receiver::migrate_invoked_receiver(&mut writer, &mut clock, "original");
        assert_eq!(resolve(&writer, &effect_id).unwrap(), historical);
        let closure_records = writer.records.clone();
        let closure_state = writer.state.clone();
        drop(writer);
        let mut refusals = 0;
        while refusals < 3 {
            let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
            refusals += usize::from(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_invalid"),
            );
            assert_no_entry(&resource);
            assert!(!resource.join("root-candidate").exists());
            assert_eq!(runner.local_native_claims().next(), Some(&original_claim));
            let current = Store::open_read_only(&store_path).unwrap();
            assert_eq!(current.records, closure_records);
            assert!(fsm_store::snapshot::store_states_eq(
                &closure_state,
                &current.state
            ));
            assert!(
                Instant::now() < deadline,
                "new invoked closure reused stale approval: {lines:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut writer = Store::open(&store_path).unwrap();
        migrate_receiver(
            &mut writer,
            &mut clock,
            "original",
            "invalid-receiver",
            true,
        );
        assert_eq!(resolve(&writer, &effect_id).unwrap(), historical);
        assert_ne!(
            historical.emitting_machine_id,
            writer.state.instance_machines["original"]
        );
        let migrated_records = writer.records.clone();
        let migrated_state = writer.state.clone();
        drop(writer);
        let mut refusals = 0;
        while refusals < 3 {
            let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
            refusals += usize::from(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_invalid"),
            );
            assert_no_entry(&resource);
            assert!(!resource.join("root-candidate").exists());
            assert_eq!(runner.local_native_claims().next(), Some(&original_claim));
            let current = Store::open_read_only(&store_path).unwrap();
            assert_eq!(current.records, migrated_records);
            assert!(fsm_store::snapshot::store_states_eq(
                &migrated_state,
                &current.state
            ));
            assert!(
                Instant::now() < deadline,
                "receiver migration did not refuse entry: {lines:?}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut writer = Store::open(&store_path).unwrap();
        migrate_receiver(
            &mut writer,
            &mut clock,
            "original",
            "repaired-receiver",
            false,
        );
        assert_eq!(resolve(&writer, &effect_id).unwrap(), historical);
        drop(writer);
    }
    while !candidate_recorded(&resource) {
        let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        assert!(
            Instant::now() < deadline,
            "compatible handler never entered: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    if matches!(scenario, Scenario::Contention) {
        hold_writer_through_timeout(
            &store_path,
            &resource,
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut clock,
        );
        retry::observe(
            &store_path,
            &resource,
            retry::State {
                table: &mut table,
                watcher: &mut watcher,
                scheduler: &mut scheduler,
                runner: &mut runner,
                clock: &mut clock,
            },
            tick,
        );
    }
    // The held-result fixture waits for its child, which waits for the
    // grandchild; releasing only the root publishes a result but leaves the
    // original tree and inherited pipes live forever under the fixed clock.
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    loop {
        let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        let current = Store::open_read_only(&store_path).unwrap();
        let instance = &current.state.instances[completed_instance];
        let finished = if matches!(scenario, Scenario::AckOnly) {
            instance.status == fsm_core::machine::Status::Running && instance.pending.is_empty()
        } else {
            instance.status == fsm_core::machine::Status::Completed
        };
        if finished
            && current.state.execution.unresolved().count() == 0
            && runner.local_native_claims().next().is_none()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "repaired handler did not settle and retire: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    if matches!(scenario, Scenario::AckOnly) {
        let current = Store::open_read_only(&store_path).unwrap();
        settlement::assert_ack_only(&current, completed_instance, &effect_id);
        let settled_records = current.records.clone();
        drop(current);
        for _ in 0..3 {
            tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
            assert_eq!(
                Store::open_read_only(&store_path).unwrap().records,
                settled_records
            );
        }
    }
    if matches!(scenario, Scenario::Fairness) {
        let current = Store::open_read_only(&store_path).unwrap();
        assert_eq!(
            current.state.instances["original"].pending,
            state.instances["original"].pending
        );
        assert_eq!(
            current.state.instances["original"].status,
            fsm_core::machine::Status::Running
        );
        for record in current
            .records
            .iter()
            .filter(|record| record.kind == fsm_core::record::RecordKind::ExecutionClaimed)
        {
            let mut fields = record.body.as_obj().unwrap().clone();
            fields.remove("request_id");
            fields.remove("request_fp");
            let claim =
                fsm_core::record::execution::Claim::from_value(&Value::Obj(fields)).unwrap();
            assert_eq!(
                claim.effect().0,
                "z-compatible",
                "incompatible work published an execution claim"
            );
        }
    }
    assert_eq!(
        fsm_store::journal_io::verify(&store_path).health,
        fsm_store::journal_io::JournalHealth::Ok
    );
}

fn candidate_recorded(resource: &std::path::Path) -> bool {
    match fs::read(resource.join("root-candidate")) {
        Ok(bytes) => !bytes.is_empty(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => panic!("cannot read original candidate observation: {error}"),
    }
}

fn assert_no_entry(resource: &std::path::Path) {
    // The Root coordinator preallocates these PID slots so DynamicUser cleanup
    // cannot unlink the independent observations; only nonempty slots show entry.
    for role in ["root", "child", "grandchild"] {
        let marker = format!("{role}-entered");
        let observed = fs::read(resource.join(&marker)).unwrap();
        assert!(
            observed.is_empty(),
            "refused generation entered original handler: {marker}"
        );
    }
    for marker in ["root-candidate", "root-published"] {
        assert!(
            !resource.join(marker).try_exists().unwrap(),
            "refused generation entered original handler: {marker}"
        );
    }
}

// Only the initial dispatch borrows a writer; while another caller owns it,
// standalone acquisition must fail without blocking the original native owner.
fn hold_writer_through_timeout(
    store_path: &std::path::Path,
    resource: &std::path::Path,
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    clock: &mut FixedClock,
) {
    let identities: Vec<_> = ["root", "child", "grandchild"]
        .into_iter()
        .map(|role| {
            let pid = fs::read_to_string(resource.join(format!("{role}-entered"))).unwrap();
            let pid: u32 = pid.trim().parse().unwrap();
            let identity = process_identity(pid).expect("original fixture must still be live");
            (pid, identity)
        })
        .collect();
    let candidate = fs::read(resource.join("root-candidate")).unwrap();
    let mut competing = Store::open(store_path).unwrap();
    assert_eq!(competing.state.execution.unresolved().count(), 1);
    assert_eq!(
        competing
            .records
            .iter()
            .filter(|record| record.kind == fsm_core::record::RecordKind::ExecutionClaimed)
            .count(),
        1
    );
    competing
        .create_instance_ctx_on(
            clock,
            "native-admission",
            "blocked",
            "create-blocked",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let records = competing.records.clone();
    let state = competing.state.clone();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut writer_refused = false;
    loop {
        let outcome = tick_reporting(
            watcher,
            scheduler,
            runner,
            &mut Pipeline,
            store_path,
            clock,
            2000,
        );
        writer_refused |= outcome.writer_unavailable;
        let current = Store::open_read_only(store_path).unwrap();
        assert_eq!(
            current.records, records,
            "writer contention published an execution decision"
        );
        assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
        assert_eq!(
            fs::read(resource.join("root-candidate")).unwrap(),
            candidate,
            "competing work entered a second root handler"
        );
        for (role, (pid, _)) in ["root", "child", "grandchild"].into_iter().zip(&identities) {
            assert_eq!(
                fs::read_to_string(resource.join(format!("{role}-entered")))
                    .unwrap()
                    .trim(),
                pid.to_string(),
                "competing work entered a second descendant"
            );
        }
        assert!(!resource.join("root-release").exists());
        assert!(
            !resource.join("root-published").exists(),
            "held handler published a result"
        );
        assert!(
            runner.local_native_claims().next().is_some(),
            "writer contention lost the original owner"
        );
        if identities
            .iter()
            .all(|(pid, identity)| process_identity(*pid).as_ref() != Some(identity))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "timeout did not reap the original process tree while writer was held"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        writer_refused,
        "eligible second instance never reached writer contention"
    );
    competing
        .cancel_instance_reason_on(clock, "blocked", "cancel-blocked", "fixture cleanup")
        .unwrap();
    drop(competing);
}

// Linux start time disambiguates PID reuse; absence of the original identity
// proves reap rather than merely observing a stopped or zombie process.
fn process_identity(pid: u32) -> Option<String> {
    let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("cannot observe original process {pid}: {error}"),
    };
    Some(
        stat.rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap()
            .into(),
    )
}

fn manifest() -> Value {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let path = PathBuf::from(
        std::env::var_os("FSM_CONTRACT_NATIVE_MANIFEST").expect("Root coordinator manifest"),
    );
    assert!(path.is_absolute());
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.uid(), 0);
        assert_eq!(metadata.mode() & 0o022, 0);
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000)
        .open(&path)
        .unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 65_536);
    assert_eq!(
        (metadata.uid(), metadata.mode() & 0o7777, metadata.nlink()),
        (0, 0o444, 1)
    );
    let mut encoded = Vec::new();
    Read::by_ref(&mut file)
        .take(65_537)
        .read_to_end(&mut encoded)
        .unwrap();
    assert!(encoded.len() <= 65_536);
    let current = fs::symlink_metadata(path).unwrap();
    assert_eq!(
        (metadata.dev(), metadata.ino()),
        (current.dev(), current.ino())
    );
    parse(&encoded, &JsonLimits::DEFAULT).unwrap()
}
