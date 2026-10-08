//! Native retry disposition preserves the original deadline across writer reopen.

use super::*;

pub(super) fn settle_retry(
    fixture: &mut Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
    mode: &str,
    barriers: &Barriers,
) {
    use fsm_core::record::execution::{PendingEffect, Settlement};
    let mut store = Store::open(&fixture.store).unwrap();
    let mut pipeline = fsm_execute::run::Pipeline;
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    let before = store.records.len();
    if mode == "retry-timeout-kill" {
        drop(store);
        super::stopped_host::kill_after_publication(fixture, mode);
        store = Store::open(&fixture.store).unwrap();
        assert_eq!(store.records.len(), before + 2);
        assert_eq!(
            store.records[before].kind,
            fsm_core::record::RecordKind::ExecutionStopped
        );
        assert_eq!(
            store.records[before + 1].kind,
            fsm_core::record::RecordKind::ExecutionSettled
        );
        assert_eq!(store.records[before + 1].ts, 1001);
        assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
    } else {
        pipeline
            .stop_native(
                &mut store,
                &mut clock,
                claim,
                completion,
                "native-retry-stop",
            )
            .unwrap();
        assert_eq!(
            store
                .state
                .execution
                .settlement_for(claim, PendingEffect::Present)
                .unwrap(),
            Settlement::Attempted
        );
        let settled = settle_owned(fixture, &mut store, &mut clock, claim, completion);
        assert_eq!(settled.get("duplicate"), Some(&Value::Bool(false)));
    }
    let replay = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Attempted,
            &fsm_execute::rid::attempt_rid(effect, 1),
        )
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(store.records.len(), before + 2);
    assert!(
        store
            .state
            .execution
            .claim_for("instance", effect)
            .is_none()
    );
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", effect)
            .is_none()
    );
    assert!(
        store.state.instances["instance"]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
    let execution = store.state.execution.clone();
    drop(store);
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(reopened.state.execution, execution);
    assert!(
        reopened.state.instances["instance"]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
    drop(reopened);
    let successor = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let material = claim.to_value();
    let retry =
        fsm_core::record::execution::RetryPolicy::from_value(material.get("retry").unwrap())
            .unwrap();
    let request = || fsm_store::store::ExecutionClaimRequest {
        instance_id: "instance",
        effect_id: effect,
        handler_fingerprint: material
            .get("handler_fingerprint")
            .unwrap()
            .as_str()
            .unwrap(),
        retry: &retry,
        domain: &successor,
        request_id: "native-retry-successor",
        expected_seq: None,
    };
    let mut store = Store::open(&fixture.store).unwrap();
    let before = store.records.len();
    // Stop consumes 1000 and Attempted consumes 1001; the original policy
    // permits the successor exactly ten milliseconds after settlement.
    let mut early = fsm_store::clock::FixedClock::new(1010, 1);
    let refused = pipeline
        .claim_native(&mut store, &mut early, request())
        .unwrap_err();
    assert_eq!(refused.code, "exec/store");
    assert_eq!(
        refused
            .details
            .as_ref()
            .unwrap()
            .get("code")
            .and_then(Value::as_str),
        Some("store/execution_retry")
    );
    assert_eq!(store.records.len(), before);
    assert_eq!(store.state.execution, execution);
    let mut due = fsm_store::clock::FixedClock::new(1011, 1);
    let mut changed_fingerprint = material
        .get("handler_fingerprint")
        .unwrap()
        .as_str()
        .unwrap()
        .as_bytes()
        .to_vec();
    changed_fingerprint[7] = if changed_fingerprint[7] == b'0' {
        b'1'
    } else {
        b'0'
    };
    let changed_fingerprint = String::from_utf8(changed_fingerprint).unwrap();
    let mut changed_policy = retry.to_value().as_obj().unwrap().clone();
    changed_policy.insert("attempts".into(), Value::Num("3".into()));
    let changed_policy =
        fsm_core::record::execution::RetryPolicy::from_value(&Value::Obj(changed_policy)).unwrap();
    for changed in [
        fsm_store::store::ExecutionClaimRequest {
            handler_fingerprint: &changed_fingerprint,
            ..request()
        },
        fsm_store::store::ExecutionClaimRequest {
            retry: &changed_policy,
            ..request()
        },
    ] {
        let refused = pipeline
            .claim_native(&mut store, &mut due, changed)
            .unwrap_err();
        assert_eq!(refused.code, "exec/store");
        assert_eq!(
            refused
                .details
                .as_ref()
                .unwrap()
                .get("code")
                .and_then(Value::as_str),
            Some("store/execution_contract")
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(store.state.execution, execution);
        for name in ["binding", "launch", "entry", "handoff"] {
            assert_eq!(
                fs::symlink_metadata(fixture.directory.join(format!("{name}-2.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    pipeline
        .claim_native(&mut store, &mut due, request())
        .unwrap();
    assert_eq!(store.records.len(), before + 1);
    let next = store
        .state
        .execution
        .claim_for("instance", effect)
        .unwrap()
        .clone();
    assert_eq!(next.run_id(), claim.run_id() + 1);
    let next_material = next.to_value();
    assert_eq!(next_material.get("attempt"), Some(&Value::Num("2".into())));
    assert_eq!(next_material.get("retry"), material.get("retry"));
    assert_eq!(
        next_material.get("handler_fingerprint"),
        material.get("handler_fingerprint")
    );
    assert_eq!(next_material.get("domain"), Some(&successor.to_value()));
    if mode == "retry-timeout-kill" {
        let records = store.records.clone();
        let error = pipeline
            .stop_native(
                &mut store,
                &mut due,
                &next,
                completion,
                "stale-successor-stop",
            )
            .unwrap_err();
        assert_eq!(error.store_code(), Some("store/execution_evidence"));
        assert_eq!(store.records, records);
    }
    let retained = store.state.execution.clone();
    drop(store);
    assert_eq!(
        Store::open_read_only(&fixture.store)
            .unwrap()
            .state
            .execution,
        retained
    );
    if mode == "retry-timeout-kill" {
        launch_successor(fixture, barriers, &next, completion);
    }
}

fn launch_successor(
    fixture: &Fixture,
    barriers: &Barriers,
    claim: &fsm_core::record::execution::Claim,
    original_completion: &fsm_execute::run::native_client::NativeCompletion,
) {
    use fsm_execute::run::{Pipeline, native_client::NativeCompletion};
    original_completion
        .proof()
        .check_store(&fixture.store)
        .unwrap();
    assert_eq!(
        identity(&fs::symlink_metadata(&barriers.path).unwrap()),
        barriers.identity
    );
    for name in ["root-ready", "descendant-ready", "release"] {
        let path = barriers.path.join(name);
        // DynamicUser RemoveIPC may already retire its /dev/shm markers;
        // the caller verified native closure before reaching this fixture cleanup.
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                assert!(metadata.is_file());
                fs::remove_file(path).unwrap();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("successor marker inspection failed: {error}"),
        }
    }
    let snapshot = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        snapshot
            .state
            .execution
            .claim_for("instance", claim.effect().1),
        Some(claim)
    );
    let hash = snapshot.current_execution_claim_hash(claim).unwrap();
    drop(snapshot);
    let binding = object([
        ("format", Value::Str("fsm.native-claim-binding/1".into())),
        ("claim", claim.to_value()),
        ("journal_claim", Value::Str(hash.clone())),
    ]);
    bind(&fixture.directory, &binding).unwrap();
    let directory = fixture.directory.clone();
    let execution = std::thread::spawn(move || runner::execute(&directory, 2));
    let deadline = Instant::now() + Duration::from_secs(8);
    while !barriers.path.join("root-ready").exists() {
        assert!(
            !execution.is_finished(),
            "successor ended before enrollment"
        );
        assert!(Instant::now() < deadline, "successor enrollment deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let root = read_value(&barriers.path.join("root-ready"), false).unwrap();
    let descendants = read_value(&barriers.path.join("descendant-ready"), false).unwrap();
    let material = claim.to_value();
    let namespace = text(material.get("domain").unwrap(), "namespace").unwrap();
    let unit = format!("fsm-containment-{namespace}-1-2.service");
    let old_unit = format!("fsm-containment-{namespace}-1-1.service");
    assert!(
        !Path::new("/sys/fs/cgroup/system.slice")
            .join(old_unit)
            .exists()
    );
    for pid in [
        number(&root, "pid").unwrap(),
        number(&descendants, "pid").unwrap(),
        number(&descendants, "grandchild").unwrap(),
    ] {
        assert_eq!(
            fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
            format!("0::/system.slice/{unit}\n")
        );
    }
    let snapshot = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        snapshot
            .state
            .execution
            .claim_for("instance", claim.effect().1),
        Some(claim)
    );
    assert_eq!(snapshot.current_execution_claim_hash(claim).unwrap(), hash);
    drop(snapshot);
    fs::write(
        barriers.path.join("release"),
        b"successor claim and enrollment verified",
    )
    .unwrap();
    while !execution.is_finished() {
        assert!(Instant::now() < deadline, "successor closure deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = execution.join().unwrap().unwrap();
    assert!(!Path::new("/sys/fs/cgroup/system.slice").join(unit).exists());
    let response = object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(true)),
        ("result", result),
    ]);
    let completion = NativeCompletion::verify(&response, claim, &hash).unwrap();
    assert_eq!(completion.stopped_outcome().status(), "timeout");
    let mut writer = Store::open(&fixture.store).unwrap();
    let before = writer.records.len();
    let mut clock = fsm_store::clock::FixedClock::new(2000, 1);
    Pipeline
        .stop_native(
            &mut writer,
            &mut clock,
            claim,
            &completion,
            "successor-stop",
        )
        .unwrap();
    Pipeline
        .settle_native_stopped(&mut writer, &mut clock, claim, &completion)
        .unwrap();
    assert_eq!(writer.records.len(), before + 2);
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    assert!(
        !writer.state.instances["instance"]
            .pending
            .contains(&claim.effect().1.to_owned())
    );
    let records = writer.records.clone();
    drop(writer);
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
}
