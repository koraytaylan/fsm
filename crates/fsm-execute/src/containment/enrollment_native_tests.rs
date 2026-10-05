//! Production startup, entry, matched stop and protected closure controls.

use super::super::super::{authorize, bind, enrollment, launch, manager, object, read_value};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::Store;
use std::fs;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

struct Gate<'a> {
    fixture: &'a Fixture,
    child: Child,
}

impl Drop for Gate<'_> {
    fn drop(&mut self) {
        // Production submission validates the owned inode before touching it;
        // an already absent domain is not manufactured into closure evidence.
        let _ = super::super::super::termination::request(&self.fixture.directory, 1);
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

pub(super) fn run() {
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let material = domain.to_value();
    let namespace = super::super::super::text(&material, "namespace").unwrap();
    let generation = super::super::super::number(&material, "generation")
        .unwrap()
        .to_string();
    let allocation = super::super::super::number(&material, "allocation")
        .unwrap()
        .to_string();
    let unit = format!("fsm-containment-{namespace}-{generation}-{allocation}.service");
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let intent_path = fixture.directory.join("launch-1.json");
    // Partial intent from a failed prior submission cannot arm another gate.
    fs::write(&intent_path, b"partial native launch intent").unwrap();
    assert!(
        launch::begin(
            &fixture.directory,
            1,
            [Stdio::null(), Stdio::null(), Stdio::null()]
        )
        .unwrap_err()
        .contains("already submitted or uncertain")
    );
    assert_eq!(
        fs::read(&intent_path).unwrap(),
        b"partial native launch intent"
    );
    assert!(
        fs::read_to_string(fixture.groups[0].0.join("cgroup.events"))
            .unwrap()
            .lines()
            .any(|line| line == "populated 0")
    );
    // Explicit repair of this test-owned injected partial record, not cold
    // production recovery or permission to recycle a failed launch.
    fs::remove_file(&intent_path).unwrap();
    for suffix in ["json", "json.pending"] {
        let prearmed = fixture.directory.join(format!("entry-1.{suffix}"));
        for symlink in [false, true] {
            if symlink {
                std::os::unix::fs::symlink("test-owned-absent-target", &prearmed).unwrap();
            } else {
                fs::write(&prearmed, b"test-owned-prearmed-entry").unwrap();
            }
            assert!(
                launch::begin(
                    &fixture.directory,
                    1,
                    [Stdio::null(), Stdio::null(), Stdio::null()]
                )
                .unwrap_err()
                .contains("preexisting entry authorization")
            );
            assert!(!intent_path.exists());
            assert!(
                fs::read_to_string(fixture.groups[0].0.join("cgroup.events"))
                    .unwrap()
                    .lines()
                    .any(|line| line == "populated 0")
            );
            fs::remove_file(&prearmed).unwrap();
        }
    }
    let (mut stdout, output) = UnixStream::pair().unwrap();
    let (mut stderr, diagnostics) = UnixStream::pair().unwrap();
    stdout.set_nonblocking(true).unwrap();
    stderr.set_nonblocking(true).unwrap();
    let output: OwnedFd = output.into();
    let diagnostics: OwnedFd = diagnostics.into();
    let (child, bound) = launch::begin(
        &fixture.directory,
        1,
        [Stdio::null(), Stdio::from(output), Stdio::from(diagnostics)],
    )
    .unwrap();
    assert_eq!(bound, Duration::from_millis(5100));
    let mut gate = Gate {
        fixture: &fixture,
        child,
    };
    let intent = read_value(&intent_path, true).unwrap();
    assert_eq!(intent.get("binding"), Some(&binding));
    let handoff_path = fixture.directory.join("handoff-1.json");
    let handoff = read_value(&handoff_path, true).unwrap();
    assert_eq!(handoff.get("binding"), Some(&binding));
    assert_eq!(
        handoff.get("format"),
        Some(&Value::Str("fsm.native-launch-handoff/1".into()))
    );
    assert!(
        launch::begin(
            &fixture.directory,
            1,
            [Stdio::null(), Stdio::null(), Stdio::null()]
        )
        .unwrap_err()
        .contains("already submitted or uncertain")
    );
    assert_eq!(read_value(&intent_path, true).unwrap(), intent);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if manager::properties(&unit, &["ActiveState", "SubState"]).is_ok_and(|fields| {
            fields["ActiveState"] == "active" && fields["SubState"] == "running"
        }) {
            break;
        }
        assert!(
            gate.child.try_wait().unwrap().is_none(),
            "gate exited before enrollment"
        );
        assert!(Instant::now() < deadline, "gate enrollment timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Exact executable/argv/proc/security validation proves this is still the
    // gate rather than the approved handler, before any grant exists.
    let group = enrollment::group(&material).unwrap();
    assert_eq!(
        handoff.get("gate").unwrap().get("group_id"),
        Some(&Value::Num(group.to_string()))
    );
    assert!(!fixture.directory.join("entry-1.json").exists());
    assert!(gate.child.try_wait().unwrap().is_none());
    let grant = object([
        ("format", Value::Str("fsm.native-entry/1".into())),
        ("claim", binding.get("claim").unwrap().clone()),
        (
            "journal_claim",
            binding.get("journal_claim").unwrap().clone(),
        ),
        ("argv", Value::Arr(vec![Value::Str("/bin/true".into())])),
    ]);
    let request = object([("grant", grant)]);
    // A missing protected handoff is not inferred from a live gate alone.
    let saved = fixture.directory.join("test-owned-handoff.json");
    fs::rename(&handoff_path, &saved).unwrap();
    assert!(authorize::publish_enrolled(&fixture.directory, &request).is_err());
    assert!(!fixture.directory.join("entry-1.json").exists());
    fs::rename(&saved, &handoff_path).unwrap();
    let mut altered = handoff.as_obj().unwrap().clone();
    let mut identity = handoff.get("gate").unwrap().as_obj().unwrap().clone();
    identity.insert("invocation_id".into(), Value::Str("0".repeat(32)));
    altered.insert("gate".into(), Value::Obj(identity));
    fs::write(
        &handoff_path,
        fsm_core::canon::canon_bytes(&Value::Obj(altered)),
    )
    .unwrap();
    assert!(
        authorize::publish_enrolled(&fixture.directory, &request)
            .unwrap_err()
            .contains("differs from protected handoff")
    );
    assert!(!fixture.directory.join("entry-1.json").exists());
    // Repair only this deliberately injected test-owned corruption.
    fs::write(&handoff_path, fsm_core::canon::canon_bytes(&handoff)).unwrap();
    authorize::publish_enrolled(&fixture.directory, &request).unwrap();
    let metadata = fs::symlink_metadata(fixture.directory.join("entry-1.json")).unwrap();
    assert_eq!(
        (metadata.uid(), metadata.gid(), metadata.mode() & 0o777),
        (0, group, 0o440)
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let properties = manager::properties(
            &unit,
            &[
                "InvocationID",
                "ExecMainPID",
                "ExecMainCode",
                "ExecMainStatus",
            ],
        )
        .unwrap();
        assert_eq!(
            properties["InvocationID"],
            handoff
                .get("gate")
                .unwrap()
                .get("invocation_id")
                .unwrap()
                .as_str()
                .unwrap()
        );
        assert_eq!(
            properties["ExecMainPID"],
            super::super::super::number(handoff.get("gate").unwrap(), "pid")
                .unwrap()
                .to_string()
        );
        if properties["ExecMainCode"] == "1" {
            assert_eq!(properties["ExecMainStatus"], "0");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "approved handler root exit timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!manager::retired(&unit, deadline).unwrap());
    assert!(super::super::super::closure::complete(&fixture.directory, 1).is_err());
    assert!(!fixture.directory.join("closed-1.json").exists());
    super::super::super::stop::request(&fixture.directory, 1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = gate.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "production gate did not execute approved true handler"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "approved gate handler exit timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!fixture.directory.join("closed-1.json").exists());
    let deadline = Instant::now() + Duration::from_secs(2);
    for stream in [&mut stdout, &mut stderr] {
        let mut byte = [0; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => panic!("quiet true handler unexpectedly produced output"),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("production stream observation failed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "production transport retained a stream writer"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    assert!(fixture.directory.join("manager-stopped-1.json").exists());
    super::super::super::closure::complete(&fixture.directory, 1).unwrap();
    assert!(fixture.directory.join("manager-stopped-1.json").exists());
    let run_id = super::super::super::number(binding.get("claim").unwrap(), "run_id").unwrap();
    let receipt = fixture.directory.join(format!("closure-1-{run_id}.json"));
    fsm_store::store::VerifiedClosure::read(&receipt).unwrap();
    super::super::super::closure::complete(&fixture.directory, 1).unwrap();
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", &effect)
            .is_none()
    );
    drop(store);
    drop(gate);
    fixture.cleanup().unwrap();
    stop_running_handler();
    execute_fast_process_handlers();
    execute_process_handlers();
    super::runner_cases::run();
}

fn execute_fast_process_handlers() {
    for (argv, status, failure) in [
        (vec!["/bin/true"], "0", Value::Null),
        (vec!["/bin/false"], "1", Value::Str("nonzero_exit".into())),
        (
            vec!["/bin/sh", "-c", "kill -KILL $$"],
            "-1",
            Value::Str("nonzero_exit".into()),
        ),
    ] {
        let table = object([
            ("format", Value::Str("fsm.handlers/1".into())),
            (
                "handlers",
                Value::Arr(vec![object([
                    ("effect", Value::Str("notify".into())),
                    (
                        "argv",
                        Value::Arr(argv.into_iter().map(|arg| Value::Str(arg.into())).collect()),
                    ),
                    ("timeout_ms", Value::Num("1000".into())),
                    (
                        "retry",
                        object([
                            ("attempts", Value::Num("1".into())),
                            ("backoff_ms", Value::Num("10".into())),
                            ("max_backoff_ms", Value::Num("10".into())),
                            ("on", Value::Arr(vec![])),
                        ]),
                    ),
                ])]),
            ),
        ]);
        let mut fixture = Fixture::new_for_table(table);
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, _) = claim_binding(&fixture, &domain);
        bind(&fixture.directory, &binding).unwrap();
        let result = super::super::super::runner::execute(&fixture.directory, 1).unwrap();
        assert_eq!(result.get("failure_class"), Some(&failure));
        assert_eq!(
            result.get("candidate").unwrap().get("status"),
            Some(&Value::Num(status.into()))
        );
        fsm_store::store::VerifiedClosure::read(Path::new(
            result.get("receipt").unwrap().as_str().unwrap(),
        ))
        .unwrap();
        fixture.cleanup().unwrap();
    }
}

fn execute_process_handlers() {
    for size in [4096, 4097, 1048577] {
        let source = format!(
            r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"notify","argv":["/usr/bin/head","-c","{size}","/dev/zero"],"timeout_ms":1000,"retry":{{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}}}]}}"#
        );
        let table =
            fsm_core::json::parse(source.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap();
        let mut fixture = Fixture::new_for_table(table);
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, effect) = claim_binding(&fixture, &domain);
        bind(&fixture.directory, &binding).unwrap();
        let result = super::super::super::runner::execute(&fixture.directory, 1).unwrap();
        assert_eq!(result.get("claim"), binding.get("claim"));
        assert_eq!(result.get("journal_claim"), binding.get("journal_claim"));
        assert_eq!(result.get("failure_class"), Some(&Value::Null));
        let candidate = result.get("candidate").unwrap();
        assert_eq!(candidate.get("status"), Some(&Value::Num("0".into())));
        assert_eq!(
            candidate
                .get("stdout")
                .unwrap()
                .as_str()
                .unwrap()
                .as_bytes(),
            &[0; 4096]
        );
        if size == 4097 {
            // Independently calculated SHA-256 of 4097 zero bytes.
            assert_eq!(
                candidate.get("stdout_sha256"),
                Some(&Value::Str(
                    "b587fa297299ce9c602e58292b51379402bf7b1074f6b18679c2fb871c917ca8".into()
                ))
            );
        } else {
            assert!(candidate.get("stdout_sha256").is_none());
        }
        fsm_store::store::VerifiedClosure::read(std::path::Path::new(
            result.get("receipt").unwrap().as_str().unwrap(),
        ))
        .unwrap();
        let store = Store::open_read_only(&fixture.store).unwrap();
        assert!(
            store
                .state
                .execution
                .claim_for("instance", &effect)
                .is_some()
        );
        assert!(
            store
                .state
                .execution
                .stopped_for("instance", &effect)
                .is_none()
        );
        drop(store);
        assert!(super::super::super::runner::execute(&fixture.directory, 1).is_err());
        fixture.cleanup().unwrap();
    }
    let table = fsm_core::json::parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/usr/bin/sleep","300"],"timeout_ms":100,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#, &fsm_core::json::JsonLimits::DEFAULT).unwrap();
    let mut fixture = Fixture::new_for_table(table);
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let result = super::super::super::runner::execute(&fixture.directory, 1).unwrap();
    assert_eq!(
        result.get("failure_class"),
        Some(&Value::Str("timeout".into()))
    );
    assert_eq!(
        result.get("candidate").unwrap().get("error"),
        Some(&Value::Str("exec/timeout".into()))
    );
    fsm_store::store::VerifiedClosure::read(std::path::Path::new(
        result.get("receipt").unwrap().as_str().unwrap(),
    ))
    .unwrap();
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    fixture.cleanup().unwrap();
}

fn stop_running_handler() {
    let table = fsm_core::json::parse(
        br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/usr/bin/sleep","300"],"timeout_ms":300000,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#,
        &fsm_core::json::JsonLimits::DEFAULT).unwrap();
    let mut fixture = Fixture::new_for_table(table);
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let (child, _) = launch::begin(
        &fixture.directory,
        1,
        [Stdio::null(), Stdio::null(), Stdio::null()],
    )
    .unwrap();
    let mut gate = Gate {
        fixture: &fixture,
        child,
    };
    let grant = object([
        ("format", Value::Str("fsm.native-entry/1".into())),
        ("claim", binding.get("claim").unwrap().clone()),
        (
            "journal_claim",
            binding.get("journal_claim").unwrap().clone(),
        ),
        (
            "argv",
            Value::Arr(vec![
                Value::Str("/usr/bin/sleep".into()),
                Value::Str("300".into()),
            ]),
        ),
    ]);
    authorize::publish_enrolled(&fixture.directory, &object([("grant", grant)])).unwrap();
    let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
    let pid = super::super::super::number(handoff.get("gate").unwrap(), "pid").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while fs::read_link(format!("/proc/{pid}/exe")).unwrap()
        != std::path::Path::new("/usr/bin/sleep")
    {
        assert!(
            Instant::now() < deadline,
            "approved stop fixture did not exec"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let completion_path = fixture.directory.join("manager-stopped-1.json");
    fs::write(&completion_path, b"existing completion must survive").unwrap();
    assert!(super::super::super::stop::request(&fixture.directory, 1).is_err());
    assert_eq!(
        fs::read(&completion_path).unwrap(),
        b"existing completion must survive"
    );
    assert!(gate.child.try_wait().unwrap().is_none());
    fs::remove_file(&completion_path).unwrap();
    std::os::unix::fs::symlink("missing-completion", &completion_path).unwrap();
    assert!(super::super::super::stop::request(&fixture.directory, 1).is_err());
    assert_eq!(
        fs::read_link(&completion_path).unwrap(),
        std::path::Path::new("missing-completion")
    );
    assert!(gate.child.try_wait().unwrap().is_none());
    fs::remove_file(&completion_path).unwrap();
    let handoff_path = fixture.directory.join("handoff-1.json");
    let handoff_bytes = fs::read(&handoff_path).unwrap();
    fs::write(&handoff_path, b"{}").unwrap();
    let refused = super::broker_cases::refused_close(&fixture.directory);
    assert_eq!(refused.get("ok"), Some(&Value::Bool(false)));
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match super::super::super::observation::read(&fixture.directory, 1) {
            Ok(sample) => {
                assert_eq!(sample.get("domain"), Some(&domain.to_value()));
                assert_eq!(sample.get("closing"), Some(&Value::Bool(true)));
                if sample.get("populated") == Some(&Value::Bool(false)) {
                    break;
                }
            }
            Err(error) => match fs::symlink_metadata(&fixture.groups[0].0) {
                Err(absence) if absence.kind() == std::io::ErrorKind::NotFound => break,
                _ => panic!("original domain inspection failed: {error}"),
            },
        }
        assert!(
            Instant::now() < deadline,
            "refused stop left original tree populated"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::read(&handoff_path).unwrap(), b"{}");
    assert!(!completion_path.exists());
    assert!(!fixture.directory.join("entry-1.json").exists());
    assert!(super::super::super::closure::complete(&fixture.directory, 1).is_err());
    assert!(!fixture.directory.join("closed-1.json").exists());
    fs::write(&handoff_path, handoff_bytes).unwrap();
    super::super::super::stop::request(&fixture.directory, 1).unwrap();
    let completed = read_value(&fixture.directory.join("manager-stopped-1.json"), true).unwrap();
    assert_eq!(
        completed,
        object([
            ("format", Value::Str("fsm.native-manager-stopped/1".into())),
            ("domain", domain.to_value()),
            ("binding", binding.clone()),
            ("gate", handoff.get("gate").unwrap().clone()),
        ])
    );
    assert!(fixture.directory.join("closing-1.json").exists());
    assert!(!fixture.directory.join("entry-1.json").exists());
    assert!(!fixture.directory.join("closed-1.json").exists());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = gate.child.try_wait().unwrap() {
            assert!(!status.success(), "running handler survived manager stop");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "stopped gate transport did not exit"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    let completed_bytes = fs::read(&completion_path).unwrap();
    fs::write(&completion_path, b"{}").unwrap();
    assert!(super::super::super::closure::complete(&fixture.directory, 1).is_err());
    assert!(!fixture.directory.join("closed-1.json").exists());
    fs::write(&completion_path, &completed_bytes).unwrap();
    let run_id = super::super::super::number(binding.get("claim").unwrap(), "run_id").unwrap();
    let receipt_path = fixture.directory.join(format!("closure-1-{run_id}.json"));
    let pending_receipt = receipt_path.with_extension("json.pending");
    fs::write(&pending_receipt, b"partial receipt").unwrap();
    assert!(super::super::super::closure::complete(&fixture.directory, 1).is_err());
    assert_eq!(fs::read(&pending_receipt).unwrap(), b"partial receipt");
    assert!(!receipt_path.exists());
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    fs::remove_file(&pending_receipt).unwrap();
    super::super::super::closure::complete(&fixture.directory, 1).unwrap();
    fsm_store::store::VerifiedClosure::read(&receipt_path).unwrap();
    let receipt_bytes = fs::read(&receipt_path).unwrap();
    let receipt_identity =
        super::super::super::identity(&fs::symlink_metadata(&receipt_path).unwrap());
    assert_eq!(
        fs::symlink_metadata(&receipt_path).unwrap().mode() & 0o222,
        0
    );
    assert_eq!(
        read_value(&fixture.directory.join("closed-1.json"), true).unwrap(),
        object([
            ("format", Value::Str("fsm.native-domain-closed/1".into())),
            ("domain", domain.to_value()),
        ])
    );
    super::super::super::closure::complete(&fixture.directory, 1).unwrap();
    assert_eq!(fs::read(&receipt_path).unwrap(), receipt_bytes);
    assert_eq!(
        super::super::super::identity(&fs::symlink_metadata(&receipt_path).unwrap()),
        receipt_identity
    );
    assert!(
        launch::begin(
            &fixture.directory,
            1,
            [Stdio::null(), Stdio::null(), Stdio::null()]
        )
        .is_err()
    );
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    drop(gate);
    let mut store = Store::open(&fixture.store).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let proof = fsm_store::store::VerifiedClosure::read(&receipt_path).unwrap();
    let outcome = fsm_core::record::execution::StoppedOutcome::from_value(&object([
        ("status", Value::Str("interrupted".into())),
        ("result", Value::Null),
    ]))
    .unwrap();
    store
        .stop_execution_on(
            &mut fsm_store::clock::FixedClock::new(101, 1),
            fsm_store::store::ExecutionStopRequest {
                claim: &claim,
                proof: &proof,
                outcome: &outcome,
                request_id: "native-stop",
                expected_seq: None,
            },
        )
        .unwrap();
    drop(store);
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", &effect)
            .is_some()
    );
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    let successor = fixture.prepare();
    assert_eq!(
        super::super::super::number(&successor, "allocation").unwrap(),
        2
    );
    fixture.cleanup().unwrap();
}
