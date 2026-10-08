//! CLI closure of an enrolled native tree after its fixture-owned runner dies.

use super::*;
use std::process::{Command, Stdio};

pub(super) fn run(fixture: &mut Fixture, barriers: &Barriers, domain: &NativeDomain) {
    super::super::broker_cases::disconnect_cases::install_supervisor(&fixture.directory);
    let output = fixture.directory.join("orphan-runner.log");
    let mut owner = Command::new(fixture.directory.join("supervisor-test"))
        .args([
            "--exact",
            "authority::allocator::native_tests::runner_cases::orphan_cli::original_runner",
            "--ignored",
            "--nocapture",
        ])
        .env("FSM_NATIVE_ORPHAN_AUTHORITY", &fixture.directory)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output).unwrap())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !barriers.path.join("root-ready").exists()
        || !barriers.path.join("descendant-ready").exists()
    {
        assert!(
            owner.try_wait().unwrap().is_none(),
            "original native runner exited before enrollment"
        );
        if Instant::now() >= deadline {
            let _ = owner.kill();
            owner.wait().unwrap();
            panic!("owned native runner enrollment timed out; authority retained");
        }
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
    }
    fs::write(
        barriers.path.join("release"),
        b"independent enrollment verified",
    )
    .unwrap();
    let recovery = super::orphan_recovery::Session::new(fixture);
    recovery.refuse_live_runner();
    // Kill only this fixture's actual runner child; its independent enrolled
    // native root, detached child and grandchild must survive until CLI closure.
    owner.kill().unwrap();
    assert!(!owner.wait().unwrap().success());
    for pid in pids {
        assert_eq!(
            fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
            membership
        );
    }
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
    refuse_changed_domain(fixture, &recovery, &membership, &pids);
    let mut reused = super::pid_reuse::Sentinel::new(fixture, &unit, pids[0]);
    // The original numeric gate PID now belongs to a different live process;
    // it cannot supply missing domain identity or authorize claim clearance.
    refuse_changed_domain(fixture, &recovery, &membership, &pids[1..]);
    reused.assert_live();
    recovery.resume_via_cli();
    reused.assert_live();
    reused.report();
    reused.retire();
    for pid in pids {
        assert!(
            !fs::read_to_string(format!("/proc/{pid}/cgroup"))
                .is_ok_and(|observed| observed == membership)
        );
    }
    drop(recovery);
    restart_after_closure(fixture, barriers, &unit);
    fixture.cleanup().unwrap();
}

fn refuse_changed_domain(
    fixture: &Fixture,
    recovery: &super::orphan_recovery::Session<'_>,
    membership: &str,
    pids: &[u64],
) {
    let path = fixture.directory.join("prepared-1.json");
    let original = fs::read(&path).unwrap();
    let original_metadata = fs::symlink_metadata(&path).unwrap();
    let mut prepared = read_value(&path, true).unwrap().as_obj().unwrap().clone();
    let mut domain = prepared["domain"].as_obj().unwrap().clone();
    let mut group = domain["cgroup"].as_obj().unwrap().clone();
    let inode = number(&Value::Obj(group.clone()), "inode").unwrap();
    group.insert(
        "inode".into(),
        Value::Num(inode.checked_add(1).unwrap().to_string()),
    );
    domain.insert("cgroup".into(), Value::Obj(group));
    prepared.insert("domain".into(), Value::Obj(domain));
    // Alter only fixture-owned protected evidence, preserving the original
    // physical allocation; this is an identity-mismatch case, not PID reuse.
    fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(prepared))).unwrap();
    fs::File::open(&path).unwrap().sync_all().unwrap();
    recovery.refuse_changed_domain();
    for pid in pids {
        assert_eq!(
            fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
            membership
        );
    }
    for name in ["closing-1.json", "closed-1.json", "manager-stopped-1.json"] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    fs::write(&path, &original).unwrap();
    fs::File::open(&path).unwrap().sync_all().unwrap();
    let restored = fs::symlink_metadata(&path).unwrap();
    assert_eq!(
        (
            restored.dev(),
            restored.ino(),
            restored.uid(),
            restored.mode()
        ),
        (
            original_metadata.dev(),
            original_metadata.ino(),
            original_metadata.uid(),
            original_metadata.mode()
        )
    );
    assert_eq!(fs::read(path).unwrap(), original);
}

