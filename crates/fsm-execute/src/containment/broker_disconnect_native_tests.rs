//! Independent client death closes process and MCP trees before their timeout.

use super::super::runner_cases::{Barriers, SERVER};
use super::{
    Daemon, Fixture, broker_endpoint, canon_bytes, cgroup, claim_binding, identity, number, object,
    origin, read_value, request,
};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::super::handoff_cases::Handoff;

// Stable debug test executables can exceed 64 MiB; copying stays bounded.
const MAX_SUPERVISOR_BINARY: u64 = 128 * 1024 * 1024;

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

const SUPERVISOR: &str = r#"import os,sys
os.setgroups([])
os.setgid(65534)
os.setuid(65534)
assert os.getuid()==65534 and os.geteuid()==65534 and os.getgroups()==[]
from pathlib import Path
authority=Path(sys.argv[1]).parent
os.environ['FSM_NATIVE_TEST_NAMESPACE']=authority.parent.name
os.environ.pop('FSM_NATIVE_TEST_BINDING',None)
os.environ.pop('FSM_NATIVE_TEST_REFUSE',None)
os.environ.pop('FSM_NATIVE_TEST_CANCEL',None)
os.environ.pop('FSM_NATIVE_TEST_STORE',None)
if (authority.parent/'operator-store').is_dir():
    os.environ['FSM_NATIVE_TEST_STORE']=str(authority.parent/'operator-store')
if len(sys.argv)>=3:
    os.environ['FSM_NATIVE_TEST_BINDING']=sys.argv[2]
if len(sys.argv)==4:
    assert sys.argv[3] in ('refuse','cancel')
    os.environ['FSM_NATIVE_TEST_REFUSE' if sys.argv[3]=='refuse' else 'FSM_NATIVE_TEST_CANCEL']='1'
os.execv(str(authority/'supervisor-test'),['supervisor-test','--exact','authority::allocator::native_tests::supervisor_probe::owned_request','--ignored','--nocapture','--color','never'])
"#;

