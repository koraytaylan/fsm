//! Provisioned ordinary CLI workflows; all original scenario assertions remain.
use super::*;
use std::io::{Read, Write};
use std::process::{Command, Stdio};

#[path = "workflow_memory_limits.rs"]
mod memory_limits;

fn workflow_broker(directory: &Path, store: &Path) -> super::broker_cases::Daemon {
    super::broker_cases::disconnect_cases::permit_operator_store(store);
    super::super::super::broker_endpoint::provision(directory, 65534).unwrap();
    super::broker_cases::Daemon::ready(directory, 1)
}

const OPERATIONS: [&str; 7] = [
    "check_prerequisite",
    "check_identity",
    "check_access",
    "check_target",
    "suspend",
    "perform_work",
    "restore",
];

fn stage_artifact(destination: &Path, source_variable: &str, digest_variable: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let source = PathBuf::from(std::env::var_os(source_variable).expect("exact built artifact"));
    let expected = std::env::var(digest_variable).expect("frozen artifact digest");
    let mut original = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000 | 0o4000)
        .open(source)
        .unwrap();
    let metadata = original.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 64 * 1024 * 1024);
    let mut bytes = Vec::new();
    Read::by_ref(&mut original)
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 64 * 1024 * 1024);
    assert_eq!(
        fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(&bytes)),
        expected
    );
    // No unverified source bytes are published into this readable protected
    // directory, even if a user replaces or edits the original Cargo artifact.
    let mut installed = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(destination)
        .unwrap();
    installed.write_all(&bytes).unwrap();
    installed
        .set_permissions(fs::Permissions::from_mode(0o555))
        .unwrap();
    installed.sync_all().unwrap();
    fs::File::open(destination.parent().unwrap())
        .unwrap()
        .sync_all()
        .unwrap();
    let metadata = installed.metadata().unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), 0);
    assert_eq!(metadata.mode() & 0o7777, 0o555);
}

