//! Cancellation after claim must retire original authority without handler entry.

use super::*;

pub(super) fn observe(
    store_path: &std::path::Path,
    resource: &std::path::Path,
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    clock: &mut FixedClock,
    tick: impl Fn(&mut Watcher, &mut Scheduler, &mut Runner, &mut FixedClock) -> Vec<String>,
) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while runner.local_native_claims().next().is_none() {
        tick(watcher, scheduler, runner, clock);
        assert_no_entry(resource);
        assert!(
            Instant::now() < deadline,
            "original claim was not published"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let claim = runner.local_native_claims().next().unwrap().clone();
    let mut competing = Store::open(store_path).unwrap();
    let claim_hash = competing.current_execution_claim_hash(&claim).unwrap();
    let authority = PathBuf::from(manifest().get("authority").unwrap().as_str().unwrap());
    let domain = claim.domain().to_value();
    let allocation = domain.get("allocation").unwrap().as_num().unwrap();
    let memory_receipt = authority.join(format!("fixture-memory-{allocation}.json"));
    // Require actual contained startup before cancelling: otherwise this case
    // could pass by closing an allocation that never launched its waiting gate.
    let claimed_records = competing.records.clone();
    let claimed_state = competing.state.clone();
    while !memory_receipt.try_exists().unwrap() {
        let outcome = tick_reporting(
            watcher,
            scheduler,
            runner,
            &mut Pipeline,
            store_path,
            clock,
            2000,
        );
        assert_no_entry(resource);
        let current = Store::open_read_only(store_path).unwrap();
        assert_eq!(current.records, claimed_records);
        assert!(fsm_store::snapshot::store_states_eq(
            &claimed_state,
            &current.state
        ));
        assert!(
            Instant::now() < deadline,
            "original contained startup was not observed: {:?}",
            outcome.lines
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    competing
        .cancel_instance_reason_on(clock, "original", "cancel-original", "before handler entry")
        .unwrap();
    let records = competing.records.clone();
    let state = competing.state.clone();
    let closure = authority.join(format!("closure-{allocation}-{}.json", claim.run_id()));
    let mut stop_requested = false;
    let mut writer_refused = false;
    loop {
        // The service observes cancellation and drives cleanup before obtaining
        // the writer; contention cannot authorize entry or block the original stop.
        let outcome = tick_reporting(
            watcher,
            scheduler,
            runner,
            &mut Pipeline,
            store_path,
            clock,
            2000,
        );
        stop_requested |= outcome
            .lines
            .iter()
            .any(|line| line.starts_with("native-stop-requested cancelled "));
        writer_refused |= outcome.writer_unavailable;
        assert_no_entry(resource);
        assert_eq!(runner.local_native_claims().next(), Some(&claim));
        let current = Store::open_read_only(store_path).unwrap();
        assert_eq!(current.records, records);
        assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
        if closure.try_exists().unwrap() {
            let proof = fsm_store::store::VerifiedClosure::read(&closure).unwrap();
            assert!(proof.matches_claim(&claim, &claim_hash));
            proof.check_store(store_path).unwrap();
            for name in [
                format!("entry-{allocation}.json"),
                format!("entry-{allocation}.json.pending"),
            ] {
                assert!(
                    !authority.join(name).try_exists().unwrap(),
                    "cancellation consumed handler entry permission"
                );
            }
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cancelled bound authority did not close under writer contention: {:?}",
            outcome.lines
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(stop_requested && writer_refused);
    drop(competing);
    loop {
        let lines = tick(watcher, scheduler, runner, clock);
        assert_no_entry(resource);
        let current = Store::open_read_only(store_path).unwrap();
        if current.state.execution.unresolved().count() == 0
            && runner.local_native_claims().next().is_none()
        {
            assert_eq!(
                current.state.instances["original"].status,
                fsm_core::machine::Status::Cancelled
            );
            for (kind, expected) in [
                (fsm_core::record::RecordKind::ExecutionClaimed, 1),
                (fsm_core::record::RecordKind::ExecutionStopped, 1),
                (fsm_core::record::RecordKind::ExecutionSettled, 1),
                (fsm_core::record::RecordKind::EffectAcked, 0),
                (fsm_core::record::RecordKind::EventApplied, 0),
                (fsm_core::record::RecordKind::EffectAttempted, 0),
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
            for key in [
                fsm_execute::rid::ack_rid(claim.effect().1),
                fsm_execute::rid::event_rid(claim.effect().1, "done"),
                fsm_execute::rid::attempt_rid(claim.effect().1, 1),
            ] {
                assert!(
                    !current.state.dedup.contains_key(&key),
                    "cancelled work consumed derived request key {key}"
                );
            }
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original cancellation did not settle: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let settled = Store::open_read_only(store_path).unwrap().records.clone();
    for _ in 0..3 {
        tick(watcher, scheduler, runner, clock);
        assert_no_entry(resource);
        assert_eq!(Store::open_read_only(store_path).unwrap().records, settled);
        assert!(runner.local_native_claims().next().is_none());
    }
    assert_eq!(
        fsm_store::journal_io::verify(store_path).health,
        fsm_store::journal_io::JournalHealth::Ok
    );
}

fn assert_no_entry(resource: &std::path::Path) {
    // The Root coordinator preallocates these PID slots so DynamicUser cleanup
    // cannot unlink the independent observations; only nonempty slots show entry.
    for role in ["root", "child", "grandchild"] {
        let marker = format!("{role}-entered");
        let observed = fs::read(resource.join(&marker)).unwrap();
        assert!(
            observed.is_empty(),
            "cancelled generation entered original handler: {marker}"
        );
    }
    for marker in ["root-candidate", "root-published"] {
        assert!(
            !resource.join(marker).try_exists().unwrap(),
            "cancelled generation entered original handler: {marker}"
        );
    }
}
