//! Protected Root staging for the dedicated production-host crash observer.

use super::*;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

#[derive(Clone, Copy)]
struct Scenario {
    host: &'static str,
    kind: &'static str,
    behavior: &'static str,
}

#[test]
#[ignore = "requires disposable native CI and exact staged lifecycle artifacts"]
fn provisioned_lifecycle_candidate_matrix() {
    assert_eq!(
        std::env::var("FSM_NATIVE_FIXTURE_DISPOSABLE").as_deref(),
        Ok("1")
    );
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let seed = format!("{}-{:?}", std::process::id(), std::time::SystemTime::now());
    let nonce = fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(seed.as_bytes()));
    let staging = PathBuf::from(format!("/usr/libexec/fsm-crash-{}", &nonce[..24]));
    fs::DirBuilder::new().mode(0o755).create(&staging).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o755)).unwrap();
    for (name, variable) in [
        ("matrix-test", "TEST"),
        ("fixture", "FIXTURE"),
        ("fsm", "CLI"),
    ] {
        super::workflow_cases::stage_artifact(
            &staging.join(name),
            &format!("FSM_CRASH_{variable}_ARTIFACT"),
            &format!("FSM_CRASH_{variable}_SHA256"),
        );
    }
    for host in ["standalone", "embedded"] {
        for kind in ["process", "mcp"] {
            for behavior in ["hold-result", "noisy-result", "collected-timeout"] {
                scenario(
                    &staging,
                    &nonce[..24],
                    Scenario {
                        host,
                        kind,
                        behavior,
                    },
                );
            }
        }
    }
    fs::remove_dir_all(staging).unwrap();
}

