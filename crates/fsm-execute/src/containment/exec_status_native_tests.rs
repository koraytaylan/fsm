//! Independent installed-gate controls for private exec reporting and cleanup.

use super::super::super::{bind, closure, exec_status, object, read_value, runner};
use super::{Fixture, claim_binding};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::Command;

pub(super) fn run() {
    original_path_cleanup();
    same_identity_peer_cannot_authenticate();
    interrupted_association_retains_claim();
    contended_association_retains_original_deadline();
    for mcp in [false, true] {
        for command in ["/fsm-native-exec-command-does-not-exist", "/etc/passwd"] {
            execution(vec![command.into()], mcp, "spawn", Some("exec/spawn"));
        }
    }
    execution(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf FSMEXEC1 >&2; exit 203".into(),
        ],
        false,
        "nonzero_exit",
        None,
    );
    execution(
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf 'invalid json\n'; printf FSMEXEC1 >&2; exit 203".into(),
        ],
        true,
        "",
        Some("exec/mcp_protocol"),
    );
    descriptor_retirement();
}

fn contended_association_retains_original_deadline() {
    use super::super::super::{authority_lock, launch, stop};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    for hold in [Duration::from_millis(100), Duration::from_millis(2100)] {
        let mut fixture = Fixture::new();
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, _) = claim_binding(&fixture, &domain);
        bind(&fixture.directory, &binding).unwrap();
        let listener = exec_status::Listener::create(
            &fixture.directory,
            1,
            &binding,
            &fsm_execute::config::HandlerKind::Process,
        )
        .unwrap();
        let (mut input, peer) = UnixStream::pair().unwrap();
        listener.send_challenge(&mut input).unwrap();
        let (mut child, _) = launch::begin(
            &fixture.directory,
            1,
            [
                Stdio::from(OwnedFd::from(peer)),
                Stdio::null(),
                Stdio::null(),
            ],
        )
        .unwrap();
        let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
        let gate = handoff.get("gate").unwrap().clone();
        let before = Store::open_read_only(&fixture.store)
            .unwrap()
            .records
            .clone();
        let lock = authority_lock(&fixture.directory).unwrap();
        let (entered, entering) = mpsc::channel();
        let (returned, returning) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            entered.send(()).unwrap();
            returned
                .send(listener.associate(&domain.to_value(), &gate))
                .unwrap();
        });
        entering.recv_timeout(Duration::from_secs(1)).unwrap();
        let observed = returning.recv_timeout(hold);
        assert!(!fixture.directory.join("entry-1.json").exists());
        drop(lock);
        let waited = matches!(&observed, Err(mpsc::RecvTimeoutError::Timeout));
        let result = match observed {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                returning.recv_timeout(Duration::from_secs(2)).unwrap()
            }
            Err(error) => panic!("association worker disconnected: {error}"),
        };
        worker.join().unwrap();
        if hold < Duration::from_secs(2) {
            assert!(
                waited,
                "association must wait for its original authority lock"
            );
            assert!(
                result.is_ok(),
                "association refused after original lock release"
            );
        } else {
            assert_eq!(result.err().unwrap(), "exec status association deadline");
        }
        assert_eq!(
            Store::open_read_only(&fixture.store).unwrap().records,
            before
        );
        assert!(!fixture.directory.join("entry-1.json").exists());
        drop(input);
        let _ = stop::fence(&fixture.directory, 1);
        closure::complete(&fixture.directory, 1).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        fixture.cleanup().unwrap();
    }
}

