//! Physical guard refusal is asserted only after authentic original cleanup.

use super::*;
use fsm_execute::run::native_client::NativeShutdown;

#[test]
#[ignore = "requires disposable native CI and exact staged guard artifacts"]
fn standalone_structural_guard_refuses_real_handler_entry() {
    super::observe(false, Scenario::GuardStructure);
}

#[test]
#[ignore = "requires disposable native CI and exact staged guard artifacts"]
fn borrowed_structural_guard_refuses_real_handler_entry() {
    super::observe(true, Scenario::GuardStructure);
}

#[test]
#[ignore = "requires disposable native CI and exact staged guard artifacts"]
fn standalone_bound_guard_refuses_real_handler_entry() {
    super::observe(false, Scenario::GuardBound);
}

#[test]
#[ignore = "requires disposable native CI and exact staged guard artifacts"]
fn borrowed_bound_guard_refuses_real_handler_entry() {
    super::observe(true, Scenario::GuardBound);
}

pub(super) struct Fixture<'a> {
    store_path: &'a std::path::Path,
    resource: &'a std::path::Path,
    scenario: Scenario,
}

impl<'a> Fixture<'a> {
    pub(super) fn new(
        store_path: &'a std::path::Path,
        resource: &'a std::path::Path,
        scenario: Scenario,
    ) -> Self {
        Self {
            store_path,
            resource,
            scenario,
        }
    }
}

pub(super) fn observe(
    fixture: Fixture<'_>,
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    clock: &mut FixedClock,
    tick: impl Fn(&mut Watcher, &mut Scheduler, &mut Runner, &mut FixedClock) -> Vec<String>,
) -> bool {
    let Fixture {
        store_path,
        resource,
        scenario,
    } = fixture;
    if !matches!(scenario, Scenario::GuardStructure | Scenario::GuardBound) {
        return false;
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    if matches!(scenario, Scenario::GuardBound) {
        while runner.local_native_claims().next().is_none() {
            tick(watcher, scheduler, runner, clock);
            assert_no_entry(resource);
            assert!(
                Instant::now() < deadline,
                "guard probe never published its original claim"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut store = open_writer(store_path, deadline);
        migrate_receiver(&mut store, clock, "original", "guard-receiver", true);
    }
    let before = Store::open_read_only(store_path).unwrap();
    let records = before.records.clone();
    let state = before.state.clone();
    drop(before);
    let mut refusals = 0;
    let external_entry = loop {
        let lines = tick(watcher, scheduler, runner, clock);
        let entered = !fs::read(resource.join("root-entered")).unwrap().is_empty();
        if entered {
            break true;
        }
        refusals += lines
            .iter()
            .filter(|line| *line == "error exec/contract_invalid")
            .count();
        if refusals >= 3 {
            assert_no_entry(resource);
            let current = Store::open_read_only(store_path).unwrap();
            assert_eq!(current.records, records);
            assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
            break false;
        }
        assert!(
            Instant::now() < deadline,
            "guard probe neither refused nor entered: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let original_claim = runner.local_native_claims().next().cloned();
    if let Some(claim) = original_claim {
        // Cleanup never grants entry or substitutes a claim; even a deliberate
        // failing property leaves the original domain and helper fully retired.
        let mut writer = open_writer(store_path, deadline);
        writer
            .cancel_instance_reason_on(clock, "original", "guard-cleanup", "guard probe cleanup")
            .unwrap();
        drop(writer);
        let snapshot = Store::open_read_only(store_path).unwrap();
        let mut shutdown =
            NativeShutdown::start(&snapshot, &claim, Duration::from_secs(5)).unwrap();
        drop(snapshot);
        loop {
            if shutdown.poll().unwrap().is_some() && shutdown.reap().unwrap() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "guard probe original closure did not retire"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut writer = open_writer(store_path, deadline);
        shutdown.settle_interrupted(&mut writer, clock).unwrap();
        while !runner
            .retire_native_interrupted(&mut writer, &claim, &mut shutdown, scheduler)
            .unwrap()
        {
            assert!(
                Instant::now() < deadline,
                "guard probe original helper did not retire"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(writer.state.execution.unresolved().count(), 0);
        assert_eq!(writer.state.execution_handoffs.outstanding().count(), 0);
    }
    assert!(runner.local_native_claims().next().is_none());
    assert_eq!(
        fsm_store::journal_io::verify(store_path).health,
        fsm_store::journal_io::JournalHealth::Ok
    );
    assert!(
        !external_entry,
        "native contract guard permitted external entry"
    );
    true
}

fn open_writer(path: &std::path::Path, deadline: Instant) -> Store {
    loop {
        match Store::open(path) {
            Ok(store) => return store,
            Err(error) if error.code == "store/lock" => {
                assert!(
                    Instant::now() < deadline,
                    "guard probe writer did not retire"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("guard probe writer failed: {error:?}"),
        }
    }
}