pub(super) fn prepare(directory: &Path) -> Value {
    install_supervisor(directory);
    let script = SUPERVISOR.replace("::owned_request", "::prepare_domain");
    let output = Command::new("/usr/bin/python3")
        .args(["-c", &script])
        .arg(directory.join("broker"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "typed preparation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let domains: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("FSM_NATIVE_TEST_DOMAIN="))
        .collect();
    assert_eq!(domains.len(), 1);
    parse(domains[0].as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

pub(super) fn discard_prepared(directory: &Path, domain: &Value) {
    let script = SUPERVISOR.replace("::owned_request", "::discard_prepared_domain");
    let output = Command::new("/usr/bin/python3")
        .args(["-c", &script])
        .arg(directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(domain)).unwrap())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "owned prepared cleanup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(super) fn complete(
    directory: &Path,
    binding: &Value,
    timeout: bool,
    competitor: &NativeDomain,
) -> Value {
    let cancelled = Command::new("/usr/bin/python3")
        .args(["-c", SUPERVISOR])
        .arg(directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(binding)).unwrap())
        .arg("cancel")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(cancelled.stdout.len() <= 8192 && cancelled.stderr.len() <= 8192);
    assert!(
        cancelled.status.success(),
        "cancel supervisor failed: {}",
        String::from_utf8_lossy(&cancelled.stderr)
    );
    assert_eq!(
        std::str::from_utf8(&cancelled.stdout)
            .unwrap()
            .lines()
            .filter(|line| *line == "FSM_NATIVE_TEST_CANCELLED")
            .count(),
        1
    );
    for name in [
        "binding-1.json",
        "launch-1.json",
        "entry-1.json",
        "handoff-1.json",
    ] {
        assert_eq!(
            fs::symlink_metadata(directory.join(name))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound,
            "pre-poll cancellation left {name} or inspection failed"
        );
    }
    let mut wrong = binding.as_obj().unwrap().clone();
    let hash = wrong.get("journal_claim").unwrap().as_str().unwrap();
    let mut changed = hash.as_bytes().to_vec();
    changed[7] = if changed[7] == b'0' { b'1' } else { b'0' };
    wrong.insert(
        "journal_claim".into(),
        Value::Str(String::from_utf8(changed).unwrap()),
    );
    let refused = Command::new("/usr/bin/python3")
        .args(["-c", SUPERVISOR])
        .arg(directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(&Value::Obj(wrong))).unwrap())
        .arg("refuse")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(refused.stdout.len() <= 8192 && refused.stderr.len() <= 8192);
    assert!(
        refused.status.success(),
        "refusal supervisor failed: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert_eq!(
        std::str::from_utf8(&refused.stdout)
            .unwrap()
            .lines()
            .filter(|line| *line == "FSM_NATIVE_TEST_REFUSED")
            .count(),
        1
    );
    for name in [
        "binding-1.json",
        "launch-1.json",
        "entry-1.json",
        "handoff-1.json",
    ] {
        assert_eq!(
            fs::symlink_metadata(directory.join(name))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound,
            "refused binding left {name} or inspection failed"
        );
    }
    let output = Command::new("/usr/bin/python3")
        .env("FSM_NATIVE_TEST_TIMEOUT", if timeout { "1" } else { "0" })
        .env(
            "FSM_NATIVE_TEST_COMPETING_DOMAIN",
            std::str::from_utf8(&canon_bytes(&competitor.to_value())).unwrap(),
        )
        .args(["-c", SUPERVISOR])
        .arg(directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(binding)).unwrap())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 70 * 1024 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "public supervisor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let responses: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("FSM_NATIVE_TEST_RESPONSE="))
        .collect();
    assert_eq!(responses.len(), 1);
    fsm_core::json::parse(
        responses[0].as_bytes(),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn install_supervisor(directory: &Path) {
    let source = fs::File::open(std::env::current_exe().unwrap()).unwrap();
    let length = source.metadata().unwrap().len();
    assert!(
        length > 0 && length <= MAX_SUPERVISOR_BINARY,
        "native supervisor executable size {length} exceeds fixture bound {MAX_SUPERVISOR_BINARY} or is empty"
    );
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("supervisor-test"))
        .unwrap();
    assert_eq!(
        std::io::copy(&mut source.take(length + 1), &mut destination).unwrap(),
        length
    );
    destination
        .set_permissions(fs::Permissions::from_mode(0o755))
        .unwrap();
    destination.sync_all().unwrap();
    fs::File::open(directory).unwrap().sync_all().unwrap();
}

fn supervised_helper(pid: u32) -> u32 {
    let expected = identity(&fs::metadata("/usr/libexec/fsm-containment-authority").unwrap());
    let mut found = Vec::new();
    for task in fs::read_dir(format!("/proc/{pid}/task")).unwrap() {
        let children = fs::read_to_string(task.unwrap().path().join("children")).unwrap();
        assert!(children.len() <= 4096);
        for child in children.split_whitespace() {
            let child: u32 = child.parse().unwrap();
            if fs::metadata(format!("/proc/{child}/exe"))
                .is_ok_and(|metadata| identity(&metadata) == expected)
            {
                assert_eq!(fs::metadata(format!("/proc/{child}")).unwrap().uid(), 65534);
                found.push(child);
            }
        }
    }
    assert_eq!(
        found.len(),
        1,
        "supervisor must own exactly one installed helper"
    );
    found[0]
}

fn process_dead(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/status")) {
        Ok(status) => status
            .lines()
            .find_map(|line| line.strip_prefix("State:"))
            .is_some_and(|state| state.trim_start().starts_with('Z')),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

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
    for (mode, watch, supervisor) in [
        ("cancel-process", false, false),
        ("cancel-mcp", false, false),
        ("cancel-process", true, false),
        ("cancel-mcp", true, false),
        ("cancel-process", false, true),
        ("cancel-mcp", false, true),
    ] {
        let barriers = Barriers::new();
        let mut fixture = Fixture::new_for_table(table(&barriers.path, mode));
        if supervisor {
            install_supervisor(&fixture.directory);
        }
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
            .args([
                "-c",
                if supervisor {
                    SUPERVISOR
                } else if watch {
                    WATCH_CLIENT
                } else {
                    CLIENT
                },
            ])
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
        } else if !supervisor {
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
            identity(
                &fs::metadata(if supervisor {
                    fixture.directory.join("supervisor-test")
                } else {
                    Path::new("/usr/libexec/fsm-containment-authority").to_path_buf()
                })
                .unwrap()
            )
        );
        let helper = supervisor.then(|| supervised_helper(client.0.id()));
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
        if let Some(helper) = helper {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !process_dead(helper) {
                assert!(
                    Instant::now() < deadline,
                    "helper executes after supervisor death and closure"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
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

pub(super) fn discovery_faults(directory: &Path) {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let counter = fs::read(directory.join("counter.json")).unwrap();
    let public = directory.join("store-identity.json");
    let saved = directory.join("fixture-discovery-identity.saved");
    let refuse = |expected: &str| {
        let script = SUPERVISOR.replace("::owned_request", "::refuse_discovery");
        let output = Command::new("/usr/bin/python3")
            .args(["-c", &script])
            .arg(directory.join("broker"))
            .env("FSM_NATIVE_TEST_DISCOVERY_ERROR", expected)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
        assert!(
            output.status.success(),
            "discovery refusal failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(directory.join("counter.json")).unwrap(), counter);
    };
    fs::set_permissions(&public, fs::Permissions::from_mode(0o644)).unwrap();
    refuse("native discovery document is not immutable root publication");
    fs::set_permissions(&public, fs::Permissions::from_mode(0o444)).unwrap();
    let original = fs::read(&public).unwrap();
    // Exact read budget reaches JSON validation; plus-one must fail the byte guard.
    fs::write(&public, vec![b' '; 4096]).unwrap();
    refuse("native discovery document JSON invalid");
    fs::write(&public, vec![b' '; 4097]).unwrap();
    refuse("native discovery document exceeds bound");
    fs::write(&public, b"{").unwrap();
    refuse("native discovery document JSON invalid");
    fs::write(&public, &original).unwrap();
    let mut wrong_store = parse(&original, &JsonLimits::DEFAULT).unwrap();
    let Value::Obj(registration) = &mut wrong_store else {
        unreachable!()
    };
    let Value::Obj(physical) = registration.get_mut("identity").unwrap() else {
        unreachable!()
    };
    // A real directory inode is nonzero; copied publication bytes cannot identify it.
    physical.insert("inode".into(), Value::Num("0".into()));
    fs::write(&public, canon_bytes(&wrong_store)).unwrap();
    refuse("native discovery store registration missing");
    // Reach the shared directory-entry bound through real production discovery.
    // A deliberately absent registration distinguishes exact-limit traversal
    // from plus-one refusal without allocating another native domain.
    let namespace = directory.parent().unwrap();
    let base = namespace.parent().unwrap();
    let inventory = fs::read_dir(base)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            1 + fs::read_dir(entry.path()).unwrap().count()
        })
        .sum::<usize>();
    assert!(inventory < 4096);
    let mut fillers = Vec::new();
    for index in 0..=4096 - inventory {
        let path = namespace.join(format!("fixture-discovery-inventory-{index}"));
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        fillers.push(path);
        if fillers.len() == 4096 - inventory {
            refuse("native discovery store registration missing");
        }
    }
    refuse("native discovery inventory exceeds bound");
    for path in fillers {
        fs::remove_file(path).unwrap();
    }
    fs::write(&public, &original).unwrap();
    // Keeping the publication bytes and pathname cannot authorize a different
    // physical store; keep the original inode alive so it cannot be reused.
    let store = directory.parent().unwrap().join("operator-store");
    let saved_store = directory.parent().unwrap().join("fixture-store.saved");
    let original_store = fs::metadata(&store).unwrap();
    fs::rename(&store, &saved_store).unwrap();
    fs::create_dir(&store).unwrap();
    fs::set_permissions(&store, fs::Permissions::from_mode(0o755)).unwrap();
    let replacement_store = fs::metadata(&store).unwrap();
    assert_ne!(
        (original_store.dev(), original_store.ino()),
        (replacement_store.dev(), replacement_store.ino())
    );
    refuse("native discovery store registration missing");
    assert_eq!(fs::read(&public).unwrap(), original);
    fs::remove_dir(&store).unwrap();
    fs::rename(&saved_store, &store).unwrap();
    let restored_store = fs::metadata(&store).unwrap();
    assert_eq!(
        (original_store.dev(), original_store.ino()),
        (restored_store.dev(), restored_store.ino())
    );
    let duplicate = directory.parent().unwrap().join("authority-2");
    fs::DirBuilder::new().create(&duplicate).unwrap();
    fs::set_permissions(&duplicate, fs::Permissions::from_mode(0o755)).unwrap();
    let duplicate_identity = duplicate.join("store-identity.json");
    fs::write(&duplicate_identity, &original).unwrap();
    fs::set_permissions(&duplicate_identity, fs::Permissions::from_mode(0o444)).unwrap();
    refuse("native discovery store registration is ambiguous");
    fs::remove_file(&duplicate_identity).unwrap();
    fs::remove_dir(&duplicate).unwrap();
    fs::rename(&public, &saved).unwrap();
    symlink(&saved, &public).unwrap();
    refuse("native discovery document is not immutable root publication");
    fs::remove_file(&public).unwrap();
    fs::rename(&saved, &public).unwrap();
    let route = directory.join("broker/route.json");
    fs::set_permissions(&route, fs::Permissions::from_mode(0o644)).unwrap();
    refuse("native discovery document is not immutable root publication");
    fs::set_permissions(&route, fs::Permissions::from_mode(0o444)).unwrap();
    let original_route = fs::read(&route).unwrap();
    for (field, replacement) in [
        ("operator", Value::Num("1".into())),
        (
            "boot",
            Value::Str("00000000-0000-0000-0000-000000000000".into()),
        ),
    ] {
        let mut wrong_route = parse(&original_route, &JsonLimits::DEFAULT).unwrap();
        let Value::Obj(fields) = &mut wrong_route else {
            unreachable!()
        };
        let Value::Obj(configuration) = fields.get_mut("configuration").unwrap() else {
            unreachable!()
        };
        configuration.insert(field.into(), replacement);
        fs::write(&route, canon_bytes(&wrong_route)).unwrap();
        refuse("native discovery operator, boot or authority differs");
        fs::write(&route, &original_route).unwrap();
    }
    let mut wrong_socket = parse(&original_route, &JsonLimits::DEFAULT).unwrap();
    let Value::Obj(fields) = &mut wrong_socket else {
        unreachable!()
    };
    let Value::Obj(socket) = fields.get_mut("socket").unwrap() else {
        unreachable!()
    };
    socket.insert("inode".into(), Value::Num("0".into()));
    fs::write(&route, canon_bytes(&wrong_socket)).unwrap();
    refuse("native discovery socket identity or access differs");
    fs::write(&route, &original_route).unwrap();
}

pub(super) fn permit_operator_store(path: &Path) {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert_eq!(metadata.uid(), 0);
    assert!(metadata.is_dir() || metadata.is_file());
    if metadata.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            permit_operator_store(&entry.unwrap().path());
        }
    }
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if metadata.is_dir() { 0o700 } else { 0o600 }),
    )
    .unwrap();
    std::os::unix::fs::chown(path, Some(65534), Some(65534)).unwrap();
}

