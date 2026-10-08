//! Independent native process/MCP tree observer around production execution.

use super::super::super::{bind, closure, identity, number, object, read_value, runner, text};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "runner_advance_native_tests.rs"]
mod advance;
use advance::settle_owned;
#[path = "runner_claim_host_native_tests.rs"]
mod claim_host;
#[path = "runner_handoff_recovery_native_tests.rs"]
mod handoff_recovery;
#[path = "runner_orphan_cli_native_tests.rs"]
mod orphan_cli;
#[path = "runner_orphan_recovery_native_tests.rs"]
mod orphan_recovery;
#[path = "runner_pre_run_recovery_native_tests.rs"]
mod pre_run_recovery;
#[path = "runner_recovery_native_tests.rs"]
mod recovery;
#[path = "runner_retry_native_tests.rs"]
mod retry;
#[path = "runner_stopped_host_native_tests.rs"]
mod stopped_host;
#[path = "runner_stopped_recovery_native_tests.rs"]
mod stopped_recovery;
use retry::settle_retry;

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

pub(super) fn recover_pre_run() {
    pre_run_recovery::run();
}

pub(super) fn run() {
    let modes = [
        "answer",
        "protocol",
        "timeout",
        "retry-timeout",
        "retry-timeout-kill",
        "process-exit",
        "process-recover-removed",
        "process-recover-changed",
        "process-recover-stopped-kill",
        "process-recover-stopped-removed",
        "process-recover-stopped-changed",
        "process-recover-acked-kill",
        "process-recover-public-tick",
        "process-recover-public-tick-with",
        "process-failure",
        "process-signal",
        "cancel-mcp",
        "cancel-process",
        "uncertain-mcp",
        "uncertain-mcp-cli",
        "uncertain-process-cli",
        "uncertain-process",
    ];
    let selected = std::env::var("FSM_NATIVE_RUNNER_MODE_FILTER").ok();
    if let Some(selected) = selected.as_deref() {
        assert!(
            modes.contains(&selected),
            "unknown native runner fixture mode"
        );
    } else {
        claim_host::run();
    }
    for mode in modes
        .into_iter()
        .filter(|mode| selected.as_deref().is_none_or(|selected| selected == *mode))
    {
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
                                if mode.starts_with("process-recover-") {
                                    "process-exit"
                                } else if mode == "uncertain-process-cli" {
                                    "uncertain-process"
                                } else {
                                    mode
                                },
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
                    (
                        "timeout_ms",
                        Value::Num(
                            if mode.starts_with("uncertain-") {
                                "30000"
                            } else {
                                "3000"
                            }
                            .into(),
                        ),
                    ),
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
                | "process-recover-removed"
                | "process-recover-changed"
                | "process-recover-public-tick"
                | "process-recover-public-tick-with"
                | "process-recover-acked-kill"
                | "process-recover-stopped-removed"
                | "process-recover-stopped-changed"
                | "process-recover-stopped-kill"
                | "process-failure"
                | "process-signal"
                | "cancel-process"
                | "uncertain-process"
                | "uncertain-process-cli"
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
            // The live-owner probe must finish before this fixture's original
            // handler deadline; cancellation below still bounds actual cleanup.
            handler.insert(
                "timeout_ms".into(),
                Value::Num(
                    if mode.starts_with("uncertain-") {
                        "30000"
                    } else {
                        "10000"
                    }
                    .into(),
                ),
            );
            handler.remove("tool");
            handler.remove("arguments");
            if mode.starts_with("process-recover-") {
                handler.insert(
                    "on_ok".into(),
                    object([("event", Value::Str("docs_ok".into()))]),
                );
            }
        }
        if matches!(mode, "timeout" | "retry-timeout" | "retry-timeout-kill") {
            let mut fields = table.as_obj().unwrap().clone();
            let mut handlers = fields.get("handlers").unwrap().as_arr().unwrap().to_vec();
            let mut handler = handlers[0].as_obj().unwrap().clone();
            let mut retry = handler.get("retry").unwrap().as_obj().unwrap().clone();
            retry.insert("on".into(), Value::Arr(vec![Value::Str("timeout".into())]));
            if matches!(mode, "retry-timeout" | "retry-timeout-kill") {
                retry.insert("attempts".into(), Value::Num("2".into()));
            }
            handler.insert("retry".into(), Value::Obj(retry));
            handlers[0] = Value::Obj(handler);
            fields.insert("handlers".into(), Value::Arr(handlers));
            table = Value::Obj(fields);
        }
        let mut fixture = if mode.starts_with("process-recover-") || mode.starts_with("uncertain-")
        {
            Fixture::new_for_operator(table)
        } else {
            Fixture::new_for_table(table)
        };
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, effect) = claim_binding(&fixture, &domain);
        // Exercise the production entry before binding: neutralizing only the
        // lease guard reaches the distinct missing-binding error without launch.
        let held_lease = super::super::super::runner_lease::acquire(&fixture.directory, 1).unwrap();
        let refusal = runner::execute(&fixture.directory, 1).unwrap_err();
        assert_eq!(
            refusal,
            "original runner remains active or lease locking is unavailable"
        );
        assert!(!fixture.directory.join("launch-1.json").exists());
        assert!(!fixture.directory.join("binding-1.json").exists());
        drop(held_lease);
        bind(&fixture.directory, &binding).unwrap();
        if matches!(mode, "uncertain-mcp-cli" | "uncertain-process-cli") {
            orphan_cli::run(&mut fixture, &barriers, &domain);
            continue;
        }
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
        let reconciliation = mode
            .starts_with("uncertain-")
            .then(|| orphan_recovery::Session::new(&fixture));
        if let Some(reconciliation) = &reconciliation {
            // Enrollment is already independently proved; the live-owner CLI
            // refusal must not consume the server's two-second setup barrier.
            // Uncertain handlers remain live after release until cancellation.
            fs::write(
                barriers.path.join("release"),
                b"owner refusal enrollment verified",
            )
            .unwrap();
            reconciliation.refuse_live_runner();
            assert!(!execution.is_finished());
            for pid in pids {
                assert_eq!(
                    fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
                    membership
                );
            }
            assert_eq!(fs::read(&handoff_path).unwrap(), saved_handoff);
        }
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
            // Failed proof must still fence the original live tree; an empty
            // sample or physical absence never releases the unresolved claim.
            let group = Path::new("/sys/fs/cgroup/system.slice").join(&unit);
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match fs::symlink_metadata(&group) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                    Err(error) => panic!("uncertain native identity inspection failed: {error}"),
                    Ok(_) => {
                        let sample =
                            match super::super::super::observation::read(&fixture.directory, 1) {
                                Ok(sample) => sample,
                                // Retirement may race the read after metadata above;
                                // absence ends sampling, never the proof refusal below.
                                Err(_)
                                    if fs::symlink_metadata(&group).is_err_and(|error| {
                                        error.kind() == std::io::ErrorKind::NotFound
                                    }) =>
                                {
                                    break;
                                }
                                Err(error) => {
                                    panic!("uncertain native observation failed: {error}")
                                }
                            };
                        assert_eq!(sample.get("domain"), Some(&domain.to_value()));
                        assert_eq!(sample.get("closing"), Some(&Value::Bool(true)));
                        if sample.get("populated") == Some(&Value::Bool(false)) {
                            break;
                        }
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "uncertain original tree remains populated"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(fs::symlink_metadata(fixture.directory.join("entry-1.json")).is_err());
            assert!(fs::symlink_metadata(fixture.directory.join("closed-1.json")).is_err());
            let receipt = fixture.directory.join(format!(
                "closure-1-{}.json",
                number(binding.get("claim").unwrap(), "run_id").unwrap()
            ));
            assert!(VerifiedClosure::read(&receipt).is_err());
            assert_eq!(fs::read(&handoff_path).unwrap(), b"{}");
            assert_unresolved(&fixture, &effect);
            assert!(runner::recover(&fixture.directory, 1).is_err());
            assert!(runner::execute(&fixture.directory, 1).is_err());
            // Restore only the exact fixture-owned fault; startup must obtain
            // its own original-domain proof before settling the interrupted run.
            fs::write(&handoff_path, saved_handoff).unwrap();
            fs::File::open(&handoff_path).unwrap().sync_all().unwrap();
            fs::File::open(&fixture.directory)
                .unwrap()
                .sync_all()
                .unwrap();
            reconciliation.as_ref().unwrap().resume();
            VerifiedClosure::read(&receipt).unwrap();
            drop(reconciliation);
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
        } else if matches!(mode, "timeout" | "retry-timeout" | "retry-timeout-kill") {
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
        assert_eq!(
            completion.handler().fingerprint(),
            original_claim
                .to_value()
                .get("handler_fingerprint")
                .unwrap()
                .as_str()
                .unwrap()
        );
        assert_eq!(
            completion.handler().contract_value(),
            *result.get("handler_contract").unwrap()
        );
        let expected = match mode {
            "answer" => "mcp_error",
            "protocol" => "failed",
            "timeout" | "retry-timeout" | "retry-timeout-kill" => "timeout",
            "cancel-process" | "cancel-mcp" => "interrupted",
            "process-exit"
            | "process-recover-removed"
            | "process-recover-changed"
            | "process-recover-public-tick"
            | "process-recover-public-tick-with"
            | "process-recover-acked-kill"
            | "process-recover-stopped-removed"
            | "process-recover-stopped-changed"
            | "process-recover-stopped-kill" => "ok",
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
        if matches!(mode, "retry-timeout" | "retry-timeout-kill") {
            settle_retry(
                &mut fixture,
                &effect,
                &original_claim,
                &completion,
                mode,
                &barriers,
            );
        } else if mode == "process-failure" {
            settle_failure(&fixture, &effect, &original_claim, &completion);
        } else if matches!(mode, "cancel-process" | "cancel-mcp") {
            settle_interrupted(&fixture, &effect, &original_claim, &completion);
        } else if mode == "process-exit" || mode.starts_with("process-recover-") {
            let (mut store, before) =
                recovery::reopen_stopped(&fixture, &effect, &original_claim, &completion, mode);
            if matches!(
                mode,
                "process-recover-stopped-removed" | "process-recover-stopped-changed"
            ) {
                drop(store);
                stopped_recovery::resume(&fixture, &completion, mode);
                assert!(runner::execute(&fixture.directory, 1).is_err());
                fixture.cleanup().unwrap();
                continue;
            }
            if mode != "process-recover-acked-kill" {
                settle_owned(
                    &fixture,
                    &mut store,
                    &mut fsm_store::clock::FixedClock::new(1000, 1),
                    &original_claim,
                    &completion,
                );
            }
            assert_eq!(store.records.len(), before + 2);
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
            let records = store.records.clone();
            drop(store);
            let reopened = Store::open(&fixture.store).unwrap();
            assert_eq!(reopened.records, records);
            assert!(
                !reopened.state.instances["instance"]
                    .pending
                    .contains(&effect)
            );
            drop(reopened);
            if mode.starts_with("process-recover-") {
                handoff_recovery::resume_original_event(&fixture, &effect, &completion, mode);
            }
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
    settle_owned(fixture, &mut store, &mut clock, claim, completion);
    let replay = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Acked,
            &fsm_execute::rid::ack_rid(effect),
        )
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(store.records.len(), before + 2);
    assert!(
        !store.state.instances["instance"]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
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
            .iter()
            .any(|pending| pending == effect)
    );
}

fn assert_unresolved(fixture: &Fixture, effect: &str) {
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", effect)
            .is_some()
    );
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", effect)
            .is_none()
    );
    drop(store);
}

fn settle_interrupted(
    fixture: &Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
) {
    use fsm_core::record::execution::Settlement;
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
            "native-interrupted-stop",
        )
        .unwrap();
    let settled = settle_owned(fixture, &mut store, &mut clock, claim, completion);
    assert_eq!(
        settled
            .get("execution")
            .unwrap()
            .get("disposition")
            .and_then(Value::as_str),
        Some("interrupted")
    );
    assert_eq!(store.records.len(), before + 2);
    assert!(
        store.state.instances["instance"]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
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
    assert_eq!(store.state.execution.failed_count("instance", effect), 0);
    let request = format!("exec-interrupted-{effect}-{}", claim.run_id());
    let replay = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Interrupted,
            &request,
        )
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(
        pipeline
            .advance_native_settled(&mut store, &mut clock, claim, completion, &request)
            .unwrap_err()
            .code,
        "exec/inflight_deferred"
    );
    assert_eq!(store.records.len(), before + 2);
    assert!(
        !store
            .state
            .dedup
            .contains_key(&fsm_execute::rid::ack_rid(effect))
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
}
