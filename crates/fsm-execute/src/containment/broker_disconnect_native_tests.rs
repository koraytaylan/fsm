//! Independent client death closes process and MCP trees before their timeout.

use super::super::runner_cases::{Barriers, SERVER};
use super::{
    Daemon, Fixture, broker_endpoint, canon_bytes, cgroup, claim_binding, identity, number, object,
    origin, read_value, request,
};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CLIENT: &str = r#"import json,os,socket,sys
os.setgroups([])
os.setgid(65534)
os.setuid(65534)
assert os.getuid()==65534 and os.geteuid()==65534 and os.getgroups()==[]
from pathlib import Path
base=Path(sys.argv[1])
encoded=sys.argv[2].encode()
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

const WATCH_CLIENT: &str = r#"import os,sys
os.setgroups([])
os.setgid(65534)
os.setuid(65534)
assert os.getuid()==65534 and os.geteuid()==65534 and os.getgroups()==[]
from pathlib import Path
authority=Path(sys.argv[1]).parent
os.execv('/usr/libexec/fsm-containment-authority',['fsm-containment-authority','client-watch',authority.parent.name,authority.name.removeprefix('authority-')])
"#;

struct Client(Child);

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

fn table(path: &Path, mode: &str) -> Value {
    let handler = object([
        ("effect", Value::Str("notify".into())),
        (
            "kind",
            Value::Str(
                if mode == "cancel-mcp" {
                    "mcp"
                } else {
                    "process"
                }
                .into(),
            ),
        ),
        (
            "argv",
            Value::Arr(
                [
                    "/usr/bin/python3",
                    "-c",
                    SERVER,
                    path.to_str().unwrap(),
                    mode,
                ]
                .into_iter()
                .map(|argument| Value::Str(argument.into()))
                .collect(),
            ),
        ),
        ("timeout_ms", Value::Num("10000".into())),
        (
            "retry",
            object([
                ("attempts", Value::Num("1".into())),
                ("backoff_ms", Value::Num("10".into())),
                ("max_backoff_ms", Value::Num("10".into())),
                ("on", Value::Arr(vec![])),
            ]),
        ),
    ]);
    let mut fields = handler.as_obj().unwrap().clone();
    if mode == "cancel-mcp" {
        fields.insert("tool".into(), Value::Str("probe".into()));
        fields.insert(
            "arguments".into(),
            object([("fixture", Value::Str("native".into()))]),
        );
    }
    object([
        ("format", Value::Str("fsm.handlers/1".into())),
        ("handlers", Value::Arr(vec![Value::Obj(fields)])),
    ])
}

