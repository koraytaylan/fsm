//! Unprovisioned service refusal proves queue routing, not native acceptance.

use super::*;
use fsm_execute::{
    config::Advance,
    run::{Pipeline, Runner},
    sched::Scheduler,
    service::{tick_reporting, tick_with},
    watch::Watcher,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[test]
fn acknowledged_service_recovery_preserves_refused_key_then_advances_once_without_restart() {
    use fsm_execute::rid::{ack_rid, event_rid};
    for borrowed in [false, true] {
        let directory = Directory::new();
        let (source, original, mut table) = fixture();
        let mut document = source.state.machines[&original.emitting_machine_id]
            .compiled
            .spec
            .to_value();
        let Value::Obj(fields) = &mut document else {
            unreachable!()
        };
        fields.insert(
            "events".into(),
            parse(
                br#"[{"name":"next","fields":[{"name":"approved","ty":"bool"}]}]"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
        );
        table.handlers.get_mut("work").unwrap().on_ok = Some(Advance {
            event: "next".into(),
            payload: Value::Obj(BTreeMap::from([(
                "approved".into(),
                Value::Str("wrong".into()),
            )])),
            stamps: Vec::new(),
        });
        let mut clock = FixedClock::new(2000, 1);
        let mut store = Store::open(&directory.0).unwrap();
        store
            .define_machine_on(&mut clock, document, false, false)
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
        let effect = store.state.instances["case-1"].pending[0].clone();
        store
            .ack_effect_outcome_on(&mut clock, "case-1", &effect, &ack_rid(&effect), "ok", None)
            .unwrap();
        let state = store.state.clone();
        let records = store.records.clone();
        drop(store);
        let derived = event_rid(&effect, "next");
        let mut watcher = Watcher::with_handlers(directory.0.clone(), &table);
        let mut scheduler = Scheduler::new(table.clone());
        let mut runner = Runner::new_native().unwrap();
        let mut tick = |watcher: &mut Watcher, scheduler: &mut Scheduler, runner: &mut Runner| {
            let lines = if borrowed {
                let mut writer = Store::open(&directory.0).unwrap();
                tick_with(
                    watcher,
                    scheduler,
                    runner,
                    &mut Pipeline,
                    &mut writer,
                    &mut clock,
                    2000,
                )
            } else {
                tick_reporting(
                    watcher,
                    scheduler,
                    runner,
                    &mut Pipeline,
                    &directory.0,
                    &mut clock,
                    2000,
                )
                .lines
            };
            assert!(
                !lines
                    .iter()
                    .any(|line| line == &format!("observed pending work {effect}")),
                "acknowledged work was selected for a new start: {lines:?}"
            );
            assert!(scheduler.inflight_effect(&effect).is_none());
            lines
        };
        for _ in 0..3 {
            let lines = tick(&mut watcher, &mut scheduler, &mut runner);
            assert!(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_invalid"),
                "{lines:?}"
            );
            let snapshot = Store::open_read_only(&directory.0).unwrap();
            assert_eq!(snapshot.records, records);
            assert!(fsm_store::snapshot::store_states_eq(
                &state,
                &snapshot.state
            ));
            assert!(!snapshot.state.dedup.contains_key(&derived));
            assert!(runner.local_native_claims().next().is_none());
        }
        table
            .handlers
            .get_mut("work")
            .unwrap()
            .on_ok
            .as_mut()
            .unwrap()
            .payload = Value::Obj(BTreeMap::from([("approved".into(), Value::Bool(true))]));
        watcher = Watcher::with_handlers(directory.0.clone(), &table);
        scheduler = Scheduler::new(table);
        tick(&mut watcher, &mut scheduler, &mut runner);
        let recovered = Store::open_read_only(&directory.0).unwrap();
        assert!(recovered.state.dedup.contains_key(&derived));
        assert_eq!(
            recovered
                .records
                .iter()
                .filter(|record| record.kind == fsm_core::record::RecordKind::EffectAcked)
                .count(),
            1
        );
        assert_eq!(
            recovered
                .records
                .iter()
                .filter(|record| record.kind == fsm_core::record::RecordKind::EventApplied)
                .count(),
            1
        );
        assert!(runner.local_native_claims().next().is_none());
        let records = recovered.records.clone();
        drop(recovered);
        for _ in 0..3 {
            tick(&mut watcher, &mut scheduler, &mut runner);
            assert_eq!(
                Store::open_read_only(&directory.0).unwrap().records,
                records
            );
            assert!(runner.local_native_claims().next().is_none());
        }
    }
}

struct Directory(PathBuf);

#[test]
fn migrated_receiver_invalidates_warm_service_evidence_without_replacing_historical_arguments() {
    for borrowed in [false, true] {
        let directory = Directory::new();
        let (store, effect, mut table) = durable_fixture(&directory);
        table.handlers.get_mut("work").unwrap().on_ok = Some(Advance {
            event: "next".into(),
            payload: Value::Obj(BTreeMap::new()),
            stamps: Vec::new(),
        });
        let mut replacement = store.state.machines[&store.state.instance_machines["case-1"]]
            .compiled
            .spec
            .to_value();
        let Value::Obj(fields) = &mut replacement else {
            unreachable!()
        };
        fields.insert("name".into(), Value::Str("replacement".into()));
        let digest = fsm_core::hashes::digest_of(&store.state.instance_machines["case-1"]).unwrap();
        fields.insert("supersedes".into(), parse(format!(
            r#"{{"machine":"{digest}","states":{{"first":"first","later":"later"}},"context":{{"resource":"\"replacement\""}}}}"#
        ).as_bytes(), &JsonLimits::DEFAULT).unwrap());
        fields.insert(
            "events".into(),
            parse(
                br#"[{"name":"next","fields":[{"name":"approved","ty":"bool"}]}]"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
        );
        let before = store.records.clone();
        drop(store);
        let mut watcher = Watcher::with_handlers(directory.0.clone(), &table);
        let mut scheduler = Scheduler::new(table.clone());
        let mut runner = Runner::new_native().unwrap();
        let mut clock = FixedClock::new(2000, 1);
        let mut tick = |watcher: &mut Watcher, scheduler: &mut Scheduler, runner: &mut Runner| {
            if borrowed {
                let mut writer = Store::open(&directory.0).unwrap();
                tick_with(
                    watcher,
                    scheduler,
                    runner,
                    &mut Pipeline,
                    &mut writer,
                    &mut clock,
                    2000,
                )
            } else {
                tick_reporting(
                    watcher,
                    scheduler,
                    runner,
                    &mut Pipeline,
                    &directory.0,
                    &mut clock,
                    2000,
                )
                .lines
            }
        };
        let warm = tick(&mut watcher, &mut scheduler, &mut runner);
        assert!(
            warm.iter().any(|line| line == "error exec/mode"),
            "{warm:?}"
        );
        assert_eq!(Store::open_read_only(&directory.0).unwrap().records, before);
        let mut store = Store::open(&directory.0).unwrap();
        let historical = resolve(&store, &effect).unwrap();
        let mut migration_clock = FixedClock::new(3000, 1);
        store
            .define_machine_on(&mut migration_clock, replacement, false, false)
            .unwrap();
        store
            .migrate_instance_on(&mut migration_clock, "case-1", "replacement", "migration")
            .unwrap();
        let reconstructed = resolve(&store, &effect).unwrap();
        assert_eq!(reconstructed, historical);
        assert_eq!(
            store.state.instances["case-1"].ctx["resource"],
            Val::Str("replacement".into())
        );
        assert_eq!(reconstructed.args["resource"], Val::Str("original".into()));
        assert_ne!(
            reconstructed.emitting_machine_id,
            store.state.instance_machines["case-1"]
        );
        let state = store.state.clone();
        let records = store.records.clone();
        drop(store);
        for _ in 0..3 {
            let lines = tick(&mut watcher, &mut scheduler, &mut runner);
            assert!(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_invalid"),
                "{lines:?}"
            );
            assert!(
                !lines.iter().any(|line| line.starts_with("native-preparing")
                    || line.starts_with("native-claimed")
                    || line.starts_with("native-launched"))
            );
            let snapshot = Store::open_read_only(&directory.0).unwrap();
            assert_eq!(snapshot.records, records);
            assert!(fsm_store::snapshot::store_states_eq(
                &state,
                &snapshot.state
            ));
            assert!(scheduler.inflight_effect(&effect).is_none());
            assert!(runner.local_native_claims().next().is_none());
        }
        table
            .handlers
            .get_mut("work")
            .unwrap()
            .on_ok
            .as_mut()
            .unwrap()
            .payload = Value::Obj(BTreeMap::from([("approved".into(), Value::Bool(true))]));
        watcher = Watcher::with_handlers(directory.0.clone(), &table);
        scheduler = Scheduler::new(table);
        let repaired = tick(&mut watcher, &mut scheduler, &mut runner);
        assert!(
            repaired.iter().any(|line| line == "error exec/mode"),
            "{repaired:?}"
        );
        assert!(
            !repaired
                .iter()
                .any(|line| line == "error exec/contract_invalid")
        );
        let snapshot = Store::open_read_only(&directory.0).unwrap();
        assert_eq!(snapshot.records, records);
        assert_eq!(resolve(&snapshot, &effect).unwrap(), historical);
        assert!(fsm_store::snapshot::store_states_eq(
            &state,
            &snapshot.state
        ));
    }
}

impl Directory {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fsm-contract-service-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn durable_fixture(directory: &Directory) -> (Store, String, HandlerTable) {
    let (source, effect, table) = fixture();
    let mut store = Store::open(&directory.0).unwrap();
    let mut clock = FixedClock::new(1000, 1);
    store
        .define_machine_on(
            &mut clock,
            source.state.machines[&effect.emitting_machine_id]
                .compiled
                .spec
                .to_value(),
            false,
            false,
        )
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
    let effect = store.state.instances["case-1"].pending[0].clone();
    (store, effect, table)
}

#[test]
fn native_service_queue_refuses_later_or_missing_argument_contract_through_both_writer_entries() {
    for (borrowed, invalid_later) in [(false, true), (true, true), (false, false), (true, false)] {
        let directory = Directory::new();
        let (mut store, effect, mut table) = durable_fixture(&directory);
        if invalid_later {
            table.handlers.get_mut("restore").unwrap().on_ok = Some(Advance {
                event: "undeclared".into(),
                payload: Value::Obj(BTreeMap::new()),
                stamps: Vec::new(),
            });
        } else {
            table
                .handlers
                .get_mut("work")
                .unwrap()
                .argv
                .push("{missing}".into());
        }
        let records = store.records.clone();
        let state = store.state.clone();
        let mut watcher = Watcher::with_handlers(directory.0.clone(), &table);
        let mut scheduler = Scheduler::new(table);
        let mut runner = Runner::new_native().unwrap();
        let mut clock = FixedClock::new(2000, 1);
        let lines = if borrowed {
            tick_with(
                &mut watcher,
                &mut scheduler,
                &mut runner,
                &mut Pipeline,
                &mut store,
                &mut clock,
                2000,
            )
        } else {
            drop(store);
            let outcome = tick_reporting(
                &mut watcher,
                &mut scheduler,
                &mut runner,
                &mut Pipeline,
                &directory.0,
                &mut clock,
                2000,
            );
            assert!(!outcome.writer_unavailable);
            store = Store::open(&directory.0).unwrap();
            outcome.lines
        };
        assert!(
            lines
                .iter()
                .any(|line| line == "error exec/contract_invalid"),
            "{lines:?}"
        );
        assert!(
            !lines.iter().any(|line| line.starts_with("native-preparing")
                || line.starts_with("native-claimed")
                || line.starts_with("native-launched")),
            "{lines:?}"
        );
        assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
        assert_eq!(store.records, records);
        assert!(scheduler.inflight_effect(&effect).is_none());
        assert!(runner.local_native_claims().next().is_none());
        assert!(runner.finished_effects().is_empty());
    }
}

#[test]
fn contended_native_queue_reports_writer_unavailable_and_releases_unclaimed_reservation() {
    let directory = Directory::new();
    let (store, effect, table) = durable_fixture(&directory);
    let state = store.state.clone();
    let records = store.records.clone();
    let mut watcher = Watcher::with_handlers(directory.0.clone(), &table);
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new_native().unwrap();
    let mut clock = FixedClock::new(2000, 1);
    for _ in 0..3 {
        let outcome = tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut Pipeline,
            &directory.0,
            &mut clock,
            2000,
        );
        assert!(outcome.writer_unavailable, "{:?}", outcome.lines);
        assert!(
            outcome
                .lines
                .iter()
                .any(|line| line == "error exec/store store/lock")
        );
        assert!(
            !outcome
                .lines
                .iter()
                .any(|line| line.starts_with("native-preparing"))
        );
        assert!(scheduler.inflight_effect(&effect).is_none());
        assert!(runner.local_native_claims().next().is_none());
    }
    assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
    assert_eq!(store.records, records);
    drop(store);
    let outcome = tick_reporting(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut Pipeline,
        &directory.0,
        &mut clock,
        2000,
    );
    assert!(!outcome.writer_unavailable);
    assert!(
        outcome
            .lines
            .iter()
            .any(|line| line.starts_with("observed pending "))
    );
    assert!(outcome.lines.iter().any(|line| line == "error exec/mode"));
    assert!(scheduler.inflight_effect(&effect).is_none());
}

#[test]
fn incompatible_work_cannot_repeatedly_take_the_only_native_selection_slot() {
    for borrowed in [false, true] {
        let directory = Directory::new();
        let (mut store, _, mut table) = durable_fixture(&directory);
        table.max_inflight = 1;
        table.max_inflight_per_instance = 1;
        table.handlers.get_mut("restore").unwrap().on_ok = Some(Advance {
            event: "undeclared".into(),
            payload: Value::Obj(BTreeMap::new()),
            stamps: Vec::new(),
        });
        let mut clock = FixedClock::new(2000, 1);
        store.define_machine_on(&mut clock, parse(br#"{
          "format":"fsm.machine/1","name":"compatible","context":[],"events":[],
          "effects":[{"name":"work","fields":[]}],
          "states":[{"name":"ready","entry":{"emit":[{"effect":"work","args":{"resource":"\"original\""}}]}}],
          "initial":"ready","transitions":[]
        }"#, &JsonLimits::DEFAULT).unwrap(), false, false).unwrap();
        store
            .create_instance_ctx_on(
                &mut clock,
                "compatible",
                "z-compatible",
                "create-compatible",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        let good = store.state.instances["z-compatible"].pending[0].clone();
        let records = store.records.clone();
        let mut watcher = Watcher::with_handlers(directory.0.clone(), &table);
        let mut scheduler = Scheduler::new(table);
        let mut runner = Runner::new_native().unwrap();
        drop(store);
        for _ in 0..3 {
            let lines = if borrowed {
                let mut writer = Store::open(&directory.0).unwrap();
                tick_with(
                    &mut watcher,
                    &mut scheduler,
                    &mut runner,
                    &mut Pipeline,
                    &mut writer,
                    &mut clock,
                    2000,
                )
            } else {
                tick_reporting(
                    &mut watcher,
                    &mut scheduler,
                    &mut runner,
                    &mut Pipeline,
                    &directory.0,
                    &mut clock,
                    2000,
                )
                .lines
            };
            assert!(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_invalid"),
                "{lines:?}"
            );
            assert!(
                lines
                    .iter()
                    .any(|line| line == &format!("observed pending work {good}")),
                "incompatible work starved the compatible candidate: {lines:?}"
            );
            assert_eq!(
                Store::open_read_only(&directory.0).unwrap().records,
                records
            );
        }
    }
}

#[test]
fn unknown_timestamp_outcome_refuses_both_service_entries_without_mutation() {
    for borrowed in [false, true] {
        let directory = Directory::new();
        let (source, original, mut table) = fixture();
        let Value::Obj(mut document) = source.state.machines[&original.emitting_machine_id]
            .compiled
            .spec
            .to_value()
        else {
            unreachable!()
        };
        document.insert(
            "enums".into(),
            parse(br#"{"StampRange":["0","2000"]}"#, &JsonLimits::DEFAULT).unwrap(),
        );
        let Value::Arr(events) = document.get_mut("events").unwrap() else {
            unreachable!()
        };
        events.push(
            parse(
                br#"{"name":"stamped","fields":[{"name":"at","ty":{"enum":"StampRange"}}]}"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
        );
        table.handlers.get_mut("restore").unwrap().on_ok = Some(Advance {
            event: "stamped".into(),
            payload: Value::Obj(BTreeMap::new()),
            stamps: vec!["at".into()],
        });
        let mut clock = FixedClock::new(2000, 1);
        let mut store = Store::open(&directory.0).unwrap();
        store
            .define_machine_on(&mut clock, Value::Obj(document), false, false)
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
        let state = store.state.clone();
        let records = store.records.clone();
        drop(store);
        let mut watcher = Watcher::with_handlers(directory.0.clone(), &table);
        let mut scheduler = Scheduler::new(table);
        let mut runner = Runner::new_native().unwrap();
        for _ in 0..3 {
            let lines = if borrowed {
                let mut writer = Store::open(&directory.0).unwrap();
                tick_with(
                    &mut watcher,
                    &mut scheduler,
                    &mut runner,
                    &mut Pipeline,
                    &mut writer,
                    &mut clock,
                    2000,
                )
            } else {
                tick_reporting(
                    &mut watcher,
                    &mut scheduler,
                    &mut runner,
                    &mut Pipeline,
                    &directory.0,
                    &mut clock,
                    2000,
                )
                .lines
            };
            assert!(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_unknown"),
                "{lines:?}"
            );
            assert!(
                !lines.iter().any(|line| line.starts_with("native-preparing")
                    || line.starts_with("native-claimed")
                    || line.starts_with("native-launched")),
                "{lines:?}"
            );
            let current = Store::open_read_only(&directory.0).unwrap();
            assert_eq!(current.records, records);
            assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
            assert!(runner.local_native_claims().next().is_none());
        }
    }
}
