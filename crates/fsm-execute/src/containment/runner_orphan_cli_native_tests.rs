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
    fixture.cleanup().unwrap();
}

#[test]
#[ignore = "fixture-owned Root original runner for an enrolled MCP tree"]
fn original_runner() {
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let directory = PathBuf::from(std::env::var_os("FSM_NATIVE_ORPHAN_AUTHORITY").unwrap());
    runner::execute(&directory, 1).unwrap();
}
