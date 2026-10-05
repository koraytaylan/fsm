//! Independent native process/MCP tree observer around production execution.

use super::super::super::{
    bind, closure, identity, number, object, read_value, runner, stop, text,
};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) const SERVER: &str = r#"import json,os,subprocess,sys,time
from pathlib import Path
base=Path(sys.argv[1])
mode=sys.argv[2]
def publish(name,value):
    temporary=base/(name+'.pending')
    temporary.write_text(json.dumps(value,sort_keys=True,separators=(',',':')))
    temporary.replace(base/name)
def reply(request,result):
    print(json.dumps(dict(jsonrpc='2.0',id=request['id'],result=result)),flush=True)
child_code='''import json,os,subprocess,sys,time
from pathlib import Path
base=Path(sys.argv[1])
os.setsid()
grandchild=subprocess.Popen(['/usr/bin/sleep','300'])
temporary=base/'descendant-ready.pending'
temporary.write_text(json.dumps(dict(pid=os.getpid(),grandchild=grandchild.pid),sort_keys=True,separators=(',',':')))
temporary.replace(base/'descendant-ready')
time.sleep(300)
'''
def enrolled_tree():
    child=subprocess.Popen(['/usr/bin/python3','-c',child_code,str(base)])
    deadline=time.monotonic()+2
    while not (base/'descendant-ready').exists():
        assert time.monotonic()<deadline
        time.sleep(.005)
    publish('root-ready',dict(pid=os.getpid()))
    while not (base/'release').exists():
        assert time.monotonic()<deadline
        time.sleep(.005)
if mode in ('process-exit','process-failure','process-signal','cancel-process','uncertain-process'):
    enrolled_tree()
    if mode in ('process-exit','process-failure','process-signal'):
        print('root-exited',flush=True)
        if mode=='process-signal':
            os.kill(os.getpid(),9)
        sys.exit(17 if mode=='process-failure' else 0)
    time.sleep(300)
for line in sys.stdin:
    request=json.loads(line)
    if request['method']=='initialize':
        reply(request,dict())
    elif request['method']=='tools/call':
        assert request['params']['name']=='probe'
        assert request['params']['arguments']==dict(fixture='native')
        enrolled_tree()
        sys.stderr.buffer.write(bytes(4097))
        sys.stderr.buffer.flush()
        if mode=='answer':
            reply(request,dict(structuredContent=dict(fixture='native'),isError=True))
        elif mode=='protocol':
            print('invalid-json',flush=True)
        time.sleep(300)
"#;

pub(super) struct Barriers {
    pub(super) path: PathBuf,
    identity: Value,
}

impl Barriers {
    pub(super) fn new() -> Self {
        let token = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // DynamicUser's strict system protection keeps /run read-only;
        // /dev/shm is a writable fixture resource, not an authority route.
        let path = PathBuf::from(format!(
            "/dev/shm/fsm-native-mcp-{}-{token}",
            std::process::id()
        ));
        fs::DirBuilder::new().mode(0o1777).create(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o1777)).unwrap();
        Self {
            identity: identity(&fs::symlink_metadata(&path).unwrap()),
            path,
        }
    }
}

impl Drop for Barriers {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| identity(&metadata) == self.identity)
        {
            // Delete only fixture-owned names, preserving unknown directory
            // entries and refusing to follow or delete a replaced namespace.
            for name in [
                "root-ready",
                "root-ready.pending",
                "descendant-ready",
                "descendant-ready.pending",
                "release",
            ] {
                let _ = fs::remove_file(self.path.join(name));
            }
            let _ = fs::remove_dir(&self.path);
        }
    }
}

