//! Retry admission must use the current closure, including after executor restart.

use super::*;

pub(super) struct State<'a> {
    pub table: &'a mut HandlerTable,
    pub watcher: &'a mut Watcher,
    pub scheduler: &'a mut Scheduler,
    pub runner: &'a mut Runner,
    pub clock: &'a mut FixedClock,
}

pub(super) fn observe(
    store_path: &std::path::Path,
    resource: &std::path::Path,
    state: State<'_>,
    tick: impl Fn(&mut Watcher, &mut Scheduler, &mut Runner, &mut FixedClock) -> Vec<String>,
) {
    let State {
        table,
        watcher,
        scheduler,
        runner,
        clock,
    } = state;
    let deadline = Instant::now() + Duration::from_secs(12);
    let first = runner.local_native_claims().next().unwrap().clone();
    assert_eq!(
        first.to_value().get("attempt").and_then(Value::as_num),
        Some("1")
    );
    let historical = resolve(
        &Store::open_read_only(store_path).unwrap(),
        first.effect().1,
    )
    .unwrap();
    let markers: Vec<_> = [
        "root-entered",
        "root-candidate",
        "child-entered",
        "grandchild-entered",
    ]
    .into_iter()
    .map(|name| {
        let path = resource.join(name);
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        (path.clone(), fs::read(path).unwrap(), modified)
    })
    .collect();
    assert!(!resource.join("root-published").exists());
    let original_table = table.clone();
    table.handlers.get_mut("restore").unwrap().on_ok = Some(Advance {
        event: "undeclared".into(),
        payload: Value::Obj(BTreeMap::new()),
        stamps: Vec::new(),
    });
    *scheduler = Scheduler::new(table.clone());
    *watcher = Watcher::with_handlers(store_path.to_path_buf(), table);
    // Original completion retains its original handler/retry contract while
    // the replacement closure refuses preparation of the next attempt.
    loop {
        let lines = tick(watcher, scheduler, runner, clock);
        unchanged_markers(resource, &markers);
        let current = Store::open_read_only(store_path).unwrap();
        if current.state.execution.unresolved().count() == 0
            && runner.local_native_claims().next().is_none()
        {
            assert_eq!(
                current.state.instances["original"].status,
                fsm_core::machine::Status::Running
            );
            assert_eq!(resolve(&current, first.effect().1).unwrap(), historical);
            let attempted: Vec<_> = current
                .records
                .iter()
                .filter(|record| {
                    record.kind == fsm_core::record::RecordKind::ExecutionSettled
                        && record.body.get("disposition").and_then(Value::as_str)
                            == Some("attempted")
                })
                .collect();
            assert_eq!(attempted.len(), 1);
            assert_eq!(
                attempted[0].body.get("attempt").and_then(Value::as_num),
                Some("1")
            );
            let observation = watcher.scan(2000).unwrap();
            assert_eq!(observation.attempts[first.effect().1].attempt, 1);
            assert_eq!(
                observation.attempts[first.effect().1].last_ts,
                attempted[0].ts
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "first timeout did not settle as attempted: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let current = Store::open_read_only(store_path).unwrap();
    let records = current.records.clone();
    let snapshot = current.state.clone();
    let attempt = watcher.scan(2000).unwrap().attempts[first.effect().1];
    drop(current);
    let due = fsm_execute::sched::ready_at(
        &original_table.handlers["notify"].retry,
        attempt.attempt,
        attempt.last_ts,
    );
    assert!(due > attempt.last_ts);
    *clock = FixedClock::new(due, 0);
    for restart in [false, true] {
        if restart {
            // The original owner is already closed and retired; this replaces
            // the real executor state, with no unresolved claim to rebind.
            *runner = Runner::new_native().unwrap();
            *scheduler = Scheduler::new(table.clone());
            *watcher = Watcher::with_handlers(store_path.to_path_buf(), table);
        }
        for _ in 0..3 {
            let lines = tick(watcher, scheduler, runner, clock);
            assert!(
                lines
                    .iter()
                    .any(|line| line == "error exec/contract_invalid"),
                "{lines:?}"
            );
            unchanged_markers(resource, &markers);
            let current = Store::open_read_only(store_path).unwrap();
            assert_eq!(current.records, records);
            assert!(fsm_store::snapshot::store_states_eq(
                &snapshot,
                &current.state
            ));
            assert_eq!(resolve(&current, first.effect().1).unwrap(), historical);
            assert!(
                !current
                    .state
                    .dedup
                    .contains_key(&fsm_execute::rid::attempt_rid(first.effect().1, 2))
            );
            assert!(runner.local_native_claims().next().is_none());
            let observed = watcher.scan(2000).unwrap();
            assert_eq!(observed.attempts[first.effect().1].attempt, attempt.attempt);
            assert_eq!(observed.attempts[first.effect().1].last_ts, attempt.last_ts);
        }
    }
    *table = original_table;
    *scheduler = Scheduler::new(table.clone());
    *watcher = Watcher::with_handlers(store_path.to_path_buf(), table);
    // Repair does not erase the original durable backoff: one millisecond
    // before eligibility, no new domain, entry or journal record is permitted.
    *clock = FixedClock::new(due - 1, 0);
    tick(watcher, scheduler, runner, clock);
    unchanged_markers(resource, &markers);
    let current = Store::open_read_only(store_path).unwrap();
    assert_eq!(current.records, records);
    assert!(fsm_store::snapshot::store_states_eq(
        &snapshot,
        &current.state
    ));
    assert!(runner.local_native_claims().next().is_none());
    drop(current);
    *clock = FixedClock::new(due, 0);
    loop {
        let lines = tick(watcher, scheduler, runner, clock);
        if let Some(second) = runner.local_native_claims().next() {
            assert_eq!(second.effect(), first.effect());
            assert_ne!(second.run_id(), first.run_id());
            assert_ne!(second.domain().to_value(), first.domain().to_value());
            assert_eq!(
                second.to_value().get("attempt").and_then(Value::as_num),
                Some("2")
            );
            if markers.iter().all(|(path, _, modified)| {
                fs::metadata(path).unwrap().modified().unwrap() != *modified
            }) {
                assert!(!resource.join("root-published").exists());
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "repaired retry did not enter its second original domain: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn unchanged_markers(
    resource: &std::path::Path,
    markers: &[(PathBuf, Vec<u8>, std::time::SystemTime)],
) {
    for (path, bytes, modified) in markers {
        assert_eq!(
            fs::read(path).unwrap(),
            *bytes,
            "refused retry entered a handler"
        );
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), *modified);
    }
    assert!(!resource.join("root-release").exists());
    assert!(!resource.join("root-published").exists());
}
