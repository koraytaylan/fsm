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

struct Directory(PathBuf);

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
