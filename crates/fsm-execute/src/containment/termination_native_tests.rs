//! Administrative tree fixtures exercise production kernel submission only.

use super::super::super::{identity, observation, termination};
use super::Fixture;
use std::fs;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Children<'a> {
    fixture: &'a Fixture,
    roots: Vec<Child>,
}

impl Drop for Children<'_> {
    fn drop(&mut self) {
        let (group, expected) = &self.fixture.groups[0];
        if fs::symlink_metadata(group).is_ok_and(|metadata| identity(&metadata) == *expected) {
            let _ = fs::write(group.join("cgroup.kill"), b"1");
        }
        for child in &mut self.roots {
            let _ = child.kill();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        for child in &mut self.roots {
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                    Ok(Some(_)) | Err(_) => break,
                }
            }
        }
    }
}

pub(super) fn members(fixture: &Fixture) {
    let group = &fixture.groups[0].0;
    let mut children = Children {
        fixture,
        roots: Vec::new(),
    };
    children.roots.push(
        Command::new("/usr/bin/sleep")
            .arg("300")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let (mut reader, writer) = UnixStream::pair().unwrap();
    reader.set_nonblocking(true).unwrap();
    let writer: OwnedFd = writer.into();
    let membership = format!(
        "0::{}\n",
        group.strip_prefix("/sys/fs/cgroup").unwrap().display()
    );
    children.roots.push(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "authority::allocator::native_tests::termination_cases::descendant_fixture",
                "--ignored",
                "--nocapture",
                "--color",
                "never",
            ])
            .env("FSM_TERMINATION_FIXTURE_CGROUP", &membership)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(writer))
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    for child in &children.roots {
        fs::write(group.join("cgroup.procs"), child.id().to_string()).unwrap();
    }
    // Only the administrative fixture has run so far: its descendant is
    // released after enrollment, and must inherit this actual cgroup.
    children.roots[1]
        .stdin
        .take()
        .unwrap()
        .write_all(b"1")
        .unwrap();
    let descendant = ready(&mut reader);
    assert_eq!(
        fs::read_to_string(format!("/proc/{descendant}/cgroup")).unwrap(),
        membership
    );
    let members = fs::read_to_string(group.join("cgroup.procs")).unwrap();
    for pid in children.roots.iter().map(Child::id).chain([descendant]) {
        assert!(members.lines().any(|line| line == pid.to_string()));
    }
    let observed = observation::read(&fixture.directory, 1).unwrap();
    assert_eq!(
        observed.get("populated"),
        Some(&fsm_core::json::Value::Bool(true))
    );
    assert_eq!(
        observed.get("closing"),
        Some(&fsm_core::json::Value::Bool(true))
    );
    termination::request(&fixture.directory, 1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    for child in &mut children.roots {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(!status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native fixture termination timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    loop {
        if fs::read_to_string(group.join("cgroup.events"))
            .unwrap()
            .lines()
            .any(|line| line == "populated 0")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native descendant remains populated"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!fixture.directory.join("closed-1.json").exists());
    let observed = observation::read(&fixture.directory, 1).unwrap();
    assert_eq!(
        observed.get("populated"),
        Some(&fsm_core::json::Value::Bool(false))
    );
    assert_eq!(
        observed.get("closing"),
        Some(&fsm_core::json::Value::Bool(true))
    );
}

fn ready(reader: &mut UnixStream) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0; 512];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => panic!("native descendant fixture exited before readiness"),
            Ok(count) => {
                assert!(
                    count <= 4096 - bytes.len(),
                    "native fixture output exceeds bound"
                );
                bytes.extend_from_slice(&chunk[..count]);
                let output = std::str::from_utf8(&bytes).unwrap();
                if let Some((pid, _)) = output
                    .split_once("native-descendant=")
                    .and_then(|(_, remaining)| remaining.split_once('\n'))
                {
                    return pid.parse().unwrap();
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("native fixture read failed: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "native descendant readiness timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "re-executed only after administrative native enrollment"]
fn descendant_fixture() {
    let expected = std::env::var("FSM_TERMINATION_FIXTURE_CGROUP").unwrap();
    let mut release = [0; 1];
    std::io::stdin().read_exact(&mut release).unwrap();
    assert_eq!(release, *b"1");
    assert_eq!(fs::read_to_string("/proc/self/cgroup").unwrap(), expected);
    let mut descendant = Command::new("/usr/bin/sleep")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert_eq!(
        fs::read_to_string(format!("/proc/{}/cgroup", descendant.id())).unwrap(),
        expected
    );
    writeln!(
        std::io::stdout().lock(),
        "native-descendant={}",
        descendant.id()
    )
    .unwrap();
    while descendant.try_wait().unwrap().is_none() {
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("fixture descendant exited without production native termination");
}
