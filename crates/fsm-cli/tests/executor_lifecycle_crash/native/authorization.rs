//! Observe the actual enrolled gate before its native grant becomes visible.
//! Root-protected handoff identity and kernel membership bind the live gate;
//! only matched closure permits interruption and subsequent handler entry.

use super::*;

pub(super) fn observe(manifest: &Value, store: &Path, resource: &Path, original: &mut Host) {
    let authority = Path::new(field(manifest, "authority"));
    let ready = authority.join("crash-authorization-1.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !fs::symlink_metadata(&ready).is_ok_and(|metadata| {
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o7777 == 0o444
    }) {
        assert!(
            original.process.try_wait().unwrap().is_none(),
            "authorization host exited: {}",
            original.diagnostics()
        );
        assert!(
            Instant::now() < deadline,
            "authorization cut absent: {}",
            original.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    use std::os::unix::fs::OpenOptionsExt;
    let mut source = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000 | 0o4000)
        .open(&ready)
        .unwrap();
    let metadata = source.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 65_536);
    assert_eq!(
        (metadata.uid(), metadata.mode() & 0o7777, metadata.nlink()),
        (0, 0o444, 1)
    );
    let mut encoded = Vec::new();
    Read::by_ref(&mut source)
        .take(65_537)
        .read_to_end(&mut encoded)
        .unwrap();
    assert!(encoded.len() <= 65_536);
    let current = fs::symlink_metadata(&ready).unwrap();
    assert_eq!(
        (metadata.dev(), metadata.ino()),
        (current.dev(), current.ino())
    );
    let observed = parse(&encoded, &JsonLimits::DEFAULT).unwrap();
    let snapshot = Store::open_read_only(store).unwrap();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    let claim = snapshot
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .clone();
    let hash = snapshot.current_execution_claim_hash(&claim).unwrap();
    let records = snapshot.records.clone();
    assert_eq!(claim.run_id(), 1);
    assert_eq!(observed.get("claim"), Some(&claim.to_value()));
    assert_eq!(
        observed.get("cut"),
        Some(&Value::Str("authorization".into()))
    );
    assert!(
        observed.get("candidate").is_none(),
        "authorization must not fabricate a collected candidate"
    );
    let handoff = observed.get("handoff").unwrap();
    assert_eq!(
        handoff.get("format"),
        Some(&Value::Str("fsm.native-launch-handoff/1".into()))
    );
    let binding = handoff.get("binding").unwrap();
    assert_eq!(binding.get("claim"), Some(&claim.to_value()));
    assert_eq!(
        binding.get("journal_claim"),
        Some(&Value::Str(hash.clone()))
    );
    let gate = identity(
        handoff
            .get("gate")
            .unwrap()
            .get("pid")
            .unwrap()
            .as_num()
            .unwrap()
            .parse()
            .unwrap(),
    );
    let domain = claim.domain().to_value();
    let allocation = domain.get("allocation").unwrap().as_num().unwrap();
    assert_eq!(
        fs::read_to_string(format!("/proc/{}/cgroup", gate.0)).unwrap(),
        format!(
            "0::/system.slice/fsm-containment-{}-{}-{allocation}.service\n",
            field(&domain, "namespace"),
            domain.get("generation").unwrap().as_num().unwrap()
        )
    );
    assert!(live(&gate));
    assert!(
        !authority
            .join(format!("entry-{allocation}.json"))
            .try_exists()
            .unwrap()
    );
    assert_no_handler_entry(resource);
    assert!(!records.iter().any(|record| matches!(
        record.kind,
        RecordKind::ExecutionStopped | RecordKind::ExecutionSettled | RecordKind::EventApplied
    )));
    drop(snapshot);
    original.kill_and_wait();
    let deadline = Instant::now() + Duration::from_secs(5);
    let held = loop {
        match Store::open(store) {
            Ok(writer) => break writer,
            Err(error) if error.code == "store/lock" => {
                assert!(
                    Instant::now() < deadline,
                    "authorization host retained its writer"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("authorization writer reopen: {error:?}"),
        }
    };
    assert_eq!(held.records, records);
    let blocked = Host::start(manifest, "immediate-restart");
    assert!(
        live(&gate),
        "restart must begin while the original gate is alive"
    );
    assert_eq!(Store::open_read_only(store).unwrap().records, records);
    assert_no_handler_entry(resource);
    fs::write(
        resource.join("closure-release"),
        b"original gate observed across restart",
    )
    .unwrap();
    let receipt = authority.join(format!("closure-{allocation}-{}.json", claim.run_id()));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert_eq!(Store::open_read_only(store).unwrap().records, records);
        assert_no_handler_entry(resource);
        if let Ok(proof) = VerifiedClosure::read(&receipt) {
            assert!(proof.matches_claim(&claim, &hash));
            proof.check_store(store).unwrap();
            assert!(!live(&gate));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "authorization closure absent: {}",
            blocked.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !authority
            .join(format!("entry-{allocation}.json"))
            .try_exists()
            .unwrap()
    );
    drop(blocked);
    drop(held);
    let successor = Host::start(manifest, "verified-restart");
    let deadline = Instant::now() + Duration::from_secs(12);
    while !resource.join("root-candidate").is_file() {
        assert!(
            Instant::now() < deadline,
            "authorization successor absent: {}",
            successor.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!live(&gate));
    let snapshot = Store::open_read_only(store).unwrap();
    assert!(snapshot.records.starts_with(&records));
    let stopped = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionStopped)
        .unwrap();
    let settled = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionSettled)
        .unwrap();
    let next = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionClaimed && record.seq > stopped.seq)
        .unwrap();
    assert!(stopped.seq < settled.seq && settled.seq < next.seq);
    assert_eq!(
        stopped.body.get("run_id"),
        Some(&Value::Num(claim.run_id().to_string()))
    );
    assert_eq!(
        settled.body.get("run_id"),
        Some(&Value::Num(claim.run_id().to_string()))
    );
    assert_eq!(
        settled.body.get("disposition"),
        Some(&Value::Str("interrupted".into()))
    );
    assert_eq!(next.body.get("attempt"), claim.to_value().get("attempt"));
    assert!(
        !snapshot
            .records
            .iter()
            .any(|record| record.kind == RecordKind::EventApplied)
    );
    drop(snapshot);
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    wait_for_completion(store, &successor);
    let snapshot = Store::open_read_only(store).unwrap();
    assert_eq!(
        snapshot
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::EventApplied)
            .count(),
        1
    );
}
