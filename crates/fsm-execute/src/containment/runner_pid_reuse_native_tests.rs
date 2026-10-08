//! Genuine kernel PID reuse on explicitly disposable native CI machines.

use super::*;
use std::process::{Child, Command, Stdio};

// The helper owns every fork through a pipe: its death closes the write end
// and retires the sentinel without ever selecting a process by numeric PID.
const SENTINEL: &str = r#"import json,os,signal,sys
from pathlib import Path
assert os.environ['FSM_NATIVE_FIXTURE_PID_REUSE']=='1'
target=int(sys.argv[1])
ready=Path(sys.argv[2])
for attempt in range(8):
    reader,writer=os.pipe()
    Path('/proc/sys/kernel/ns_last_pid').write_text(str(target-1))
    child=os.fork()
    if child==0:
        os.close(writer)
        signal.alarm(60)
        while os.read(reader,1):
            pass
        os._exit(0)
    os.close(reader)
    if child==target:
        break
    os.close(writer)
    os.waitpid(child,0)
else:
    raise AssertionError('kernel did not allocate the original PID within eight attempts')
try:
    pending=ready.with_suffix('.pending')
    pending.write_text(json.dumps(dict(pid=child)))
    pending.replace(ready)
    sys.stdin.buffer.read()
finally:
    os.close(writer)
    _,status=os.waitpid(child,0)
    assert status==0
"#;

pub(super) struct Sentinel {
    owner: Child,
    pid: u64,
    original_start: u64,
    reused_start: u64,
    membership: String,
}

impl Sentinel {
    pub(super) fn new(fixture: &Fixture, unit: &str, pid: u64) -> Self {
        assert_eq!(
            std::env::var("FSM_NATIVE_FIXTURE_PID_REUSE").as_deref(),
            Ok("1"),
            "PID cursor writes require an explicitly disposable native fixture"
        );
        let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
        assert_eq!(number(handoff.get("gate").unwrap(), "pid").unwrap(), pid);
        let original_start = start_time(pid);
        // Retire only the main process of this independently recorded fixture
        // unit; ExitType=cgroup retains its detached descendants for recovery.
        assert!(
            Command::new("/usr/bin/systemctl")
                .args(["kill", "--kill-whom=main", "--signal=KILL", unit])
                .status()
                .unwrap()
                .success()
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while Path::new(&format!("/proc/{pid}")).exists() {
            assert!(Instant::now() < deadline, "original gate was not reaped");
            std::thread::sleep(Duration::from_millis(5));
        }
        let ready = fixture.directory.join("pid-reuse-ready.json");
        assert!(!ready.exists());
        let owner = Command::new("/usr/bin/python3")
            .args(["-c", SENTINEL, &pid.to_string()])
            .arg(&ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut sentinel = Self {
            owner,
            pid,
            original_start,
            reused_start: 0,
            membership: String::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(3);
        while !ready.exists() {
            assert!(sentinel.owner.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "PID reuse sentinel was not ready"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            number(&read_value(&ready, false).unwrap(), "pid").unwrap(),
            pid
        );
        sentinel.reused_start = start_time(pid);
        assert_ne!(sentinel.reused_start, original_start);
        sentinel.membership = fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap();
        assert_ne!(sentinel.membership, format!("0::/system.slice/{unit}\n"));
        sentinel.assert_live();
        sentinel
    }

    pub(super) fn assert_live(&mut self) {
        assert!(self.owner.try_wait().unwrap().is_none());
        assert_eq!(start_time(self.pid), self.reused_start);
        assert_eq!(
            fs::read_to_string(format!("/proc/{}/cgroup", self.pid)).unwrap(),
            self.membership
        );
    }

    pub(super) fn report(&self) {
        #[allow(clippy::print_stdout)] // Retained independent kernel identity evidence.
        {
            println!(
                "FSM_NATIVE_PID_REUSE_SURVIVED {} {} {}",
                self.pid, self.original_start, self.reused_start
            );
        }
    }

    pub(super) fn retire(mut self) {
        drop(self.owner.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = self.owner.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "owned PID sentinel did not retire cleanly"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "owned PID sentinel retirement timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        drop(self.owner.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if self.owner.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                // Killing only our actual helper Child also closes its pipe;
                // the forked sentinel observes EOF, including on assertion failure.
                let _ = self.owner.kill();
                let _ = self.owner.wait();
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn start_time(pid: u64) -> u64 {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    stat.rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap()
        .parse()
        .unwrap()
}