fn interrupted_association_retains_claim() {
    use super::super::super::{launch, stop};
    use std::io::{Error, ErrorKind, Read};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    for interrupt_accept in [true, false] {
        let mut fixture = Fixture::new();
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, effect) = claim_binding(&fixture, &domain);
        bind(&fixture.directory, &binding).unwrap();
        let listener = exec_status::Listener::create(
            &fixture.directory,
            1,
            &binding,
            &fsm_execute::config::HandlerKind::Process,
        )
        .unwrap();
        let (mut input, peer) = UnixStream::pair().unwrap();
        listener.send_challenge(&mut input).unwrap();
        let (mut child, _) = launch::begin(
            &fixture.directory,
            1,
            [
                Stdio::from(OwnedFd::from(peer)),
                Stdio::null(),
                Stdio::null(),
            ],
        )
        .unwrap();
        let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
        let mut injected = false;
        let mut read_injected = false;
        let mut accept_calls = 0;
        let mut read_calls = 0;
        let refusal = listener
            .associate_with_io(
                &domain.to_value(),
                handoff.get("gate").unwrap(),
                |socket| {
                    accept_calls += 1;
                    if interrupt_accept && !injected {
                        injected = true;
                        std::thread::sleep(Duration::from_millis(2100));
                        return Err(Error::from(ErrorKind::Interrupted));
                    }
                    socket.accept()
                },
                |stream, bytes| {
                    read_calls += 1;
                    if !interrupt_accept && !read_injected {
                        read_injected = true;
                        std::thread::sleep(Duration::from_millis(2100));
                        return Err(Error::from(ErrorKind::Interrupted));
                    }
                    stream.read(bytes)
                },
            )
            .err()
            .expect("interrupted retry bypassed association deadline");
        assert_eq!(refusal, "exec status association deadline");
        if interrupt_accept {
            assert_eq!(accept_calls, 1, "accept retried after the deadline");
            assert_eq!(read_calls, 0);
        } else {
            assert_eq!(read_calls, 1, "hello read retried after the deadline");
        }
        assert!(if interrupt_accept {
            injected
        } else {
            read_injected
        });
        assert!(!fixture.directory.join("entry-1.json").exists());
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
        drop(input);
        let _ = stop::fence(&fixture.directory, 1);
        closure::complete(&fixture.directory, 1).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        fixture.cleanup().unwrap();
    }
}