pub(super) fn shared_recovery(directory: &Path, binding: &Value, competitor: &NativeDomain) {
    let catalogue = directory.join("catalogue.json");
    let saved = directory.join("shared-catalogue.saved");
    fs::rename(&catalogue, &saved).unwrap();
    let script = SUPERVISOR.replace("::owned_request", "::shared_tick_recovery");
    let output = Command::new("/usr/bin/python3")
        .env("TMPDIR", directory.parent().unwrap().join("operator-store"))
        .env(
            "FSM_NATIVE_TEST_COMPETING_DOMAIN",
            std::str::from_utf8(&canon_bytes(&competitor.to_value())).unwrap(),
        )
        .args(["-c", &script])
        .arg(directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(binding)).unwrap())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "shared recovery: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| *line == "FSM_NATIVE_SHARED_RECOVERY")
            .count(),
        1
    );
    fs::rename(saved, catalogue).unwrap();
}

pub(super) fn fresh_handoff(directory: &Path, binding: &Value, competitor: &NativeDomain) {
    handoff_control(directory, binding, competitor, Handoff::Warm);
}

pub(super) fn cold_handoff(directory: &Path, binding: &Value, competitor: &NativeDomain) {
    handoff_control(directory, binding, competitor, Handoff::Cold);
}