pub(super) fn run() {
    for (mode, watch) in [
        ("cancel-process", false),
        ("cancel-mcp", false),
        ("cancel-process", true),
        ("cancel-mcp", true),
    ] {
        let barriers = Barriers::new();
        let mut fixture = Fixture::new_for_table(table(&barriers.path, mode));
        broker_endpoint::provision(&fixture.directory, 65534).unwrap();
        let base = fixture.directory.join("broker");
        let mut daemon = Daemon::ready(&fixture.directory, 1);
        let preparation = request(&base, "prepare", Value::Null);
        assert_eq!(preparation.get("ok"), Some(&Value::Bool(true)));
        let domain = NativeDomain::from_value(preparation.get("result").unwrap()).unwrap();
        let group = cgroup(&origin(&fixture.directory).unwrap(), 1).unwrap();
        fixture.groups.push((
            group.clone(),
            domain.to_value().get("cgroup").unwrap().clone(),
        ));
        let (binding, effect) = claim_binding(&fixture, &domain);
        assert_eq!(
            request(&base, "bind", binding.clone()).get("ok"),
            Some(&Value::Bool(true))
        );
        let execution = object([
            ("format", Value::Str("fsm.native-request/1".into())),
            ("action", Value::Str("execute".into())),
            ("payload", Value::Num("1".into())),
        ]);
        let mut lifetime = None;
        let mut command = Command::new("/usr/bin/python3");
        command
            .args(["-c", if watch { WATCH_CLIENT } else { CLIENT }])
            .arg(&base);
        if watch {
            let (mut owner, input) = UnixStream::pair().unwrap();
            input.set_nonblocking(true).unwrap();
            let bytes = canon_bytes(&execution);
            owner
                .write_all(&(bytes.len() as u32).to_be_bytes())
                .unwrap();
            owner.write_all(&bytes).unwrap();
            command.stdin(Stdio::from(OwnedFd::from(input)));
            lifetime = Some(owner);
        } else {
            command
                .arg(std::str::from_utf8(&canon_bytes(&execution)).unwrap())
                .stdin(Stdio::piped());
        }
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut client = Client(command.spawn().unwrap());
        drop(command);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !barriers.path.join("root-ready").exists() {
            assert!(
                client.0.try_wait().unwrap().is_none(),
                "client exited before tree enrollment"
            );
            assert!(daemon.0.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "broker tree enrollment timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            fs::metadata(format!("/proc/{}", client.0.id()))
                .unwrap()
                .uid(),
            65534
        );
        assert_eq!(
            identity(&fs::metadata(format!("/proc/{}/exe", client.0.id())).unwrap()),
            identity(&fs::metadata("/usr/libexec/fsm-containment-authority").unwrap())
        );
        let root = read_value(&barriers.path.join("root-ready"), false).unwrap();
        let descendants = read_value(&barriers.path.join("descendant-ready"), false).unwrap();
        let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
        let pid = number(&root, "pid").unwrap();
        assert_eq!(number(handoff.get("gate").unwrap(), "pid").unwrap(), pid);
        let membership = format!(
            "0::/system.slice/{}\n",
            group.file_name().unwrap().to_str().unwrap()
        );
        for pid in [
            pid,
            number(&descendants, "pid").unwrap(),
            number(&descendants, "grandchild").unwrap(),
        ] {
            assert_eq!(
                fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap(),
                membership
            );
            assert!((61184..=65519).contains(&fs::metadata(format!("/proc/{pid}")).unwrap().uid()));
        }
        assert!(
            fs::read_to_string(group.join("cgroup.events"))
                .unwrap()
                .lines()
                .any(|line| line == "populated 1")
        );
        let receipt = fixture.directory.join(format!(
            "closure-1-{}.json",
            number(binding.get("claim").unwrap(), "run_id").unwrap()
        ));
        assert!(VerifiedClosure::read(&receipt).is_err());
        // No cancellation request is sent: either kill the helper or close
        // the only supervisor lifetime endpoint while leaving the helper alive.
        if watch {
            drop(lifetime.take());
        } else {
            client.0.kill().unwrap();
        }
        fs::write(
            barriers.path.join("release"),
            b"enrollment independently verified",
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            assert!(
                daemon.0.try_wait().unwrap().is_none(),
                "broker died during cleanup"
            );
            let single_thread = fs::read_to_string(format!("/proc/{}/status", daemon.0.id()))
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("Threads:"))
                .is_some_and(|raw| raw.trim() == "1");
            if VerifiedClosure::read(&receipt).is_ok() && !group.exists() && single_thread {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "client death did not retire broker workers and tree before timeout"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        if watch {
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                if let Some(status) = client.0.try_wait().unwrap() {
                    assert!(
                        !status.success(),
                        "lost supervisor produced successful helper response"
                    );
                    break;
                }
                assert!(Instant::now() < deadline, "helper survived lifetime EOF");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        drop(client);
        let stopped = read_value(&fixture.directory.join("manager-stopped-1.json"), true).unwrap();
        assert_eq!(stopped.get("binding"), Some(&binding));
        assert_eq!(stopped.get("gate"), handoff.get("gate"));
        assert!(fs::symlink_metadata(fixture.directory.join("entry-1.json")).is_err());
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
        assert_eq!(
            request(&base, "execute", Value::Num("1".into())).get("ok"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            identity(&fs::symlink_metadata(&fixture.directory).unwrap()),
            *domain.to_value().get("authority").unwrap()
        );
        drop(daemon);
        fixture.cleanup().unwrap();
    }
}
