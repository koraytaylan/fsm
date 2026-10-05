//! Actual provisioned binary and independent unprivileged socket clients.

use super::super::super::{broker_endpoint, identity, number, object, read_value, text};
use super::{Fixture, cgroup, claim_binding, origin};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[path = "broker_disconnect_native_tests.rs"]
mod disconnect_cases;

pub(super) fn disconnect() {
    disconnect_cases::run();
}

const CLIENT: &str = r#"import errno,json,os,socket,sys
uid=int(sys.argv[1])
os.setgroups([])
os.setgid(uid)
os.setuid(uid)
assert os.getuid()==uid and os.geteuid()==uid and os.getgroups()==[]
route=json.load(open(sys.argv[2]+'/route.json'))
path=sys.argv[2]+'/s-'+str(route['epoch'])
s=socket.socket(socket.AF_UNIX)
s.settimeout(5)
if sys.argv[3]=='deny':
    try:
        s.connect(path)
    except OSError as error:
        assert error.errno==errno.EACCES
        sys.exit(0)
    raise AssertionError('unauthorized client connected')
from pathlib import Path
base=Path(sys.argv[2])
encoded=sys.argv[3].encode()
reader,writer=os.pipe()
framed=len(encoded).to_bytes(4,'big')+encoded
while framed:
    count=os.write(writer,framed)
    framed=framed[count:]
os.close(writer)
os.dup2(reader,0)
os.close(reader)
authority=base.parent
namespace=authority.parent.name
generation=authority.name.removeprefix('authority-')
os.execv('/usr/libexec/fsm-containment-authority',['fsm-containment-authority','client',namespace,generation])
"#;

struct Daemon(Child);