fn table(helper: &Path, resource: &Path, failures: &str) -> Value {
    object([
        ("format", Value::Str("fsm.handlers/1".into())),
        (
            "handlers",
            Value::Arr(
                OPERATIONS
                    .iter()
                    .map(|operation| {
                        object([
                            ("effect", Value::Str((*operation).into())),
                            (
                                "argv",
                                Value::Arr(
                                    [
                                        helper.to_str().unwrap().to_owned(),
                                        "workflow_handler".into(),
                                        "--exact".into(),
                                        "--nocapture".into(),
                                        format!("handler-operation={operation}"),
                                        "handler-resource={resource}".into(),
                                        "handler-run={run}".into(),
                                        format!("handler-directory={}", resource.display()),
                                        format!("handler-failures={failures}"),
                                    ]
                                    .into_iter()
                                    .map(Value::Str)
                                    .collect(),
                                ),
                            ),
                            ("timeout_ms", Value::Num("30000".into())),
                            (
                                "on_ok",
                                object([("event", Value::Str(format!("{operation}_ok")))]),
                            ),
                            (
                                "on_failed",
                                object([("event", Value::Str(format!("{operation}_failed")))]),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn verify_native_runs(fixture: &Fixture, failure: &str, staging: &Path) {
    use fsm_core::record::{RecordKind, execution::Claim};
    use fsm_store::store::VerifiedClosure;
    let expected = if failure == "suspend" {
        6
    } else {
        OPERATIONS[..4]
            .iter()
            .position(|operation| *operation == failure)
            .map_or(7, |index| index + 1)
    };
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(store.state.execution.unresolved().count(), 0);
    let last = number(&fixture.counter(), "last_allocation").unwrap();
    if failure == "race" {
        assert!((expected as u64..=4096).contains(&last));
    } else {
        assert_eq!(last, expected as u64);
    }
    let mut claimed = std::collections::BTreeSet::new();
    for kind in [
        RecordKind::ExecutionClaimed,
        RecordKind::ExecutionStopped,
        RecordKind::ExecutionSettled,
    ] {
        assert_eq!(
            store
                .records
                .iter()
                .filter(|record| record.kind == kind)
                .count(),
            expected
        );
    }
    for record in store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionSettled)
    {
        assert_eq!(
            record.body.get("disposition").and_then(Value::as_str),
            Some("acked")
        );
    }
    for record in store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionClaimed)
    {
        let mut material = record.body.as_obj().unwrap().clone();
        material.remove("request_id");
        material.remove("request_fp");
        let claim = Claim::from_value(&Value::Obj(material)).unwrap();
        let domain = claim.domain().to_value();
        assert_eq!(
            text(&domain, "namespace").unwrap(),
            fixture
                .directory
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert_eq!(number(&domain, "generation").unwrap(), 1);
        let allocation = number(&domain, "allocation").unwrap();
        assert!((1..=last).contains(&allocation) && claimed.insert(allocation));
        memory_limits::verify(fixture, &domain, allocation);
        let proof = VerifiedClosure::read(
            &fixture
                .directory
                .join(format!("closure-{allocation}-{}.json", claim.run_id())),
        )
        .unwrap();
        assert!(proof.matches_claim(&claim, &format!("sha256:{}", record.hash)));
        proof.check_store(&fixture.store).unwrap();
        assert!(
            !cgroup(&origin(&fixture.directory).unwrap(), allocation)
                .unwrap()
                .exists()
        );
    }
    for allocation in (1..=last).filter(|allocation| !claimed.contains(allocation)) {
        verify_unused_domain(fixture, allocation, staging);
    }
}

fn verify_unused_domain(fixture: &Fixture, allocation: u64, staging: &Path) {
    let domain =
        super::super::super::closing::recorded_domain(&fixture.directory, allocation).unwrap();
    let namespace = text(&domain, "namespace").unwrap();
    // Retain original cleanup bytes before a verifier refusal can tear down
    // this otherwise closed fixture; absence alone is never retirement proof.
    for prefix in ["prepared", "closing", "closed"] {
        let source = fixture
            .directory
            .join(format!("{prefix}-{allocation}.json"));
        let metadata = fs::symlink_metadata(&source).unwrap();
        assert!(metadata.is_file() && metadata.uid() == 0 && metadata.len() <= 65536);
        let mut destination = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging.join(format!("unused-{namespace}-{allocation}-{prefix}.json")))
            .unwrap();
        destination.write_all(&fs::read(&source).unwrap()).unwrap();
        destination.sync_all().unwrap();
    }
    assert_eq!(
        read_value(
            &fixture.directory.join(format!("closing-{allocation}.json")),
            true
        )
        .unwrap(),
        object([
            ("format", Value::Str("fsm.native-closing/1".into())),
            ("domain", domain.clone())
        ])
    );
    assert_eq!(
        read_value(
            &fixture.directory.join(format!("closed-{allocation}.json")),
            true
        )
        .unwrap(),
        object([
            ("format", Value::Str("fsm.native-domain-closed/1".into())),
            ("domain", domain.clone())
        ])
    );
    for prefix in [
        "binding",
        "launch",
        "handoff",
        "entry",
        "exec-status",
        "manager-stopped",
        "manager-retired",
        "fixture-memory",
    ] {
        for suffix in ["json", "json.pending"] {
            assert!(
                !fixture
                    .directory
                    .join(format!("{prefix}-{allocation}.{suffix}"))
                    .try_exists()
                    .unwrap()
            );
        }
    }
    assert!(
        !cgroup(&origin(&fixture.directory).unwrap(), allocation)
            .unwrap()
            .try_exists()
            .unwrap()
    );
    let unit = format!("fsm-containment-{namespace}-1-{allocation}.service");
    assert!(
        super::super::super::manager::retired(&unit, Instant::now() + Duration::from_secs(2))
            .unwrap()
    );
}

pub(super) fn run() {
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let seed = format!("{}-{:?}", std::process::id(), std::time::SystemTime::now());
    let nonce = fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(seed.as_bytes()));
    let staging = PathBuf::from(format!("/usr/libexec/fsm-workflow-{}", &nonce[..32]));
    super::super::protected_directory(staging.parent().unwrap()).unwrap();
    fs::DirBuilder::new().mode(0o755).create(&staging).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).unwrap();
    let helper = staging.join("workflow-test");
    let cli = staging.join("fsm");
    stage_artifact(
        &helper,
        "FSM_NATIVE_WORKFLOW_TEST_ARTIFACT",
        "FSM_NATIVE_WORKFLOW_TEST_SHA256",
    );
    stage_artifact(
        &cli,
        "FSM_NATIVE_WORKFLOW_CLI_ARTIFACT",
        "FSM_NATIVE_WORKFLOW_CLI_SHA256",
    );
    for (group, (case, failures)) in [
        (
            "discovered_handlers_complete_the_workflow_in_order",
            vec![""],
        ),
        (
            "each_failed_preflight_stops_before_external_changes",
            OPERATIONS[..4].to_vec(),
        ),
        (
            "failures_after_suspension_restore_the_resource",
            vec!["suspend", "perform_work"],
        ),
        (
            "cleanup_failures_are_explicit_after_success_or_partial_work",
            vec!["restore", "perform_work,restore"],
        ),
        (
            "standalone_and_embedded_exclude_a_live_handler_tree",
            vec!["race"],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut fixtures = Vec::new();
        let mut brokers = Vec::new();
        let mut resources = Vec::new();
        let mut manifest = Vec::new();
        for (index, failure) in failures.iter().enumerate() {
            let resource = PathBuf::from(format!(
                "/dev/shm/fsm-workflow-{}-{case}-{index}",
                &nonce[..32]
            ));
            fs::DirBuilder::new().mode(0o700).create(&resource).unwrap();
            fs::set_permissions(&resource, fs::Permissions::from_mode(0o700)).unwrap();
            // Distinct DynamicUser allocations append the same external log;
            // shared fixture files are writable, while executable/catalogue and
            // operator store remain protected by their separate ownership.
            for (name, bytes) in [
                ("phase", b"active".as_slice()),
                ("calls", b"".as_slice()),
                (".work-template", b"".as_slice()),
            ] {
                let path = resource.join(name);
                fs::write(&path, bytes).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o666)).unwrap();
            }
            // Publish writability only after privileged initialization.
            fs::set_permissions(&resource, fs::Permissions::from_mode(0o777)).unwrap();
            let home = PathBuf::from(format!("/dev/shm/fwh-{}-{group}-{index}", &nonce[..16]));
            fs::DirBuilder::new().mode(0o700).create(&home).unwrap();
            fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
            std::os::unix::fs::chown(&home, Some(65534), Some(65534)).unwrap();
            assert!(
                home.join(".cache/fsm/control/c-0123456789abcdef/s")
                    .as_os_str()
                    .as_encoded_bytes()
                    .len()
                    <= 107
            );
            let fixture = Fixture::new_for_operator(table(&helper, &resource, failure));
            let limits = memory_limits::Limits::install(&fixture);
            brokers.push(workflow_broker(&fixture.directory, &fixture.store));
            let resource_identity = identity(&fs::symlink_metadata(&resource).unwrap());
            let home_identity = identity(&fs::symlink_metadata(&home).unwrap());
            manifest.push(object([
                (
                    "directory",
                    Value::Str(fixture.store.to_str().unwrap().into()),
                ),
                ("store", Value::Str(fixture.store.to_str().unwrap().into())),
                ("resource", Value::Str(resource.to_str().unwrap().into())),
                ("cli", Value::Str(cli.to_str().unwrap().into())),
                ("home", Value::Str(home.to_str().unwrap().into())),
                ("resource_identity", resource_identity.clone()),
                ("home_identity", home_identity.clone()),
                ("memory_limits", limits.inventory()),
            ]));
            fixtures.push(fixture);
            resources.push((resource, home, resource_identity, home_identity, limits));
        }
        let manifest_path = fixtures[0].directory.join("fixture-workflow.json");
        fs::write(&manifest_path, canon_bytes(&Value::Arr(manifest))).unwrap();
        fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o444)).unwrap();
        // Protected recovery inventory survives a failed scenario even when
        // the normal matched namespace teardown has already completed.
        let inventory = staging.join(format!("{case}.inventory.json"));
        fs::write(&inventory, fs::read(&manifest_path).unwrap()).unwrap();
        fs::set_permissions(inventory, fs::Permissions::from_mode(0o600)).unwrap();
        let log_path = staging.join(format!("{case}.log"));
        let log = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&log_path)
            .unwrap();
        let mut child = Command::new("/usr/bin/python3")
            .args(["-c", "import os,sys;os.setgroups([]);os.setgid(65534);os.setuid(65534);os.execv(sys.argv[1],sys.argv[1:])"])
            .arg(&helper).args(["--exact", case, "--ignored", "--nocapture", "--color", "never"])
            .env("FSM_NATIVE_WORKFLOW_MANIFEST", &manifest_path)
            .env("TMPDIR", &fixtures[0].store)
            .stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log)
            .spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30 * failures.len() as u64 + 20);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "native workflow parent timeout; retain fixture authority and staging {}",
                    staging.display()
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut output = Vec::new();
        fs::File::open(&log_path)
            .unwrap()
            .take(65537)
            .read_to_end(&mut output)
            .unwrap();
        assert!(output.len() <= 65536);
        assert!(
            status.success(),
            "ordinary native workflow {case}: {}",
            String::from_utf8_lossy(&output)
        );
        assert!(String::from_utf8_lossy(&output).contains("1 passed; 0 failed; 0 ignored;"));
        // No absence is promoted into a production closure receipt: these are
        // matched test teardown guards after original workflow/journal assertions.
        for (fixture, failure) in fixtures.iter().zip(&failures) {
            memory_limits::archive(fixture, &staging);
            verify_native_runs(fixture, failure, &staging);
        }
        let mut report_output = std::io::stdout().lock();
        writeln!(
            report_output,
            "FSM_NATIVE_WORKFLOW_CASE {case} {}",
            failures.len()
        )
        .unwrap();
        drop(report_output);
        drop(brokers);
        for (mut fixture, (resource, home, resource_identity, home_identity, limits)) in
            fixtures.into_iter().zip(resources)
        {
            let resource_metadata = fs::symlink_metadata(&resource).unwrap();
            let home_metadata = fs::symlink_metadata(&home).unwrap();
            assert!(resource_metadata.is_dir() && home_metadata.is_dir());
            assert_eq!(identity(&resource_metadata), resource_identity);
            assert_eq!(identity(&home_metadata), home_identity);
            assert_eq!(resource_metadata.uid(), 0);
            assert_eq!(home_metadata.uid(), 65534);
            fixture
                .cleanup()
                .expect("retain unknown or surviving original domain");
            fs::remove_dir_all(resource).unwrap();
            fs::remove_dir_all(home).unwrap();
            limits.retire();
        }
    }
    // Only complete matched teardown permits retiring the staged helper bytes.
    fs::remove_dir_all(staging).unwrap();
}