pub(super) fn conflicting_handoff(directory: &Path, binding: &Value, competitor: &NativeDomain) {
    handoff_control(directory, binding, competitor, Handoff::Conflicting);
}

pub(super) fn rejected_handoff(directory: &Path, binding: &Value, competitor: &NativeDomain) {
    handoff_control(directory, binding, competitor, Handoff::Rejected);
}

fn handoff_control(directory: &Path, binding: &Value, competitor: &NativeDomain, case: Handoff) {
    let test = if case == Handoff::Rejected {
        "::fresh_handoff::shared_tick_rejected_handoff"
    } else if case == Handoff::Conflicting {
        "::fresh_handoff::shared_tick_conflicting_handoff"
    } else if case.cold() {
        "::fresh_handoff::shared_tick_cold_handoff"
    } else {
        "::fresh_handoff::shared_tick_fresh"
    };
    let script = SUPERVISOR.replace("::owned_request", test);
    let output = Command::new("/usr/bin/python3")
        .env("TMPDIR", directory.parent().unwrap().join("operator-store"))
        .env(
            "FSM_NATIVE_TEST_COMPETING_DOMAIN",
            std::str::from_utf8(&canon_bytes(&competitor.to_value())).unwrap(),
        )
        .args(["-c", &script])
        .arg(directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(binding)).unwrap())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "fresh Runner handoff: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| *line
                == if case == Handoff::Rejected {
                    "FSM_NATIVE_REJECTED_HANDOFF"
                } else if case == Handoff::Conflicting {
                    "FSM_NATIVE_CONFLICTING_HANDOFF"
                } else if case.cold() {
                    "FSM_NATIVE_COLD_HANDOFF"
                } else {
                    "FSM_NATIVE_FRESH_HANDOFF"
                })
            .count(),
        1
    );
}

