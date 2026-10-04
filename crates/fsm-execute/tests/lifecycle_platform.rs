//! Negative feasibility evidence, not a containment backend acceptance suite.
//! The same executable supplies native fixtures without shell dependencies.

#[cfg(target_os = "linux")]
#[path = "lifecycle_platform/broker.rs"]
mod broker;
#[cfg(target_os = "linux")]
#[path = "lifecycle_platform/identity_root.rs"]
mod identity_root;

#[cfg(target_os = "linux")]
#[path = "lifecycle_platform/pipe_cancel.rs"]
mod pipe_cancel;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn fixture(directory: &Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command
        .args(["native_fixture", "--exact", "--nocapture"])
        .env("FSM_LIFECYCLE_PROBE_DIRECTORY", directory)
        .env("FSM_LIFECYCLE_PROBE_MODE", mode);
    command
}

#[test]
#[allow(clippy::zombie_processes)] // Root exit before descendant exit is the negative case under test.
fn native_fixture() {
    let Some(directory) = std::env::var_os("FSM_LIFECYCLE_PROBE_DIRECTORY") else {
        return;
    };
    let directory = PathBuf::from(directory);
    let mode = std::env::var("FSM_LIFECYCLE_PROBE_MODE").expect("fixture mode");
    if mode == "identity-publication" {
        #[cfg(target_os = "linux")]
        {
            fs::write(directory.join("publication-ready"), b"ready").expect("publication barrier");
            await_file(&directory.join("publication-release"));
            for value in 0..64 {
                identity_root::put_public(&directory.join("publication"), &format!("{value}\n"))
                    .expect("readable atomic publication");
                let deadline = Instant::now();
                while !identity_root::read(&directory.join("work/publication-ack"))
                    .is_ok_and(|ack| ack.trim() == value.to_string())
                {
                    assert!(
                        deadline.elapsed() < Duration::from_secs(3),
                        "publication reader missing"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            fs::write(directory.join("publication-done"), b"done").expect("publication completion");
        }
        #[cfg(not(target_os = "linux"))]
        panic!("native publication requires Linux");
        #[cfg(target_os = "linux")]
        return;
    }
    if let Some(handle) = mode.strip_prefix("identity-lease-client:") {
        #[cfg(target_os = "linux")]
        broker::client(&directory, handle).expect("native lease client");
        #[cfg(not(target_os = "linux"))]
        panic!("native lease client requires Linux: {handle}");
        #[cfg(target_os = "linux")]
        return;
    }
    if mode == "identity-pre-entry" {
        #[cfg(not(target_os = "linux"))]
        panic!("native pre-entry requires Linux");
        #[cfg(target_os = "linux")]
        {
            // Trusted pre-start fixture work only; the actual handler has not
            // reached its entry gate. Provisioning owns the release barrier.
            fs::write(directory.join("entry-ready"), b"pending start job")
                .expect("pre-entry barrier");
            await_file(&directory.parent().expect("namespace").join("release-entry"));
            return;
        }
    }
    if let Some(uid) = mode.strip_prefix("identity-broker:") {
        #[cfg(target_os = "linux")]
        broker::serve(&directory, uid.parse().expect("operator UID"))
            .expect("native broker prototype");
        #[cfg(not(target_os = "linux"))]
        panic!("native broker requires Linux: {uid}");
        #[cfg(target_os = "linux")]
        return;
    }
    if let Some(operation) = mode.strip_prefix("identity:") {
        #[cfg(target_os = "linux")]
        identity_root::run(&directory, operation).expect("native identity prototype");
        #[cfg(not(target_os = "linux"))]
        panic!("native identity prototype requires Linux: {operation}");
        #[cfg(target_os = "linux")]
        return;
    }
    let mode = if let Some(handle) = mode.strip_prefix("identity-gate:") {
        #[cfg(target_os = "linux")]
        {
            identity_root::gate(&directory, handle).expect("trusted native entry gate");
            "kill".to_owned()
        }
        #[cfg(not(target_os = "linux"))]
        panic!("native entry gate requires Linux: {handle}");
    } else {
        mode
    };
    if std::env::var_os("FSM_LIFECYCLE_PROBE_CONTAINED").is_some() {
        // Record native membership before spawning or external fixture work.
        let membership = fs::read_to_string("/proc/self/cgroup").expect("native Linux cgroup");
        let status = fs::read_to_string("/proc/self/status").expect("native Linux identity");
        fs::write(directory.join(format!("{mode}-cgroup")), membership).expect("membership");
        fs::write(directory.join(format!("{mode}-status")), status).expect("identity");
        assert!(
            fs::write(
                "/sys/fs/cgroup/cgroup.procs",
                std::process::id().to_string()
            )
            .is_err(),
            "handler must have no root-domain migration authority"
        );
        fs::write(
            directory.join(format!("{mode}-migration-refused")),
            b"refused",
        )
        .expect("migration observation");
    }
    if mode == "forker" {
        fs::write(directory.join("ready"), b"ready").expect("ready barrier");
        await_file(&directory.join("fork-during-stop"));
        let mut leaves = Vec::new();
        for index in 0..2 {
            let role = format!("leaf-{index}");
            leaves.push(fixture(&directory, &role).spawn().expect("late descendant"));
            await_file(&directory.join(format!("{role}-ready")));
        }
        fs::write(directory.join("fork-complete"), b"two descendants").expect("fork barrier");
        for mut leaf in leaves {
            leaf.wait().expect("leaf wait");
        }
        return;
    }
    if mode.starts_with("leaf-") {
        fs::write(directory.join(format!("{mode}-ready")), b"ready").expect("leaf barrier");
        let watchdog = Instant::now();
        while watchdog.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        return;
    }
    if mode == "descendant" {
        fs::write(directory.join("ready"), b"ready").expect("ready barrier");
        let watchdog = Instant::now();
        while !directory.join("stop").exists() && watchdog.elapsed() < Duration::from_secs(10) {
            if directory.join("challenge").exists() {
                fs::write(directory.join("response"), b"alive").expect("liveness response");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if std::env::var_os("FSM_LIFECYCLE_SOCKET_CANCEL_FIXTURE").is_some() {
            fs::write(
                directory.join("descendant-stopped"),
                b"fixture stop observed",
            )
            .expect("cleanup observation");
        }
        return;
    }
    let descendant_mode = if mode == "spawn-stop" {
        "forker"
    } else {
        "descendant"
    };
    let mut descendant_command = fixture(&directory, descendant_mode);
    #[cfg(target_os = "linux")]
    if mode == "escape" {
        use std::os::unix::process::CommandExt;
        // Leaving the process group must not leave the native domain.
        descendant_command.process_group(0);
    }
    let mut descendant = descendant_command.spawn().expect("descendant");
    await_file(&directory.join("ready"));
    fs::write(directory.join("root-ready"), b"ready").expect("root barrier");
    if mode == "exit" {
        // Deliberately leave the descendant alive; its watchdog bounds failure cleanup.
        return;
    }
    descendant.wait().expect("descendant wait");
}

fn await_file(path: &Path) {
    let watchdog = Instant::now();
    while !path.exists() {
        assert!(
            watchdog.elapsed() < Duration::from_secs(5),
            "missing barrier: {path:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct Probe {
    directory: PathBuf,
    root: Child,
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = fs::write(self.directory.join("stop"), b"stop");
        let _ = self.root.kill();
        let _ = self.root.wait();
        // Do not remove the stop barrier before the orphan has observed it.
        // The test removes the directory after EOF, which proves fixture exit.
    }
}

fn prove_direct_child_is_insufficient(mode: &str) {
    let directory = std::env::temp_dir().join(format!(
        "fsm-lifecycle-negative-{}-{mode}",
        std::process::id()
    ));
    fs::create_dir(&directory).expect("unique probe directory");
    let root = fixture(&directory, mode)
        .stdout(Stdio::piped())
        .spawn()
        .expect("root fixture");
    let mut probe = Probe { directory, root };
    let mut stdout = probe.root.stdout.take().expect("stdout pipe");
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let result = std::io::copy(&mut stdout, &mut std::io::sink());
        let _ = sender.send(result);
    });
    await_file(&probe.directory.join("root-ready"));
    if mode == "kill" {
        probe.root.kill().expect("direct-child termination");
    }
    probe.root.wait().expect("root reaped");
    fs::write(probe.directory.join("challenge"), b"challenge").expect("challenge");
    await_file(&probe.directory.join("response"));
    assert!(
        matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "a surviving descendant must retain the pipe after root death"
    );
    fs::write(probe.directory.join("stop"), b"stop").expect("fixture cleanup");
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("bounded fixture EOF")
        .expect("pipe read");
    reader.join().expect("reader joined");
    fs::remove_dir_all(&probe.directory).expect("probe cleanup");
}

#[test]
fn direct_child_kill_leaves_descendant_and_pipe_alive() {
    prove_direct_child_is_insufficient("kill");
}

#[test]
fn normal_root_exit_leaves_descendant_and_pipe_alive() {
    prove_direct_child_is_insufficient("exit");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn provisioned_native_modes_refuse_before_fixture_work() {
    let directory =
        std::env::temp_dir().join(format!("fsm-lifecycle-unsupported-{}", std::process::id()));
    fs::create_dir(&directory).expect("unique unsupported probe directory");
    for (mode, reason) in [
        ("identity-publication", "native publication requires Linux"),
        (
            "identity-lease-client:1:2:unavailable",
            "native lease client requires Linux",
        ),
        ("identity-pre-entry", "native pre-entry requires Linux"),
        ("identity-broker:1000", "native broker requires Linux"),
        (
            "identity:allocate",
            "native identity prototype requires Linux",
        ),
        (
            "identity-gate:1:2:unavailable",
            "native entry gate requires Linux",
        ),
    ] {
        let output = fixture(&directory, mode)
            .output()
            .expect("unsupported fixture");
        assert!(
            !output.status.success(),
            "unsupported mode was accepted: {mode}"
        );
        let diagnostics = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        assert!(diagnostics.contains(reason), "wrong refusal: {diagnostics}");
        assert_eq!(
            fs::read_dir(&directory).expect("probe directory").count(),
            0
        );
    }
    fs::remove_dir(&directory).expect("unsupported probe cleanup");
}