fn scenario(staging: &Path, nonce: &str, case: Scenario) {
    let Scenario {
        host,
        kind,
        behavior,
    } = case;
    let resource = PathBuf::from(format!("/dev/shm/fsm-crash-{nonce}-{host}-{kind}"));
    fs::DirBuilder::new().mode(0o777).create(&resource).unwrap();
    fs::set_permissions(&resource, fs::Permissions::from_mode(0o777)).unwrap();
    // DynamicUser RemoveIPC can unlink files owned by the departing identity;
    // Root-owned observation slots survive closure and remain writable by runs.
    for role in ["root", "child", "grandchild"] {
        let path = resource.join(format!("{role}-entered"));
        fs::write(&path, b"").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    }
    let resource_identity = identity(&fs::symlink_metadata(&resource).unwrap());
    // Leave room for the longest control label and its private socket suffix.
    let home = PathBuf::from(format!(
        "/dev/shm/fc-{}-{}-{}",
        &nonce[..12],
        &host[..1],
        &kind[..1]
    ));
    assert!(
        home.join("immediate-restart-control/c-0123456789abcdef/s")
            .as_os_str()
            .len()
            < 108
    );
    fs::DirBuilder::new().mode(0o700).create(&home).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
    std::os::unix::fs::chown(&home, Some(65534), Some(65534)).unwrap();
    let home_identity = identity(&fs::symlink_metadata(&home).unwrap());
    let catalogue = table(&staging.join("fixture"), &resource, case);
    let mut fixture = Fixture::new_for_workflow(catalogue.clone());
    if behavior == "collected-timeout" {
        let request = fixture.directory.join("crash-candidate-barrier.json");
        fs::write(
            &request,
            canon_bytes(&object([("attempt", Value::Num("1".into()))])),
        )
        .unwrap();
        fs::set_permissions(request, fs::Permissions::from_mode(0o444)).unwrap();
    }
    let limits = memory_limits::Limits::install(&fixture);
    super::broker_cases::disconnect_cases::permit_operator_store(&fixture.store);
    super::super::super::broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    let broker = super::broker_cases::Daemon::ready(&fixture.directory, 1);
    let handlers = fixture.store.join("handlers.json");
    fs::write(&handlers, canon_bytes(&catalogue)).unwrap();
    fs::set_permissions(&handlers, fs::Permissions::from_mode(0o444)).unwrap();
    let manifest = fixture.directory.join("crash-matrix.json");
    fs::write(
        &manifest,
        canon_bytes(&object([
            ("store", Value::Str(fixture.store.to_str().unwrap().into())),
            ("resource", Value::Str(resource.to_str().unwrap().into())),
            ("home", Value::Str(home.to_str().unwrap().into())),
            (
                "cli",
                Value::Str(staging.join("fsm").to_str().unwrap().into()),
            ),
            ("host", Value::Str(host.into())),
            ("kind", Value::Str(kind.into())),
            ("behavior", Value::Str(behavior.into())),
            (
                "authority",
                Value::Str(fixture.directory.to_str().unwrap().into()),
            ),
        ])),
    )
    .unwrap();
    fs::set_permissions(&manifest, fs::Permissions::from_mode(0o444)).unwrap();
    let log_path = staging.join(format!("{host}-{kind}-{behavior}.log"));
    let log = fs::File::create(&log_path).unwrap();
    let mut actor = Command::new("/usr/bin/python3")
        .args(["-c", "import os,sys;os.setgroups([]);os.setgid(65534);os.setuid(65534);os.execv(sys.argv[1],sys.argv[1:])"])
        .arg(staging.join("matrix-test"))
        .args(["--exact", "native::production_candidate_result_crash_retains_original_tree_until_verified_closure", "--ignored", "--nocapture", "--color", "never"])
        .env("FSM_LIFECYCLE_NATIVE_MANIFEST", &manifest).env("TMPDIR", &fixture.store)
        .stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(80);
    let status = loop {
        if let Some(status) = actor.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = actor.kill();
            let _ = actor.wait();
            super::workflow_cases::archive_failure(&fixture, staging);
            panic!(
                "candidate matrix timeout; retain {} and {}",
                fixture.directory.display(),
                staging.display()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // Preserve the original observations before any verdict assertion can panic.
    super::workflow_cases::archive_failure(&fixture, staging);
    let mut output = Vec::new();
    fs::File::open(log_path)
        .unwrap()
        .take(65_537)
        .read_to_end(&mut output)
        .unwrap();
    assert!(output.len() <= 65_536);
    assert!(
        status.success(),
        "candidate matrix {host}/{kind}: {}",
        String::from_utf8_lossy(&output)
    );
    assert!(String::from_utf8_lossy(&output).contains("1 passed; 0 failed; 0 ignored;"));
    verify(&fixture, behavior);
    memory_limits::archive(&fixture, staging);
    writeln!(
        std::io::stdout().lock(),
        "FSM_NATIVE_CRASH_CASE candidate-result {host} {kind} {behavior}"
    )
    .unwrap();
    drop(broker);
    assert_eq!(
        identity(&fs::symlink_metadata(&resource).unwrap()),
        resource_identity
    );
    assert_eq!(
        identity(&fs::symlink_metadata(&home).unwrap()),
        home_identity
    );
    fixture
        .cleanup()
        .expect("retain unknown or surviving original domain");
    limits.retire();
    fs::remove_dir_all(resource).unwrap();
    fs::remove_dir_all(home).unwrap();
    // Retired successful namespaces need no failure snapshot; keep the bounded
    // export inventory available for a later failed scenario's original state.
    let namespace = fixture
        .directory
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let prefix = format!("failure-{namespace}-");
    for entry in fs::read_dir(staging).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_str().unwrap().starts_with(&prefix) {
            fs::remove_file(entry.path()).unwrap();
        }
    }
}

fn table(executable: &Path, resource: &Path, case: Scenario) -> Value {
    let Scenario { kind, behavior, .. } = case;
    let mut handler = BTreeMap::from([
        ("effect".into(), Value::Str("notify".into())),
        ("kind".into(), Value::Str(kind.into())),
        (
            "argv".into(),
            Value::Arr(
                [
                    executable.to_str().unwrap(),
                    kind,
                    resource.to_str().unwrap(),
                    if behavior == "collected-timeout" {
                        "hold-result"
                    } else {
                        behavior
                    },
                ]
                .into_iter()
                .map(|value| Value::Str(value.into()))
                .collect(),
            ),
        ),
        ("timeout_ms".into(), Value::Num("3000".into())),
        (
            "retry".into(),
            parse(
                br#"{"attempts":2,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]}"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
        ),
        (
            "on_ok".into(),
            object([("event", Value::Str("done".into()))]),
        ),
    ]);
    if kind == "mcp" {
        handler.insert("tool".into(), Value::Str("run".into()));
        handler.insert("arguments".into(), object([]));
    }
    object([
        ("format", Value::Str("fsm.handlers/1".into())),
        ("handlers", Value::Arr(vec![Value::Obj(handler)])),
    ])
}

fn verify(fixture: &Fixture, behavior: &str) {
    use fsm_core::record::{RecordKind, execution::Claim};
    use fsm_store::store::VerifiedClosure;
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(store.state.execution.unresolved().count(), 0);
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
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
            2
        );
    }
    for record in store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionClaimed)
    {
        let mut fields = record.body.as_obj().unwrap().clone();
        fields.remove("request_id");
        fields.remove("request_fp");
        let claim = Claim::from_value(&Value::Obj(fields)).unwrap();
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
        memory_limits::verify(fixture, &domain, number(&domain, "allocation").unwrap());
        let proof = VerifiedClosure::read(&fixture.directory.join(format!(
            "closure-{}-{}.json",
            number(&domain, "allocation").unwrap(),
            claim.run_id()
        )))
        .unwrap();
        assert!(proof.matches_claim(&claim, &format!("sha256:{}", record.hash)));
        proof.check_store(&fixture.store).unwrap();
        if behavior == "noisy-result" && number(&claim.to_value(), "attempt").unwrap() == 2 {
            let response = read_value(
                &fixture.directory.join(format!(
                    "completed-{}-{}.json",
                    number(&domain, "allocation").unwrap(),
                    claim.run_id()
                )),
                true,
            )
            .unwrap();
            let candidate = response.get("result").unwrap().get("candidate").unwrap();
            assert_eq!(candidate.get("stderr"), Some(&Value::Str("n".repeat(4096))));
            assert_eq!(
                candidate.get("stderr_sha256"),
                Some(&Value::Str(fsm_core::sha256::to_hex(
                    &fsm_core::sha256::sha256(&[b'n'; 16_384])
                )))
            );
        }
    }
}