impl Daemon {
    fn start(directory: &Path) -> Self {
        let namespace = directory.parent().unwrap().file_name().unwrap();
        Self(
            Command::new("/usr/bin/python3")
                .args([
                    "-c",
                    "import os,sys;os.umask(0o077);os.execv(sys.argv[1],sys.argv[1:])",
                ])
                .arg("/usr/libexec/fsm-containment-authority")
                .arg("serve")
                .arg(namespace)
                .arg("1")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    fn ready(directory: &Path, epoch: u64) -> Self {
        let mut daemon = Self::start(directory);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(
                daemon.0.try_wait().unwrap().is_none(),
                "broker exited before publication"
            );
            if read_value(&directory.join("broker/route.json"), true)
                .is_ok_and(|route| number(&route, "epoch").unwrap() == epoch)
            {
                return daemon;
            }
            assert!(
                Instant::now() < deadline,
                "broker route publication timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn refused(directory: &Path) {
        let mut daemon = Self::start(directory);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = daemon.0.try_wait().unwrap() {
                assert!(!status.success());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "uncertain broker startup was not refused"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

fn invoke_client(base: &Path, uid: u32, request: &str) -> std::process::Output {
    Command::new("/usr/bin/python3")
        .args(["-c", CLIENT])
        .arg(uid.to_string())
        .arg(base)
        .arg(request)
        .output()
        .unwrap()
}

fn client(base: &Path, uid: u32, request: &str) -> Vec<u8> {
    let result = invoke_client(base, uid, request);
    assert!(
        result.status.success(),
        "unprivileged broker client failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    if request == "deny" {
        return result.stdout;
    }
    assert!(result.stdout.len() >= 4 && result.stdout.len() <= 65540);
    let length = u32::from_be_bytes(result.stdout[..4].try_into().unwrap()) as usize;
    assert_eq!(length, result.stdout.len() - 4);
    result.stdout[4..].to_vec()
}

fn request(base: &Path, action: &str, payload: Value) -> Value {
    let request = object([
        ("format", Value::Str("fsm.native-request/1".into())),
        ("action", Value::Str(action.into())),
        ("payload", payload),
    ]);
    let bytes = client(
        base,
        65534,
        std::str::from_utf8(&canon_bytes(&request)).unwrap(),
    );
    parse(&bytes, &JsonLimits::DEFAULT).unwrap()
}

pub(super) fn run() {
    for timeout in [false, true] {
        run_case(timeout);
    }
}

fn run_case(timeout: bool) {
    let mut table = parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000,"on_ok":{"event":"docs_ok"},"on_failed":{"event":"note_added","payload":{"text":"original"}},"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#, &JsonLimits::DEFAULT).unwrap();
    if timeout {
        let Value::Obj(document) = &mut table else {
            unreachable!()
        };
        let Value::Arr(handlers) = document.get_mut("handlers").unwrap() else {
            unreachable!()
        };
        let Value::Obj(handler) = &mut handlers[0] else {
            unreachable!()
        };
        handler.insert(
            "argv".into(),
            Value::Arr(vec![
                Value::Str("/bin/sleep".into()),
                Value::Str("300".into()),
            ]),
        );
    }
    let mut fixture = Fixture::new_for_operator(table);
    for uid in [0, 61184, 65519, u32::MAX] {
        assert!(broker_endpoint::provision(&fixture.directory, uid).is_err());
        assert!(!fixture.directory.join("broker").exists());
    }
    broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    assert!(broker_endpoint::provision(&fixture.directory, 65534).is_err());
    let base = fixture.directory.join("broker");
    let counter_path = base.join("counter.json");
    let zero = fs::read(&counter_path).unwrap();
    let saved = base.join("fixture-counter.saved");
    fs::rename(&counter_path, &saved).unwrap();
    Daemon::refused(&fixture.directory);
    assert!(!base.join("epoch-1.json").exists());
    fs::rename(&saved, &counter_path).unwrap();
    let mut daemon = Daemon::ready(&fixture.directory, 1);
    let first_socket = base.join("s-1");
    let first_identity = identity(&fs::symlink_metadata(&first_socket).unwrap());
    assert_eq!(fs::symlink_metadata(&first_socket).unwrap().uid(), 65534);
    assert_eq!(
        fs::symlink_metadata(&first_socket).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(base.join("route.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o444
    );
    let counter = fs::read(&counter_path).unwrap();
    Daemon::refused(&fixture.directory);
    assert_eq!(fs::read(&counter_path).unwrap(), counter);
    assert!(client(&base, 65533, "deny").is_empty());
    assert!(client(&base, 61184, "deny").is_empty());
    let prohibited = object([
        ("format", Value::Str("fsm.native-request/1".into())),
        ("action", Value::Str("authorize".into())),
        ("payload", Value::Null),
    ]);
    let refused = invoke_client(
        &base,
        65534,
        std::str::from_utf8(&canon_bytes(&prohibited)).unwrap(),
    );
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("outside policy"));
    assert_eq!(fs::read(&counter_path).unwrap(), counter);
    let prepared = disconnect_cases::prepare(&fixture.directory);
    let domain = NativeDomain::from_value(&prepared).unwrap();
    assert_eq!(number(&domain.to_value(), "allocation").unwrap(), 1);
    let group = cgroup(&origin(&fixture.directory).unwrap(), 1).unwrap();
    fixture
        .groups
        .push((group, domain.to_value().get("cgroup").unwrap().clone()));
    let (binding, effect) = claim_binding(&fixture, &domain);
    let settlement_request = fsm_execute::rid::ack_rid(&effect);
    let successor = NativeDomain::from_value(&fixture.prepare()).unwrap();
    permit_operator_store(&fixture.store);
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    let execution = disconnect_cases::complete(&fixture.directory, &binding, timeout, &successor);
    assert_eq!(
        read_value(&fixture.directory.join("binding-1.json"), true).unwrap(),
        binding
    );
    assert_eq!(
        execution.get("ok"),
        Some(&Value::Bool(true)),
        "{execution:?}"
    );
    let original_claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let completion = fsm_execute::run::native_client::NativeCompletion::verify(
        &execution,
        &original_claim,
        text(&binding, "journal_claim").unwrap(),
    )
    .unwrap();
    let mut forged = execution.clone();
    let Value::Obj(frame) = &mut forged else {
        panic!("response object")
    };
    let Some(Value::Obj(result)) = frame.get_mut("result") else {
        panic!("result object")
    };
    let Some(Value::Obj(candidate)) = result.get_mut("candidate") else {
        panic!("candidate object")
    };
    candidate.insert("stdout".into(), Value::Str("forged captured output".into()));
    let error = match fsm_execute::run::native_client::NativeCompletion::verify(
        &forged,
        &original_claim,
        text(&binding, "journal_claim").unwrap(),
    ) {
        Err(error) => error,
        Ok(_) => panic!("changed candidate reused original closure proof"),
    };
    assert!(error.contains("attestation"), "{error}");
    let attestation = fixture
        .directory
        .join(format!("result-1-{}.json", original_claim.run_id()));
    let metadata = fs::symlink_metadata(&attestation).unwrap();
    assert_eq!(metadata.uid(), 0);
    assert_eq!(metadata.mode() & 0o777, 0o444);
    let attested_bytes = fs::read(&attestation).unwrap();
    let saved_attestation = fixture.directory.join("fixture-result-attestation.saved");
    fs::rename(&attestation, &saved_attestation).unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    fs::write(&attestation, b"{").unwrap();
    fs::set_permissions(&attestation, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    assert_eq!(fs::read(&attestation).unwrap(), b"{");
    fs::remove_file(&attestation).unwrap();
    std::os::unix::fs::symlink(&saved_attestation, &attestation).unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    fs::remove_file(&attestation).unwrap();
    fs::rename(&saved_attestation, &attestation).unwrap();
    fs::set_permissions(&attestation, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    fs::set_permissions(&attestation, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(fs::read(&attestation).unwrap(), attested_bytes);
    assert_eq!(request(&base, "recover", Value::Num("1".into())), execution);
    assert_eq!(
        completion.candidate().get("status"),
        Some(&Value::Num(if timeout { "-1" } else { "0" }.into()))
    );
    assert_eq!(
        completion.failure_class(),
        timeout.then_some(fsm_core::record::execution::FailureClass::Timeout)
    );
    assert_eq!(
        completion.stopped_outcome().status(),
        if timeout { "timeout" } else { "ok" }
    );
    assert!(
        completion
            .proof()
            .matches_claim(&original_claim, text(&binding, "journal_claim").unwrap())
    );
    let completed = fixture
        .directory
        .join(format!("completed-1-{}.json", original_claim.run_id()));
    let completed_metadata = fs::symlink_metadata(&completed).unwrap();
    assert_eq!(completed_metadata.uid(), 0);
    assert_eq!(completed_metadata.mode() & 0o777, 0o600);
    let completed_bytes = fs::read(&completed).unwrap();
    // Recovery must use durable original material, even without a live catalogue.
    let catalogue = fixture.directory.join("catalogue.json");
    let saved_catalogue = fixture.directory.join("fixture-catalogue.saved");
    let allocation_counter = fs::read(fixture.directory.join("counter.json")).unwrap();
    fs::rename(&catalogue, &saved_catalogue).unwrap();
    assert_eq!(request(&base, "recover", Value::Num("1".into())), execution);
    assert_eq!(
        request(&base, "prepare", Value::Null).get("ok"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        fs::read(fixture.directory.join("counter.json")).unwrap(),
        allocation_counter
    );
    assert!(daemon.0.try_wait().unwrap().is_none());
    assert_eq!(fs::read(&counter_path).unwrap(), counter);
    fs::write(&catalogue, b"{").unwrap();
    assert_eq!(request(&base, "recover", Value::Num("1".into())), execution);
    assert_eq!(
        request(&base, "prepare", Value::Null).get("ok"),
        Some(&Value::Bool(false))
    );
    assert_eq!(
        fs::read(fixture.directory.join("counter.json")).unwrap(),
        allocation_counter
    );
    assert!(daemon.0.try_wait().unwrap().is_none());
    fs::remove_file(&catalogue).unwrap();
    fs::rename(&saved_catalogue, &catalogue).unwrap();
    let saved_completed = fixture.directory.join("fixture-completed.saved");
    fs::rename(&completed, &saved_completed).unwrap();
    fs::write(&completed, b"{").unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    assert_eq!(fs::read(&completed).unwrap(), b"{");
    fs::remove_file(&completed).unwrap();
    fs::rename(&saved_completed, &completed).unwrap();
    assert_eq!(fs::read(&completed).unwrap(), completed_bytes);
    let public_identity = fixture.directory.join("store-identity.json");
    let identity_metadata = fs::symlink_metadata(&public_identity).unwrap();
    assert_eq!(identity_metadata.uid(), 0);
    assert_eq!(identity_metadata.mode() & 0o777, 0o444);
    let identity_bytes = fs::read(&public_identity).unwrap();
    let saved_identity = fixture.directory.join("fixture-store-identity.saved");
    fs::rename(&public_identity, &saved_identity).unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    fs::write(&public_identity, b"{").unwrap();
    fs::set_permissions(&public_identity, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        request(&base, "recover", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
    assert_eq!(fs::read(&public_identity).unwrap(), b"{");
    fs::remove_file(&public_identity).unwrap();
    fs::rename(&saved_identity, &public_identity).unwrap();
    assert_eq!(fs::read(&public_identity).unwrap(), identity_bytes);
    assert_eq!(request(&base, "recover", Value::Num("1".into())), execution);
    let result = execution.get("result").unwrap();
    assert_eq!(result.get("claim"), binding.get("claim"));
    assert_eq!(result.get("journal_claim"), binding.get("journal_claim"));
    assert_eq!(
        result.get("candidate").unwrap().get("status"),
        Some(&Value::Num(if timeout { "-1" } else { "0" }.into()))
    );
    VerifiedClosure::read(Path::new(text(result, "receipt").unwrap())).unwrap();
    assert_eq!(
        request(&base, "execute", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
    );
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
    {
        let mut store = Store::open(&fixture.store).unwrap();
        let before = store.records.len();
        let before_ownership = store.state.execution.clone();
        let mut stale = original_claim.to_value().as_obj().unwrap().clone();
        stale.insert(
            "run_id".into(),
            Value::Num((original_claim.run_id() + 1).to_string()),
        );
        let stale = fsm_core::record::execution::Claim::from_value(&Value::Obj(stale)).unwrap();
        let mut pipeline = fsm_execute::run::Pipeline;
        let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
        assert_eq!(
            pipeline
                .settle_native_stopped(&mut store, &mut clock, &original_claim, &completion,)
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(store.state.execution, before_ownership);
        assert!(
            pipeline
                .stop_native(
                    &mut store,
                    &mut clock,
                    &stale,
                    &completion,
                    "native-stale-stop"
                )
                .is_err()
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(store.state.execution, before_ownership);
        let stopped = pipeline
            .stop_native(
                &mut store,
                &mut clock,
                &original_claim,
                &completion,
                "native-proof-stop",
            )
            .unwrap();
        assert_eq!(stopped.get("duplicate"), Some(&Value::Bool(false)));
        let replay = pipeline
            .stop_native(
                &mut store,
                &mut clock,
                &original_claim,
                &completion,
                "native-proof-stop",
            )
            .unwrap();
        assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
        assert_eq!(store.records.len(), before + 1);
        assert_eq!(
            store.state.execution.claim_for("instance", &effect),
            Some(&original_claim)
        );
        let retained = store
            .state
            .execution
            .stopped_for("instance", &effect)
            .unwrap();
        assert_eq!(retained.outcome(), completion.stopped_outcome());
        assert!(store.state.instances["instance"].pending.contains(&effect));
        match pipeline.start_native(&mut store, &original_claim, Duration::from_secs(1)) {
            Err(error) => assert_eq!(error.code, "exec/inflight_deferred"),
            Ok(_) => panic!("stopped owner started another native helper"),
        }
        let claim_material = original_claim.to_value();
        let retry = fsm_core::record::execution::RetryPolicy::from_value(
            claim_material.get("retry").unwrap(),
        )
        .unwrap();
        let before = store.records.len();
        let ownership = store.state.execution.clone();
        let refused = pipeline
            .claim_native(
                &mut store,
                &mut clock,
                fsm_store::store::ExecutionClaimRequest {
                    instance_id: "instance",
                    effect_id: &effect,
                    handler_fingerprint: claim_material
                        .get("handler_fingerprint")
                        .unwrap()
                        .as_str()
                        .unwrap(),
                    retry: &retry,
                    domain: &successor,
                    request_id: "native-competing-claim",
                    expected_seq: None,
                },
            )
            .unwrap_err();
        assert_eq!(refused.code, "exec/store");
        assert_eq!(
            refused
                .details
                .as_ref()
                .unwrap()
                .get("code")
                .and_then(Value::as_str),
            Some("store/execution_owned")
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(store.state.execution, ownership);
        for prefix in ["binding", "launch", "entry", "handoff"] {
            assert_eq!(
                fs::symlink_metadata(fixture.directory.join(format!("{prefix}-2.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        reopened
            .state
            .execution
            .stopped_for("instance", &effect)
            .unwrap()
            .outcome(),
        completion.stopped_outcome()
    );
    assert_eq!(
        reopened.state.execution.claim_for("instance", &effect),
        Some(&original_claim)
    );
    drop(reopened);
    {
        let mut store = Store::open(&fixture.store).unwrap();
        let before = store.records.len();
        let mut pipeline = fsm_execute::run::Pipeline;
        let mut clock = fsm_store::clock::FixedClock::new(2000, 1);
        assert_eq!(
            store
                .state
                .execution
                .settlement_for(
                    &original_claim,
                    fsm_core::record::execution::PendingEffect::Present,
                )
                .unwrap(),
            fsm_core::record::execution::Settlement::Acked,
        );
        assert!(
            store
                .replay_execution_settlement(
                    &original_claim,
                    fsm_core::record::execution::Settlement::Acked,
                    &settlement_request,
                )
                .unwrap()
                .is_none()
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(
            store.state.execution.claim_for("instance", &effect),
            Some(&original_claim)
        );
        assert_eq!(
            pipeline
                .advance_native_settled(
                    &mut store,
                    &mut clock,
                    &original_claim,
                    &completion,
                    &settlement_request,
                )
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert_eq!(store.records.len(), before);
        let mut stale_material = original_claim.to_value().as_obj().unwrap().clone();
        stale_material.insert(
            "run_id".into(),
            Value::Num((original_claim.run_id() + 1).to_string()),
        );
        let stale =
            fsm_core::record::execution::Claim::from_value(&Value::Obj(stale_material)).unwrap();
        assert!(
            fsm_execute::run::native_client::NativeExecution::from_completion(
                &stale,
                text(&binding, "journal_claim").unwrap(),
                fsm_execute::run::native_client::NativeCompletion::verify(
                    &execution,
                    &original_claim,
                    text(&binding, "journal_claim").unwrap(),
                )
                .unwrap(),
            )
            .is_err()
        );
        let mut host = fsm_execute::run::native_client::NativeExecution::from_completion(
            &original_claim,
            text(&binding, "journal_claim").unwrap(),
            fsm_execute::run::native_client::NativeCompletion::verify(
                &execution,
                &original_claim,
                text(&binding, "journal_claim").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(host.progress().retained);
        assert!(host.progress().helper.is_none());
        assert!(host.observe().unwrap());
        let settled = host.settle(&mut store, &mut clock).unwrap();
        assert!(!host.progress().retained);
        assert_eq!(
            host.completion().unwrap().candidate(),
            completion.candidate()
        );
        assert_eq!(
            host.settle(&mut store, &mut clock)
                .unwrap()
                .get("duplicate"),
            Some(&Value::Bool(true))
        );
        assert_eq!(store.records.len(), before + 1);
        assert_eq!(settled.get("duplicate"), Some(&Value::Bool(false)));
        let replay = pipeline
            .settle_stopped(
                &mut store,
                &mut clock,
                &original_claim,
                fsm_core::record::execution::Settlement::Acked,
                &settlement_request,
            )
            .unwrap();
        assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
        assert_eq!(store.records.len(), before + 1);
        assert!(
            store
                .state
                .execution
                .claim_for("instance", &effect)
                .is_none()
        );
        assert!(
            store
                .state
                .execution
                .stopped_for("instance", &effect)
                .is_none()
        );
        assert!(!store.state.instances["instance"].pending.contains(&effect));
    }
    let mut reopened = Store::open_read_only(&fixture.store).unwrap();
    let state = reopened.state.clone();
    let head = reopened.journal.last_hash.clone();
    let replay = reopened
        .replay_execution_settlement(
            &original_claim,
            fsm_core::record::execution::Settlement::Acked,
            &settlement_request,
        )
        .unwrap()
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(
        replay.get("execution").unwrap().get("run_id"),
        Some(&Value::Num(original_claim.run_id().to_string()))
    );
    assert_eq!(
        replay.get("execution").unwrap().get("result"),
        completion.stopped_outcome().result()
    );
    assert_eq!(
        reopened
            .replay_execution_settlement(
                &original_claim,
                fsm_core::record::execution::Settlement::Attempted,
                &settlement_request,
            )
            .unwrap_err()
            .code,
        "req/request_id_conflict"
    );
    assert!(
        reopened
            .replay_execution_settlement(
                &original_claim,
                fsm_core::record::execution::Settlement::Acked,
                "native-unclaimed-settlement",
            )
            .unwrap()
            .is_none()
    );
    assert!(fsm_store::snapshot::store_states_eq(
        &reopened.state,
        &state
    ));
    assert_eq!(reopened.journal.last_hash, head);
    assert!(
        !reopened
            .state
            .dedup
            .contains_key("native-unclaimed-settlement")
    );
    assert!(
        reopened
            .state
            .execution
            .claim_for("instance", &effect)
            .is_none()
    );
    assert!(
        !reopened.state.instances["instance"]
            .pending
            .contains(&effect)
    );
    drop(reopened);
    drop(daemon);
    // Restart with original completion but no current handler catalogue.
    fs::rename(&catalogue, &saved_catalogue).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 2);
    let recovered_response = request(&base, "recover", Value::Num("1".into()));
    assert_eq!(recovered_response, execution);
    let recovered = fsm_execute::run::native_client::NativeCompletion::verify(
        &recovered_response,
        &original_claim,
        text(&binding, "journal_claim").unwrap(),
    )
    .unwrap();
    {
        let mut store = Store::open(&fixture.store).unwrap();
        let mut pipeline = fsm_execute::run::Pipeline;
        let mut clock = fsm_store::clock::FixedClock::new(3000, 1);
        let before = store.records.len();
        let mut stale = original_claim.to_value().as_obj().unwrap().clone();
        stale.insert(
            "run_id".into(),
            Value::Num((original_claim.run_id() + 1).to_string()),
        );
        let stale = fsm_core::record::execution::Claim::from_value(&Value::Obj(stale)).unwrap();
        assert_eq!(
            pipeline
                .advance_native_settled(
                    &mut store,
                    &mut clock,
                    &stale,
                    &recovered,
                    &settlement_request,
                )
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(
            pipeline
                .advance_native_settled(
                    &mut store,
                    &mut clock,
                    &original_claim,
                    &recovered,
                    &settlement_request,
                )
                .unwrap(),
            fsm_execute::run::SettleOutcome::Advanced
        );
        assert_eq!(store.records.len(), before + 1);
        let event = if timeout { "note_added" } else { "docs_ok" };
        assert!(
            store
                .state
                .dedup
                .contains_key(&fsm_execute::rid::event_rid(&effect, event))
        );
        assert!(
            !store
                .state
                .dedup
                .contains_key(&fsm_execute::rid::event_rid(&effect, "withdraw"))
        );
        let view = store.instance_view("instance", None, None).unwrap();
        if timeout {
            assert_eq!(
                view.get("context")
                    .unwrap()
                    .get("notes")
                    .and_then(Value::as_str),
                Some("1")
            );
        } else {
            assert_eq!(
                view.get("configuration")
                    .unwrap()
                    .get("leaf")
                    .and_then(Value::as_str),
                Some("risk_review")
            );
        }
        let state = store.state.clone();
        pipeline
            .advance_native_settled(
                &mut store,
                &mut clock,
                &original_claim,
                &recovered,
                &settlement_request,
            )
            .unwrap();
        assert_eq!(store.records.len(), before + 1);
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    }
    fs::rename(&saved_catalogue, &catalogue).unwrap();
    assert_eq!(fs::read(&completed).unwrap(), completed_bytes);
    assert_eq!(
        identity(&fs::symlink_metadata(&first_socket).unwrap()),
        first_identity
    );
    assert_eq!(
        request(&base, "bind", binding).get("ok"),
        Some(&Value::Bool(false))
    );
    drop(daemon);
    let current = fs::read(&counter_path).unwrap();
    fs::write(&counter_path, &zero).unwrap();
    Daemon::refused(&fixture.directory);
    assert_eq!(fs::read(&counter_path).unwrap(), zero);
    fs::write(&counter_path, &current).unwrap();
    let history = base.join("epoch-2.json");
    let saved_history = base.join("fixture-history.saved");
    fs::rename(&history, &saved_history).unwrap();
    Daemon::refused(&fixture.directory);
    assert_eq!(fs::read(&counter_path).unwrap(), current);
    assert!(!base.join("epoch-3.json").exists());
    fs::rename(&saved_history, &history).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 3);
    assert_eq!(
        identity(&fs::symlink_metadata(&first_socket).unwrap()),
        first_identity
    );
    drop(daemon);
    fixture.cleanup().unwrap();
}

fn permit_operator_store(path: &Path) {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert_eq!(metadata.uid(), 0);
    assert!(metadata.is_dir() || metadata.is_file());
    if metadata.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            permit_operator_store(&entry.unwrap().path());
        }
    }
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if metadata.is_dir() { 0o700 } else { 0o600 }),
    )
    .unwrap();
    std::os::unix::fs::chown(path, Some(65534), Some(65534)).unwrap();
}
