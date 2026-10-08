//! CLI closure of an enrolled MCP tree after its fixture-owned runner dies.

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
            "original MCP runner exited before enrollment"
        );
        if Instant::now() >= deadline {
            let _ = owner.kill();
            owner.wait().unwrap();
            panic!("owned MCP runner enrollment timed out; authority retained");
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
    // MCP root, detached child and grandchild must survive until CLI closure.
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
    recovery.resume_via_cli();
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
    fs::write(
        barriers.path.join("release"),
        b"sequential MCP enrollment verified",
    )
    .unwrap();
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

#[test]
#[ignore = "fixture-owned Root original runner for an enrolled MCP tree"]
fn original_runner() {
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let directory = PathBuf::from(std::env::var_os("FSM_NATIVE_ORPHAN_AUTHORITY").unwrap());
    runner::execute(&directory, 1).unwrap();
}