pub(super) fn run() {
    for mode in [
        "answer",
        "protocol",
        "timeout",
        "retry-timeout",
        "process-exit",
        "process-failure",
        "process-signal",
        "cancel-mcp",
        "cancel-process",
        "uncertain-mcp",
        "uncertain-process",
    ] {
        let barriers = Barriers::new();
        let mut table = object([
            ("format", Value::Str("fsm.handlers/1".into())),
            (
                "handlers",
                Value::Arr(vec![object([
                    ("effect", Value::Str("notify".into())),
                    ("kind", Value::Str("mcp".into())),
                    (
                        "argv",
                        Value::Arr(
                            [
                                "/usr/bin/python3",
                                "-c",
                                SERVER,
                                barriers.path.to_str().unwrap(),
                                mode,
                            ]
                            .into_iter()
                            .map(|value| Value::Str(value.into()))
                            .collect(),
                        ),
                    ),
                    ("tool", Value::Str("probe".into())),
                    (
                        "arguments",
                        object([("fixture", Value::Str("native".into()))]),
                    ),
                    ("timeout_ms", Value::Num("3000".into())),
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
        if matches!(
            mode,
            "process-exit"
                | "process-failure"
                | "process-signal"
                | "cancel-process"
                | "uncertain-process"
        ) {
            let Value::Obj(fields) = &mut table else {
                panic!("fixture table is not an object")
            };
            let Value::Arr(handlers) = fields.get_mut("handlers").unwrap() else {
                panic!("fixture handlers are not an array")
            };
            let Value::Obj(handler) = &mut handlers[0] else {
                panic!("fixture handler is not an object")
            };
            handler.insert("kind".into(), Value::Str("process".into()));
            handler.insert("timeout_ms".into(), Value::Num("10000".into()));
            handler.remove("tool");
            handler.remove("arguments");
        }
        if matches!(mode, "timeout" | "retry-timeout") {
            let mut fields = table.as_obj().unwrap().clone();
            let mut handlers = fields.get("handlers").unwrap().as_arr().unwrap().to_vec();
            let mut handler = handlers[0].as_obj().unwrap().clone();
            let mut retry = handler.get("retry").unwrap().as_obj().unwrap().clone();
            retry.insert("on".into(), Value::Arr(vec![Value::Str("timeout".into())]));
            if mode == "retry-timeout" {
                retry.insert("attempts".into(), Value::Num("2".into()));
            }
            handler.insert("retry".into(), Value::Obj(retry));
            handlers[0] = Value::Obj(handler);
            fields.insert("handlers".into(), Value::Arr(handlers));
            table = Value::Obj(fields);
        }
        let mut fixture = Fixture::new_for_table(table);
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, effect) = claim_binding(&fixture, &domain);
        bind(&fixture.directory, &binding).unwrap();
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if mode.starts_with("cancel-") {
            let already_cancelled = std::sync::atomic::AtomicBool::new(true);
            assert!(
                runner::execute_cancellable(&fixture.directory, 1, &already_cancelled).is_err()
            );
            assert!(fs::symlink_metadata(fixture.directory.join("launch-1.json")).is_err());
        }
        let control = cancelled.clone();
        let directory = fixture.directory.clone();
        let execution =
            std::thread::spawn(move || runner::execute_cancellable(&directory, 1, &control));
        let deadline = Instant::now() + Duration::from_secs(3);
        while !barriers.path.join("root-ready").exists() {
            assert!(
                !execution.is_finished(),
                "native MCP execution ended before enrollment barrier"
            );
            assert!(
                Instant::now() < deadline,
                "native MCP enrollment barrier timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let root = read_value(&barriers.path.join("root-ready"), false).unwrap();
        let descendants = read_value(&barriers.path.join("descendant-ready"), false).unwrap();
        let pids = [
            number(&root, "pid").unwrap(),
            number(&descendants, "pid").unwrap(),
            number(&descendants, "grandchild").unwrap(),
        ];
        let unit = format!(
            "fsm-containment-{}-1-1.service",
            text(&domain.to_value(), "namespace").unwrap()
        );
        let membership = format!("0::/system.slice/{unit}\n");
        for pid in pids {
            assert_eq!(
                fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
                membership
            );
            assert!((61184..=65519).contains(&fs::metadata(format!("/proc/{pid}")).unwrap().uid()));
        }
        let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
        assert_eq!(
            number(handoff.get("gate").unwrap(), "pid").unwrap(),
            pids[0]
        );
        assert!(!execution.is_finished());
        assert!(
            fs::read_to_string(
                Path::new("/sys/fs/cgroup/system.slice")
                    .join(&unit)
                    .join("cgroup.events")
            )
            .unwrap()
            .lines()
            .any(|line| line == "populated 1")
        );
        let handoff_path = fixture.directory.join("handoff-1.json");
        let saved_handoff = fs::read(&handoff_path).unwrap();
        if mode.starts_with("uncertain-") {
            // Fixture-owned protected corruption is introduced only after
            // independent enrollment; cleanup cannot authenticate this data.
            fs::write(&handoff_path, b"{}").unwrap();
            fs::File::open(&handoff_path).unwrap().sync_all().unwrap();
        }
        if mode.starts_with("cancel-") || mode.starts_with("uncertain-") {
            cancelled.store(true, std::sync::atomic::Ordering::Release);
        }
        fs::write(
            barriers.path.join("release"),
            b"independent enrollment verified",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !execution.is_finished() {
            assert!(
                Instant::now() < deadline,
                "native MCP execution cleanup timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let execution_result = execution.join().unwrap();
        if mode.starts_with("uncertain-") {
            let error = execution_result.unwrap_err();
            assert!(error.contains("runner cleanup uncertain"), "{error}");
            assert!(fixture.directory.join("closing-1.json").is_file());
            assert!(fs::symlink_metadata(fixture.directory.join("entry-1.json")).is_err());
            assert!(fs::symlink_metadata(fixture.directory.join("closed-1.json")).is_err());
            let receipt = fixture.directory.join(format!(
                "closure-1-{}.json",
                number(binding.get("claim").unwrap(), "run_id").unwrap()
            ));
            assert!(VerifiedClosure::read(&receipt).is_err());
            assert_eq!(fs::read(&handoff_path).unwrap(), b"{}");
            assert_unresolved(&fixture, &effect);
            assert!(runner::execute(&fixture.directory, 1).is_err());
            // Restore only the exact fixture-owned fault, then independently
            // close the domain; this cannot relabel the failed execution.
            fs::write(&handoff_path, saved_handoff).unwrap();
            fs::File::open(&handoff_path).unwrap().sync_all().unwrap();
            fs::File::open(&fixture.directory)
                .unwrap()
                .sync_all()
                .unwrap();
            let _ = stop::request(&fixture.directory, 1);
            closure::complete(&fixture.directory, 1).unwrap();
            VerifiedClosure::read(&receipt).unwrap();
            assert_unresolved(&fixture, &effect);
            fixture.cleanup().unwrap();
            continue;
        }
        let result = execution_result.unwrap();
        assert_eq!(result.get("claim"), binding.get("claim"));
        assert_eq!(result.get("journal_claim"), binding.get("journal_claim"));
        let candidate = result.get("candidate").unwrap();
        if mode == "answer" {
            assert_eq!(
                result.get("failure_class"),
                Some(&Value::Str("mcp_error".into()))
            );
            assert_eq!(
                candidate.get("error"),
                Some(&Value::Str("mcp/tool_error".into()))
            );
            assert_eq!(
                candidate.get("structured"),
                Some(&object([("fixture", Value::Str("native".into()))]))
            );
            assert_eq!(
                candidate
                    .get("stderr")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .as_bytes(),
                &[0; 4096]
            );
            assert_eq!(
                candidate.get("stderr_sha256"),
                Some(&Value::Str(
                    "b587fa297299ce9c602e58292b51379402bf7b1074f6b18679c2fb871c917ca8".into()
                ))
            );
        } else if mode == "protocol" {
            assert_eq!(result.get("failure_class"), Some(&Value::Null));
            assert_eq!(
                candidate.get("error"),
                Some(&Value::Str("exec/mcp_protocol".into()))
            );
        } else if mode.starts_with("cancel-") {
            assert_eq!(result.get("failure_class"), Some(&Value::Null));
            assert_eq!(
                candidate.get("error"),
                Some(&Value::Str("exec/cancelled".into()))
            );
        } else if matches!(mode, "timeout" | "retry-timeout") {
            assert_eq!(
                result.get("failure_class"),
                Some(&Value::Str("timeout".into()))
            );
            assert_eq!(
                candidate.get("error"),
                Some(&Value::Str("exec/timeout".into()))
            );
        } else {
            let (class, status) = if mode == "process-failure" {
                (Value::Str("nonzero_exit".into()), "17")
            } else if mode == "process-signal" {
                (Value::Str("nonzero_exit".into()), "-1")
            } else {
                (Value::Null, "0")
            };
            assert_eq!(result.get("failure_class"), Some(&class));
            assert_eq!(candidate.get("status"), Some(&Value::Num(status.into())));
            assert_eq!(
                candidate.get("stdout"),
                Some(&Value::Str("root-exited\n".into()))
            );
        }
        VerifiedClosure::read(Path::new(result.get("receipt").unwrap().as_str().unwrap())).unwrap();
        let response = object([
            ("format", Value::Str("fsm.native-response/1".into())),
            ("ok", Value::Bool(true)),
            ("result", result.clone()),
        ]);
        let original_claim =
            fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
        let completion = fsm_execute::run::native_client::NativeCompletion::verify(
            &response,
            &original_claim,
            binding.get("journal_claim").unwrap().as_str().unwrap(),
        )
        .unwrap();
        let expected = match mode {
            "answer" => "mcp_error",
            "protocol" => "failed",
            "timeout" | "retry-timeout" => "timeout",
            "cancel-process" | "cancel-mcp" => "interrupted",
            "process-exit" => "ok",
            "process-failure" | "process-signal" => "nonzero_exit",
            _ => panic!("uncertain runner must not reach verified completion"),
        };
        assert_eq!(completion.stopped_outcome().status(), expected);
        if mode == "timeout" {
            let stopped = completion.stopped_outcome().result().unwrap();
            assert_eq!(
                stopped.get("error").and_then(Value::as_str),
                Some(fsm_execute::error::RETRIES_EXHAUSTED)
            );
            assert_eq!(
                stopped.get("class").and_then(Value::as_str),
                Some("timeout")
            );
            assert_eq!(stopped.get("attempts"), Some(&Value::Num("1".into())));
            assert_eq!(stopped.get("status"), candidate.get("status"));
            assert_eq!(completion.candidate(), candidate);
            assert_eq!(
                candidate.get("error").and_then(Value::as_str),
                Some("exec/timeout")
            );
        } else {
            assert_eq!(completion.stopped_outcome().result(), Some(candidate));
        }
        assert!(
            !Path::new("/sys/fs/cgroup/system.slice")
                .join(&unit)
                .exists()
        );
        assert_unresolved(&fixture, &effect);
        if mode == "retry-timeout" {
            settle_retry(&mut fixture, &effect, &original_claim, &completion);
        } else if mode == "process-failure" {
            settle_failure(&fixture, &effect, &original_claim, &completion);
        }
        assert!(runner::execute(&fixture.directory, 1).is_err());
        fixture.cleanup().unwrap();
    }
}

fn settle_failure(
    fixture: &Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
) {
    use fsm_core::record::execution::{PendingEffect, Settlement};
    let mut store = Store::open(&fixture.store).unwrap();
    let mut pipeline = fsm_execute::run::Pipeline;
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    let before = store.records.len();
    pipeline
        .stop_native(
            &mut store,
            &mut clock,
            claim,
            completion,
            "native-failure-stop",
        )
        .unwrap();
    assert_eq!(
        store
            .state
            .execution
            .settlement_for(claim, PendingEffect::Present)
            .unwrap(),
        Settlement::Acked
    );
    pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Acked,
            "native-failure-settle",
        )
        .unwrap();
    let replay = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Acked,
            "native-failure-settle",
        )
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(store.records.len(), before + 2);
    assert!(!store.state.instances["instance"].pending.contains(effect));
    assert!(
        store
            .state
            .execution
            .claim_for("instance", effect)
            .is_none()
    );
    let record = store.records.last().unwrap();
    assert_eq!(
        record.body.get("outcome").and_then(Value::as_str),
        Some("failed")
    );
    assert_eq!(
        record.body.get("result"),
        completion.stopped_outcome().result()
    );
    let execution = store.state.execution.clone();
    drop(store);
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(reopened.state.execution, execution);
    assert!(
        !reopened.state.instances["instance"]
            .pending
            .contains(effect)
    );
}

fn settle_retry(
    fixture: &mut Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
) {
    use fsm_core::record::execution::{PendingEffect, Settlement};
    let mut store = Store::open(&fixture.store).unwrap();
    let mut pipeline = fsm_execute::run::Pipeline;
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    let before = store.records.len();
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
    let settled = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Attempted,
            "native-retry-settle",
        )
        .unwrap();
    assert_eq!(settled.get("duplicate"), Some(&Value::Bool(false)));
    let replay = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Attempted,
            "native-retry-settle",
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
    assert!(store.state.instances["instance"].pending.contains(effect));
    let execution = store.state.execution.clone();
    drop(store);
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(reopened.state.execution, execution);
    assert!(
        reopened.state.instances["instance"]
            .pending
            .contains(effect)
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
    let next = store.state.execution.claim_for("instance", effect).unwrap();
    assert_eq!(next.run_id(), claim.run_id() + 1);
    let next_material = next.to_value();
    assert_eq!(next_material.get("attempt"), Some(&Value::Num("2".into())));
    assert_eq!(next_material.get("retry"), material.get("retry"));
    assert_eq!(
        next_material.get("handler_fingerprint"),
        material.get("handler_fingerprint")
    );
    assert_eq!(next_material.get("domain"), Some(&successor.to_value()));
    let retained = store.state.execution.clone();
    drop(store);
    assert_eq!(
        Store::open_read_only(&fixture.store)
            .unwrap()
            .state
            .execution,
        retained
    );
}

fn assert_unresolved(fixture: &Fixture, effect: &str) {
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
}