pub(super) fn fresh_admission(directory: &Path, table: &Value) {
    admission_control(directory, table, "execute");
}

pub(super) fn cancel_admission(directory: &Path, table: &Value) {
    admission_control(directory, table, "cancel");
}

pub(super) fn competing_admission(directory: &Path, table: &Value, domain: &Value) {
    // Pass only an actual Root-prepared domain, never invented authority bytes.
    admission_control_inner(directory, table, "compete", Some(domain));
}

fn admission_control(directory: &Path, table: &Value, mode: &str) {
    admission_control_inner(directory, table, mode, None);
}

fn admission_control_inner(directory: &Path, table: &Value, mode: &str, domain: Option<&Value>) {
    install_supervisor(directory);
    let script = SUPERVISOR.replace(
        "::owned_request",
        "::fresh_admission::shared_tick_admission",
    );
    let mut command = Command::new("/usr/bin/python3");
    if mode == "cancel" {
        command.env("FSM_NATIVE_TEST_CANCEL_PRECLAIM", "1");
    } else {
        command.env_remove("FSM_NATIVE_TEST_CANCEL_PRECLAIM");
    }
    if let Some(domain) = domain {
        command.env(
            "FSM_NATIVE_TEST_COMPETING_DOMAIN",
            std::str::from_utf8(&canon_bytes(domain)).unwrap(),
        );
    } else {
        command.env_remove("FSM_NATIVE_TEST_COMPETING_DOMAIN");
    }
    let output = command
        .env("TMPDIR", directory.parent().unwrap().join("operator-store"))
        .env(
            "FSM_NATIVE_TEST_HANDLER_TABLE",
            std::str::from_utf8(&canon_bytes(table)).unwrap(),
        )
        .env("FSM_NATIVE_TEST_AUTHORITY", directory)
        .args(["-c", &script])
        .arg(directory.join("broker"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "fresh native admission: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| *line
                == if mode == "cancel" {
                    "FSM_NATIVE_PRECLAIM_CANCELLATION"
                } else if mode == "compete" {
                    "FSM_NATIVE_PRECLAIM_COMPETITION"
                } else {
                    "FSM_NATIVE_FRESH_ADMISSION"
                })
            .count(),
        1
    );
}
