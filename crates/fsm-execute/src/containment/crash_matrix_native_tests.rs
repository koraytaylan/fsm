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
            for behavior in [
                "hold-result",
                "signal-int",
                "signal-term",
                "torn-tail",
                "noisy-result",
                "collected-timeout",
                "collected-result",
                "supervisor-death",
                "closed-result",
                "stopped-result",
                "acked-result",
                "event-result",
                "claimed-result",
                "authorization",
                "repeated-noisy",
            ] {
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

#[test]
#[ignore = "requires disposable native CI and exact staged private host artifacts"]
fn provisioned_private_completion_owner_matrix() {
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
        ("host-test", "HOST"),
        ("boundary-test", "BOUNDARY"),
        ("owner-test", "OWNER"),
        ("fixture", "FIXTURE"),
        ("fsm", "CLI"),
    ] {
        super::workflow_cases::stage_artifact(
            &staging.join(name),
            &format!("FSM_CRASH_{variable}_ARTIFACT"),
            &format!("FSM_CRASH_{variable}_SHA256"),
        );
    }
    for (host, behavior) in [
        ("private", "private-held"),
        ("boundary", "boundary-held"),
        ("boundary", "boundary-settled"),
        ("boundary", "boundary-deferred"),
        ("capacity", "capacity-held"),
        ("private", "private-output"),
    ] {
        for kind in ["process", "mcp"] {
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
    fs::remove_dir_all(staging).unwrap();
}

#[test]
#[ignore = "requires disposable native CI and exact staged private scheduling artifacts"]
fn provisioned_private_scheduling_owner_matrix() {
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
        ("host-test", "HOST"),
        ("fixture", "FIXTURE"),
        ("fsm", "CLI"),
    ] {
        super::workflow_cases::stage_artifact(
            &staging.join(name),
            &format!("FSM_CRASH_{variable}_ARTIFACT"),
            &format!("FSM_CRASH_{variable}_SHA256"),
        );
    }
    super::closure_contention_cases::run();
    super::allocation_closing_cases::run();
    for behavior in [
        "schedule-success",
        "schedule-retry",
        "schedule-compensation",
        "schedule-recovery",
        "schedule-construction",
        "schedule-fairness",
        "schedule-queues",
    ] {
        for kind in ["process", "mcp"] {
            scenario(
                &staging,
                &nonce[..24],
                Scenario {
                    host: "private",
                    kind,
                    behavior,
                },
            );
        }
    }
    fs::remove_dir_all(staging).unwrap();
}

#[test]
#[ignore = "requires disposable native CI and exact staged contract admission artifacts"]
fn provisioned_contract_admission_matrix() {
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
        ("contract-test", "CONTRACT"),
        ("fixture", "FIXTURE"),
        ("fsm", "CLI"),
    ] {
        super::workflow_cases::stage_artifact(
            &staging.join(name),
            &format!("FSM_CRASH_{variable}_ARTIFACT"),
            &format!("FSM_CRASH_{variable}_SHA256"),
        );
    }
    for behavior in [
        "contract-standalone",
        "contract-borrowed",
        "contract-fair-standalone",
        "contract-fair-borrowed",
        "contract-unknown-standalone",
        "contract-unknown-borrowed",
        "contract-argument-standalone",
        "contract-argument-borrowed",
        "contract-contention-standalone",
        "contract-contention-borrowed",
    ] {
        for kind in ["process", "mcp"] {
            scenario(
                &staging,
                &nonce[..24],
                Scenario {
                    host: "contract",
                    kind,
                    behavior,
                },
            );
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
    if behavior == "schedule-fairness" {
        for name in ["busy", "quiet"] {
            let directory = resource.join(name);
            fs::DirBuilder::new()
                .mode(0o777)
                .create(&directory)
                .unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
            for role in ["root", "child", "grandchild"] {
                let slot = directory.join(format!("{role}-entered"));
                fs::write(&slot, b"").unwrap();
                fs::set_permissions(slot, fs::Permissions::from_mode(0o666)).unwrap();
            }
        }
    }
    if matches!(behavior, "repeated-noisy" | "schedule-queues") {
        for index in 0..if behavior == "schedule-queues" { 9 } else { 12 } {
            let directory = resource.join(format!("run-{index}"));
            fs::DirBuilder::new()
                .mode(0o777)
                .create(&directory)
                .unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o777)).unwrap();
            for role in ["root", "child", "grandchild"] {
                let slot = directory.join(format!("{role}-entered"));
                fs::write(&slot, b"").unwrap();
                fs::set_permissions(slot, fs::Permissions::from_mode(0o666)).unwrap();
            }
        }
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
    if let Some(cut) = behavior
        .strip_suffix("-result")
        .filter(|cut| matches!(*cut, "stopped" | "acked" | "event" | "claimed"))
        .or_else(|| (behavior == "schedule-recovery").then_some("acked"))
    {
        let physical = fs::metadata(&fixture.store).unwrap();
        let request = fixture.directory.join("crash-journal-barrier.json");
        fs::write(
            &request,
            canon_bytes(&object([
                ("cut", Value::Str(cut.into())),
                ("device", Value::Num(physical.dev().to_string())),
                ("inode", Value::Num(physical.ino().to_string())),
                ("attempt", Value::Num("1".into())),
                ("run_id", Value::Num("1".into())),
            ])),
        )
        .unwrap();
        fs::set_permissions(request, fs::Permissions::from_mode(0o444)).unwrap();
    }
    if matches!(
        behavior,
        "collected-timeout" | "collected-result" | "closed-result" | "authorization"
    ) {
        let request = fixture.directory.join("crash-candidate-barrier.json");
        let mut fields = BTreeMap::from([
            ("attempt".into(), Value::Num("1".into())),
            ("run_id".into(), Value::Num("1".into())),
            (
                "cut".into(),
                Value::Str(
                    if behavior == "closed-result" {
                        "domain-closed"
                    } else if behavior == "authorization" {
                        "authorization"
                    } else {
                        "candidate"
                    }
                    .into(),
                ),
            ),
        ]);
        fields.insert(
            "closure_release".into(),
            Value::Str(resource.join("closure-release").to_str().unwrap().into()),
        );
        fs::write(&request, canon_bytes(&Value::Obj(fields))).unwrap();
        fs::set_permissions(request, fs::Permissions::from_mode(0o444)).unwrap();
    }
    let limits = memory_limits::Limits::install(&fixture);
    super::broker_cases::disconnect_cases::permit_operator_store(&fixture.store);
    super::super::super::broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    let mut broker = Some(super::broker_cases::Daemon::ready(&fixture.directory, 1));
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
    let private = host == "private";
    let completion = private || matches!(host, "boundary" | "capacity" | "contract");
    let artifact = match host {
        "private" => "host-test",
        "contract" => "contract-test",
        "boundary" => "boundary-test",
        "capacity" => "owner-test",
        _ => "matrix-test",
    };
    let mut command = Command::new("/usr/bin/python3");
    command.args(["-c", "import os,sys;os.setgroups([]);os.setgid(65534);os.setuid(65534);os.execv(sys.argv[1],sys.argv[1:])"])
        .arg(staging.join(artifact))
        .args(["--exact", if behavior == "contract-contention-standalone" {
            "provisioned::standalone_native_timeout_reaps_original_tree_while_writer_is_held"
        } else if behavior == "contract-contention-borrowed" {
            "provisioned::borrowed_native_timeout_reaps_original_tree_while_writer_is_held"
        } else if behavior == "contract-argument-standalone" {
            "provisioned::standalone_native_missing_argument_preserves_work_until_repair"
        } else if behavior == "contract-argument-borrowed" {
            "provisioned::borrowed_native_missing_argument_preserves_work_until_repair"
        } else if behavior == "contract-unknown-standalone" {
            "provisioned::standalone_native_unknown_outcome_preserves_work_until_repair"
        } else if behavior == "contract-unknown-borrowed" {
            "provisioned::borrowed_native_unknown_outcome_preserves_work_until_repair"
        } else if behavior == "contract-fair-standalone" {
            "provisioned::standalone_native_incompatible_machine_cannot_starve_compatible_work"
        } else if behavior == "contract-fair-borrowed" {
            "provisioned::borrowed_native_incompatible_machine_cannot_starve_compatible_work"
        } else if behavior == "contract-standalone" {
            "provisioned::standalone_native_refusal_preserves_work_and_repair_starts_original_handler"
        } else if behavior == "contract-borrowed" {
            "provisioned::borrowed_native_refusal_preserves_work_and_repair_starts_original_handler"
        } else if behavior == "schedule-success" {
            "mcp::host::tests::held_handlers::autonomous_schedule_real_handler_success_without_another_command"
        } else if behavior == "schedule-queues" {
            "mcp::host::tests::scheduling_handlers::autonomous_schedule_ready_completions_yield_to_admitted_application_within_eight_turns"
        } else if behavior == "schedule-fairness" {
            "mcp::host::tests::scheduling_fairness::autonomous_schedule_large_outbox_cannot_starve_another_native_instance"
        } else if behavior == "schedule-construction" {
            "mcp::host::tests::scheduling_construction::autonomous_schedule_restricted_modes_start_no_genuine_fixture"
        } else if behavior == "schedule-recovery" {
            "mcp::host::tests::scheduling_recovery::autonomous_schedule_reopened_acknowledgement_advances_without_rpc"
        } else if behavior == "schedule-compensation" {
            "mcp::host::tests::scheduling_handlers::autonomous_schedule_real_compensation_completes_without_another_command"
        } else if behavior == "schedule-retry" {
            "mcp::host::tests::scheduling_handlers::autonomous_schedule_real_timeout_retry_pins_backoff_without_another_command"
        } else if behavior == "private-output" {
            "mcp::host::tests::held_handlers::execution_host_inherited_output_pipes_allow_read_mutation_and_stop"
        } else if private {
            "mcp::host::tests::held_handlers::execution_host_real_held_handler_allows_read_mutation_and_stop_without_release"
        } else if host == "capacity" {
            "service::lifecycle::tests::capacity::completion_capacity_keeps_effect_pending_until_original_reservations_release"
        } else if behavior == "boundary-deferred" && kind == "process" {
            "native::async_completion_process_pending_writer_refusal_retains_completion"
        } else if behavior == "boundary-deferred" {
            "native::async_completion_mcp_pending_writer_refusal_retains_completion"
        } else if behavior == "boundary-settled" && kind == "process" {
            "native::async_completion_process_repeated_polling_settles_once"
        } else if behavior == "boundary-settled" {
            "native::async_completion_mcp_repeated_polling_settles_once"
        } else if host == "boundary" && kind == "process" {
            "native::async_completion_process_dispatch_poll_and_stop_do_not_wait_for_release"
        } else if host == "boundary" {
            "native::async_completion_mcp_dispatch_poll_and_stop_do_not_wait_for_release"
        } else {
            "native::production_candidate_result_crash_retains_original_tree_until_verified_closure"
        }, "--ignored", "--nocapture", "--color", "never"])
        .env(if host == "contract" { "FSM_CONTRACT_NATIVE_MANIFEST" } else if completion { "FSM_COMPLETION_NATIVE_MANIFEST" } else { "FSM_LIFECYCLE_NATIVE_MANIFEST" }, &manifest)
        .env("TMPDIR", &fixture.store)
        .stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log);
    if completion {
        command.env("HOME", &home);
    }
    let mut actor = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(80);
    let mut supervisor_killed = false;
    let mut supervisor_restarted = false;
    let status = loop {
        if let Some(status) = actor.try_wait().unwrap() {
            break status;
        }
        if behavior == "supervisor-death" {
            if !supervisor_killed && resource.join("supervisor-death-request").is_file() {
                broker.take().unwrap().kill_and_wait();
                publish_supervisor_observation(&fixture, "dead", 1);
                supervisor_killed = true;
            }
            if supervisor_killed
                && !supervisor_restarted
                && resource.join("supervisor-restart-request").is_file()
            {
                broker = Some(super::broker_cases::Daemon::ready(&fixture.directory, 2));
                publish_supervisor_observation(&fixture, "restarted", 2);
                supervisor_restarted = true;
            }
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
    if behavior == "supervisor-death" {
        assert!(supervisor_killed && supervisor_restarted);
    }
    if behavior == "repeated-noisy" {
        let observations: Vec<_> = output
            .split(|byte| *byte == b'\n')
            .filter(|line| line.starts_with(b"FSM_NATIVE_RESOURCE_OBSERVATION "))
            .collect();
        assert_eq!(observations.len(), 12);
        for observation in observations {
            std::io::stdout().lock().write_all(observation).unwrap();
            std::io::stdout().lock().write_all(b"\n").unwrap();
        }
    }
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

fn publish_supervisor_observation(fixture: &Fixture, phase: &str, epoch: u64) {
    let path = fixture
        .directory
        .join(format!("crash-supervisor-{phase}.json"));
    super::super::super::publish_once(&path, &object([("epoch", Value::Num(epoch.to_string()))]))
        .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o444)).unwrap();
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
                    if behavior == "private-output" {
                        "exit-root"
                    } else if matches!(
                        behavior,
                        "collected-result"
                            | "closed-result"
                            | "stopped-result"
                            | "acked-result"
                            | "event-result"
                            | "claimed-result"
                            | "authorization"
                    ) && kind == "process"
                    {
                        "hold-exit"
                    } else if matches!(
                        behavior,
                        "collected-timeout"
                            | "collected-result"
                            | "supervisor-death"
                            | "closed-result"
                            | "stopped-result"
                            | "acked-result"
                            | "event-result"
                            | "claimed-result"
                            | "authorization"
                            | "signal-int"
                            | "signal-term"
                            | "torn-tail"
                            | "private-held"
                            | "boundary-held"
                            | "boundary-settled"
                            | "boundary-deferred"
                            | "capacity-held"
                            | "private-output"
                            | "contract-standalone"
                            | "contract-borrowed"
                            | "contract-fair-standalone"
                            | "contract-fair-borrowed"
                            | "contract-unknown-standalone"
                            | "contract-unknown-borrowed"
                            | "contract-argument-standalone"
                            | "contract-argument-borrowed"
                            | "contract-contention-standalone"
                            | "contract-contention-borrowed"
                            | "schedule-success"
                            | "schedule-retry"
                            | "schedule-compensation"
                            | "schedule-recovery"
                            | "schedule-construction"
                            | "schedule-fairness"
                            | "schedule-queues"
                    ) {
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
        (
            "timeout_ms".into(),
            Value::Num(
                if matches!(
                    behavior,
                    "private-held"
                        | "boundary-held"
                        | "boundary-settled"
                        | "boundary-deferred"
                        | "capacity-held"
                        | "private-output"
                        | "contract-standalone"
                        | "contract-borrowed"
                        | "contract-fair-standalone"
                        | "contract-fair-borrowed"
                        | "contract-unknown-standalone"
                        | "contract-unknown-borrowed"
                        | "contract-argument-standalone"
                        | "contract-argument-borrowed"
                        | "contract-contention-standalone"
                        | "contract-contention-borrowed"
                        | "schedule-success"
                        | "schedule-recovery"
                        | "schedule-construction"
                        | "schedule-fairness"
                        | "schedule-queues"
                ) {
                    "30000"
                } else {
                    "3000"
                }
                .into(),
            ),
        ),
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
    if behavior == "repeated-noisy" {
        handler.insert(
            "argv".into(),
            Value::Arr(
                [
                    executable.to_str().unwrap(),
                    kind,
                    "{directory}",
                    "noisy-exit",
                ]
                .into_iter()
                .map(|value| Value::Str(value.into()))
                .collect(),
            ),
        );
    }
    if kind == "mcp" {
        handler.insert("tool".into(), Value::Str("run".into()));
        handler.insert("arguments".into(), object([]));
    }
    if behavior == "schedule-fairness" {
        let argv = match handler.get_mut("argv").unwrap() {
            Value::Arr(argv) => argv,
            _ => unreachable!(),
        };
        argv[2] = Value::Str(resource.join("quiet").to_str().unwrap().into());
    }
    let mut handlers = Vec::new();
    if behavior == "schedule-compensation" {
        handler.insert(
            "retry".into(),
            parse(
                br#"{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]}"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
        );
        handler.insert(
            "on_failed".into(),
            object([("event", Value::Str("failed".into()))]),
        );
        let mut restore = handler.clone();
        restore.insert("effect".into(), Value::Str("restore".into()));
        handlers.push(Value::Obj(restore));
    }
    if behavior == "schedule-fairness" {
        let mut busy = handler.clone();
        busy.insert("effect".into(), Value::Str("busy".into()));
        let argv = match busy.get_mut("argv").unwrap() {
            Value::Arr(argv) => argv,
            _ => unreachable!(),
        };
        argv[2] = Value::Str(resource.join("busy").to_str().unwrap().into());
        handlers.push(Value::Obj(busy));
    }
    if behavior == "schedule-queues" {
        for index in 0..9 {
            let mut queued = handler.clone();
            queued.insert("effect".into(), Value::Str(format!("notify-{index}")));
            let Value::Arr(argv) = queued.get_mut("argv").unwrap() else {
                unreachable!()
            };
            argv[2] = Value::Str(
                resource
                    .join(format!("run-{index}"))
                    .to_str()
                    .unwrap()
                    .into(),
            );
            handlers.push(Value::Obj(queued));
        }
    } else {
        handlers.push(Value::Obj(handler));
    }
    let mut result = object([
        ("format", Value::Str("fsm.handlers/1".into())),
        ("handlers", Value::Arr(handlers)),
    ]);
    if behavior == "schedule-fairness" {
        let Value::Obj(fields) = &mut result else {
            unreachable!()
        };
        fields.insert("max_inflight".into(), Value::Num("2".into()));
        fields.insert("max_inflight_per_instance".into(), Value::Num("1".into()));
    }
    if behavior == "schedule-queues" {
        let Value::Obj(fields) = &mut result else {
            unreachable!()
        };
        fields.insert("max_inflight".into(), Value::Num("7".into()));
        fields.insert("max_inflight_per_instance".into(), Value::Num("1".into()));
    }
    result
}

fn verify(fixture: &Fixture, behavior: &str) {
    use fsm_core::record::{RecordKind, execution::Claim};
    use fsm_store::store::VerifiedClosure;
    // The observer has exited, so no executor can still append a partial line;
    // read-only replay alone would silently accept an unterminated suffix.
    let verification = fsm_store::journal_io::verify(&fixture.store);
    assert_eq!(
        verification.health,
        fsm_store::journal_io::JournalHealth::Ok
    );
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(verification.records, store.records.len() as u64);
    assert_eq!(store.state.execution.unresolved().count(), 0);
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
    if behavior == "schedule-construction" {
        let counter = read_value(&fixture.directory.join("counter.json"), true).unwrap();
        assert_eq!(number(&counter, "last_allocation").unwrap(), 0);
        assert!(!fixture.directory.join("allocation-1.json").exists());
    }
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
            if behavior == "schedule-construction" {
                0
            } else if behavior == "schedule-queues" {
                9
            } else if behavior == "repeated-noisy" {
                12
            } else if matches!(
                behavior,
                "collected-result"
                    | "closed-result"
                    | "stopped-result"
                    | "acked-result"
                    | "event-result"
                    | "private-held"
                    | "boundary-held"
                    | "boundary-settled"
                    | "boundary-deferred"
                    | "capacity-held"
                    | "private-output"
                    | "contract-standalone"
                    | "contract-borrowed"
                    | "contract-fair-standalone"
                    | "contract-fair-borrowed"
                    | "contract-unknown-standalone"
                    | "contract-unknown-borrowed"
                    | "contract-argument-standalone"
                    | "contract-argument-borrowed"
                    | "contract-contention-standalone"
                    | "contract-contention-borrowed"
                    | "schedule-success"
                    | "schedule-recovery"
            ) {
                1
            } else {
                2
            }
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
        let allocation = number(&domain, "allocation").unwrap();
        if behavior == "claimed-result" && claim.run_id() == 1 {
            // This cut never submitted a manager launch: missing memory evidence
            // is required here, while every actually launched successor retains
            // the unchanged positive memory/swap-limit verification below.
            assert_eq!(
                read_value(
                    &fixture.directory.join(format!("binding-{allocation}.json")),
                    true
                )
                .unwrap(),
                object([
                    ("format", Value::Str("fsm.native-claim-binding/1".into())),
                    ("claim", claim.to_value()),
                    (
                        "journal_claim",
                        Value::Str(format!("sha256:{}", record.hash))
                    ),
                ])
            );
            for name in [
                format!("launch-{allocation}.json"),
                format!("handoff-{allocation}.json"),
                format!("entry-{allocation}.json"),
                format!("entry-{allocation}.json.pending"),
                format!("exec-status-{allocation}.json"),
                format!("fixture-memory-{allocation}.json"),
                format!("completed-{allocation}-{}.json", claim.run_id()),
            ] {
                assert!(!fixture.directory.join(name).try_exists().unwrap());
            }
        } else {
            memory_limits::verify(fixture, &domain, allocation);
        }
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