fn same_identity_peer_cannot_authenticate() {
    use super::super::super::{enrollment, launch, number, observation, stop};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let material = domain.to_value();
    let (binding, _) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let listener = exec_status::Listener::create(
        &fixture.directory,
        1,
        &binding,
        &fsm_execute::config::HandlerKind::Process,
    )
    .unwrap();
    let (mut input, peer) = UnixStream::pair().unwrap();
    listener.send_challenge(&mut input).unwrap();
    let (mut child, _) = launch::begin(
        &fixture.directory,
        1,
        [
            Stdio::from(OwnedFd::from(peer)),
            Stdio::null(),
            Stdio::null(),
        ],
    )
    .unwrap();
    let handoff = read_value(&fixture.directory.join("handoff-1.json"), true).unwrap();
    let gate = handoff.get("gate").unwrap();
    let pid = number(gate, "pid").unwrap();
    let group = number(gate, "group_id").unwrap();
    assert_eq!(
        fs::metadata(enrollment::EXECUTABLE).unwrap().mode() & 0o7777,
        0o711
    );
    assert_eq!(
        fs::symlink_metadata(format!("/proc/{pid}/fd"))
            .unwrap()
            .uid(),
        0
    );
    let native_group = &fixture.groups[0].0;
    // Freeze only the fixture's original gate before opening the status route,
    // so the independent same-UID actor wins the first connection deterministically.
    fs::write(native_group.join("cgroup.freeze"), b"1").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while observation::read(&fixture.directory, 1)
        .unwrap()
        .get("frozen")
        != Some(&Value::Bool(true))
    {
        assert!(Instant::now() < deadline, "original gate did not freeze");
        std::thread::sleep(Duration::from_millis(5));
    }
    let script = "import errno,os,socket,sys,time\nuid=int(sys.argv[1]); pid=int(sys.argv[2]); base=sys.argv[3]\nos.setgroups([]); os.setgid(uid); os.setuid(uid)\nassert os.getuid()==uid and os.getgid()==uid\ntry: os.listdir('/proc/'+str(pid)+'/fd')\nexcept PermissionError: pass\nelse: raise AssertionError('same UID could inspect protected gate descriptors')\ndeadline=time.monotonic()+2\nwhile os.stat(base).st_mode&0o777!=0o710:\n assert time.monotonic()<deadline\n time.sleep(.005)\ns=socket.socket(socket.AF_UNIX); s.settimeout(2); s.connect(base+'/s')\ns.sendall(b'FSMEXEC1'+pid.to_bytes(4,'big')+bytes(32))";
    let fake = Command::new("/usr/bin/python3")
        .args(["-c", script])
        .arg(group.to_string())
        .arg(pid.to_string())
        .arg(fixture.directory.join("exec-1"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let refused = listener
        .associate(&material, gate)
        .err()
        .expect("same-identity forged hello authenticated");
    assert!(refused.contains("inherited challenge"), "{refused}");
    drop(input);
    let output = fake.wait_with_output().unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.directory.join("entry-1.json").exists());
    let run = number(binding.get("claim").unwrap(), "run_id").unwrap();
    for prefix in ["completed", "result"] {
        assert_eq!(
            fs::symlink_metadata(fixture.directory.join(format!("{prefix}-1-{run}.json")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    fs::write(native_group.join("cgroup.freeze"), b"0").unwrap();
    let _ = stop::fence(&fixture.directory, 1);
    closure::complete(&fixture.directory, 1).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "forged hello retained launcher");
        std::thread::sleep(Duration::from_millis(5));
    }
    fixture.cleanup().unwrap();
}

fn table(argv: Vec<String>, mcp: bool) -> Value {
    let mut handler = parse(br#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}"#, &JsonLimits::DEFAULT).unwrap().as_obj().unwrap().clone();
    handler.insert(
        "argv".into(),
        Value::Arr(argv.into_iter().map(Value::Str).collect()),
    );
    if mcp {
        handler.insert("kind".into(), Value::Str("mcp".into()));
        handler.insert("tool".into(), Value::Str("notify".into()));
        handler.insert("arguments".into(), object([]));
    }
    object([
        ("format", Value::Str("fsm.handlers/1".into())),
        ("handlers", Value::Arr(vec![Value::Obj(handler)])),
    ])
}

fn execution(argv: Vec<String>, mcp: bool, failure: &str, error: Option<&str>) {
    let argv0 = argv[0].clone();
    let mut fixture = Fixture::new_for_table(table(argv, mcp));
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let result = runner::execute(&fixture.directory, 1).unwrap_or_else(|error| {
        panic!("private exec-status fixture argv0={argv0:?} mcp={mcp}: {error}")
    });
    let expected_failure = if failure.is_empty() {
        Value::Null
    } else {
        Value::Str(failure.into())
    };
    assert_eq!(result.get("failure_class"), Some(&expected_failure));
    let candidate = result.get("candidate").unwrap();
    if let Some(error) = error {
        assert_eq!(candidate.get("error"), Some(&Value::Str(error.into())));
    }
    if failure == "spawn" {
        assert_eq!(candidate.get("argv0"), Some(&Value::Str(argv0)));
        assert_eq!(candidate.get("status"), Some(&Value::Num("-1".into())));
    } else if !mcp {
        assert_eq!(candidate.get("status"), Some(&Value::Num("203".into())));
    }
    VerifiedClosure::read(Path::new(result.get("receipt").unwrap().as_str().unwrap())).unwrap();
    assert_eq!(runner::recover(&fixture.directory, 1).unwrap(), result);
    assert_eq!(
        fs::symlink_metadata(fixture.directory.join("exec-1"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
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
    fixture.cleanup().unwrap();
}

fn original_path_cleanup() {
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let listener = exec_status::Listener::create(
        &fixture.directory,
        1,
        &binding,
        &fsm_execute::config::HandlerKind::Process,
    )
    .unwrap();
    let base = fixture.directory.join("exec-1");
    let socket = base.join("s");
    assert_eq!(fs::symlink_metadata(&base).unwrap().mode() & 0o777, 0o700);
    assert_eq!(fs::symlink_metadata(&socket).unwrap().mode() & 0o777, 0o600);
    let denied = Command::new("/usr/bin/python3").args(["-c", "import errno,os,socket,sys; os.setgroups([]); os.setgid(65534); os.setuid(65534)\ns=socket.socket(socket.AF_UNIX)\ntry: s.connect(sys.argv[1])\nexcept OSError as error: assert error.errno==errno.EACCES\nelse: raise AssertionError('operator connected to private exec socket')"]).arg(&socket).status().unwrap();
    assert!(denied.success());
    let original = base.join("original");
    fs::rename(&socket, &original).unwrap();
    let replacement = UnixListener::bind(&socket).unwrap();
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    let replacement_inode = fs::symlink_metadata(&socket).unwrap().ino();
    assert!(exec_status::retire(&fixture.directory, 1).is_err());
    assert_eq!(
        fs::symlink_metadata(&socket).unwrap().ino(),
        replacement_inode
    );
    assert!(original.exists());
    drop(replacement);
    fs::remove_file(&socket).unwrap();
    fs::rename(&original, &socket).unwrap();
    let record_path = fixture.directory.join("exec-status-1.json");
    let record = fs::read(&record_path).unwrap();
    fs::set_permissions(&record_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(exec_status::retire(&fixture.directory, 1).is_err());
    assert!(socket.exists());
    fs::set_permissions(&record_path, fs::Permissions::from_mode(0o600)).unwrap();
    let saved = fixture.directory.join("fixture-exec-status.saved");
    fs::rename(&record_path, &saved).unwrap();
    assert!(exec_status::retire(&fixture.directory, 1).is_err());
    assert!(socket.exists());
    std::os::unix::fs::symlink(&saved, &record_path).unwrap();
    assert!(exec_status::retire(&fixture.directory, 1).is_err());
    assert!(socket.exists());
    fs::remove_file(&record_path).unwrap();
    fs::rename(&saved, &record_path).unwrap();

    fs::write(&record_path, b"{").unwrap();
    assert!(closure::complete(&fixture.directory, 1).is_err());
    assert!(!fixture.directory.join("closed-1.json").exists());
    assert!(socket.exists());
    fs::write(&record_path, &record).unwrap();
    let child = base.join("unknown-child");
    fs::create_dir(&child).unwrap();
    assert!(closure::complete(&fixture.directory, 1).is_err());
    assert!(child.is_dir());
    assert!(!fixture.directory.join("closed-1.json").exists());
    fs::remove_dir(child).unwrap();
    drop(listener);
    closure::complete(&fixture.directory, 1).unwrap();
    assert!(!base.exists());
    assert_eq!(fs::read(&record_path).unwrap(), record);
    assert_eq!(
        canon_bytes(&read_value(&record_path, true).unwrap()),
        record
    );
    fixture.cleanup().unwrap();
}

fn descriptor_retirement() {
    let script = "import os,pathlib,socket,stat,sys\nassert stat.S_ISCHR(os.fstat(0).st_mode) and os.read(0,1)==b''\nroute=pathlib.Path('/proc/self/cgroup').read_text().strip().rsplit('/',1)[1].removesuffix('.service').split('-')\nbase=pathlib.Path('/var/lib/fsm-containment')/route[2]/('authority-'+route[3])/('exec-'+route[4])\nassert not base.exists()\nfor entry in pathlib.Path('/proc/self/fd').iterdir():\n if int(entry.name)<=2: continue\n try: target=os.readlink(entry)\n except FileNotFoundError: continue\n assert not target.startswith('socket:'), target\ns=socket.socket(socket.AF_UNIX)\ntry: s.connect(str(base/'s'))\nexcept FileNotFoundError: pass\nelse: raise AssertionError('handler reopened exec status route')\nprint('retired')";
    let mut fixture = Fixture::new_for_table(table(
        vec!["/usr/bin/python3".into(), "-c".into(), script.into()],
        false,
    ));
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let result = runner::execute(&fixture.directory, 1).unwrap();
    assert_eq!(result.get("failure_class"), Some(&Value::Null));
    assert_eq!(
        result.get("candidate").unwrap().get("stdout"),
        Some(&Value::Str("retired\n".into()))
    );
    VerifiedClosure::read(Path::new(result.get("receipt").unwrap().as_str().unwrap())).unwrap();
    fixture.cleanup().unwrap();
}