fn restart_after_closure(fixture: &mut Fixture, barriers: &Barriers, original_unit: &str) {
    use fsm_execute::run::{Pipeline, native_client::NativeCompletion};
    use fsm_store::clock::GlobalClock;
    assert!(
        !Path::new("/sys/fs/cgroup/system.slice")
            .join(original_unit)
            .exists()
    );
    for name in ["root-ready", "descendant-ready", "release"] {
        let path = barriers.path.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                assert!(metadata.is_file());
                fs::remove_file(path).unwrap();
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("successor marker inspection failed: {error}"),
        }
    }
    let successor = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let mut writer = Store::open(&fixture.store).unwrap();
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    let effect = writer.state.instances["instance"].pending[0].clone();
    let approved = super::super::super::super::catalogue::read(&fixture.directory).unwrap();
    Pipeline
        .claim_native_handler(
            &mut writer,
            &mut GlobalClock,
            &effect,
            &approved.handlers["notify"],
            &successor,
            "orphan-mcp-successor",
        )
        .unwrap();
    let claim = writer
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    assert_eq!(claim.run_id(), 2);
    let hash = writer.current_execution_claim_hash(&claim).unwrap();
    drop(writer);
    bind(
        &fixture.directory,
        &object([
            ("format", Value::Str("fsm.native-claim-binding/1".into())),
            ("claim", claim.to_value()),
            ("journal_claim", Value::Str(hash.clone())),
        ]),
    )
    .unwrap();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let control = cancelled.clone();
    let directory = fixture.directory.clone();
    let execution =
        std::thread::spawn(move || runner::execute_cancellable(&directory, 2, &control));
    let deadline = Instant::now() + Duration::from_secs(8);
    while !barriers.path.join("root-ready").exists()
        || !barriers.path.join("descendant-ready").exists()
    {
        assert!(
            !execution.is_finished(),
            "successor ended before MCP enrollment"
        );
        assert!(
            Instant::now() < deadline,
            "successor MCP enrollment deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let root = read_value(&barriers.path.join("root-ready"), false).unwrap();
    let descendants = read_value(&barriers.path.join("descendant-ready"), false).unwrap();
    let unit = format!(
        "fsm-containment-{}-1-2.service",
        text(&successor.to_value(), "namespace").unwrap()
    );
    fs::write(
        barriers.path.join("release"),
        b"sequential MCP enrollment verified",
    )
    .unwrap();
    replay_original_while_successor_lives(fixture);
    assert!(
        !execution.is_finished(),
        "stale CLI reconciliation ended the MCP successor"
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
    assert!(
        !Path::new("/sys/fs/cgroup/system.slice")
            .join(original_unit)
            .exists()
    );
    cancelled.store(true, std::sync::atomic::Ordering::Release);
    while !execution.is_finished() {
        assert!(Instant::now() < deadline, "successor MCP closure deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = execution.join().unwrap().unwrap();
    let completion = NativeCompletion::verify(
        &object([
            ("format", Value::Str("fsm.native-response/1".into())),
            ("ok", Value::Bool(true)),
            ("result", result),
        ]),
        &claim,
        &hash,
    )
    .unwrap();
    assert_eq!(completion.stopped_outcome().status(), "interrupted");
    let mut writer = Store::open(&fixture.store).unwrap();
    Pipeline
        .stop_native(
            &mut writer,
            &mut GlobalClock,
            &claim,
            &completion,
            "orphan-mcp-successor-stop",
        )
        .unwrap();
    Pipeline
        .settle_native_stopped(&mut writer, &mut GlobalClock, &claim, &completion)
        .unwrap();
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    assert!(!Path::new("/sys/fs/cgroup/system.slice").join(unit).exists());
}

fn replay_original_while_successor_lives(fixture: &Fixture) {
    let before = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    // Start both production callers before waiting for either: duplicate
    // reconciliation must remain idempotent while another run owns the tree.
    let mut callers = [0, 1].map(|index| start_original_replay(fixture, index));
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut statuses = [None, None];
    loop {
        for (index, (child, _, _)) in callers.iter_mut().enumerate() {
            if statuses[index].is_none() {
                statuses[index] = child.try_wait().unwrap();
            }
        }
        if statuses.iter().all(Option::is_some) {
            break;
        }
        if Instant::now() >= deadline {
            // Reap every outstanding fixture-owned caller before retaining the
            // authority; dropping a Child alone would leave its process alive.
            for (index, (child, _, _)) in callers.iter_mut().enumerate() {
                if statuses[index].is_none() {
                    let _ = child.kill();
                    child.wait().unwrap();
                }
            }
            panic!("stale MCP CLI replay exceeded its bound; authority retained");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut responses = Vec::new();
    for ((_, output, errors), status) in callers.into_iter().zip(statuses) {
        assert!(fs::metadata(&output).unwrap().len() <= 8192);
        assert!(fs::metadata(&errors).unwrap().len() <= 8192);
        responses.push((status.unwrap(), output, errors));
    }
    assert!(responses.iter().any(|(status, _, _)| status.success()));
    for (status, output, errors) in responses {
        if !status.success() {
            // The store deliberately refuses simultaneous writers; contention
            // is permitted only as the exact typed lock refusal.
            assert_eq!(status.code(), Some(4));
            assert!(fs::read(&output).unwrap().is_empty());
            let refusal = fsm_core::json::parse(
                &fs::read(&errors).unwrap(),
                &fsm_core::json::JsonLimits::DEFAULT,
            )
            .unwrap();
            assert_eq!(refusal.get("code"), Some(&Value::Str("store/lock".into())));
            continue;
        }
        let response = fsm_core::json::parse(
            &fs::read(output).unwrap(),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        assert_eq!(response.get("duplicate"), Some(&Value::Bool(true)));
        assert_eq!(
            response.get("execution").unwrap().get("run_id"),
            Some(&Value::Num("1".into()))
        );
        assert_eq!(
            response.get("execution").unwrap().get("disposition"),
            Some(&Value::Str("interrupted".into()))
        );
    }
    let observed = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(observed.records, before);
    assert_eq!(
        observed
            .state
            .execution
            .unresolved()
            .next()
            .unwrap()
            .0
            .run_id(),
        2
    );
}

fn start_original_replay(
    fixture: &Fixture,
    index: usize,
) -> (std::process::Child, PathBuf, PathBuf) {
    let output = fixture
        .directory
        .join(format!("stale-reconcile-{index}.stdout"));
    let errors = fixture
        .directory
        .join(format!("stale-reconcile-{index}.stderr"));
    let child = Command::new("/usr/bin/python3")
        .args(["-c", "import os,sys;os.setgroups([]);os.setgid(65534);os.setuid(65534);os.execv(sys.argv[1],sys.argv[1:])"])
        .arg(fixture.directory.join("recovery-cli"))
        .args(["--json", "--data-dir"])
        .arg(&fixture.store)
        .args(["execute", "reconcile", "--run-id", "1", "--timeout-ms", "1000"])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output).unwrap())
        .stderr(fs::File::create(&errors).unwrap())
        .spawn().unwrap();
    (child, output, errors)
}

#[test]
#[ignore = "fixture-owned Root original runner for an enrolled native tree"]
fn original_runner() {
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let directory = PathBuf::from(std::env::var_os("FSM_NATIVE_ORPHAN_AUTHORITY").unwrap());
    runner::execute(&directory, 1).unwrap();
}
