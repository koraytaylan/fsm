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
    let result = runner::execute(&fixture.directory, 1).unwrap();
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
    let listener = exec_status::Listener::create(&fixture.directory, 1, &binding).unwrap();
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
    let script = "import os,pathlib,socket,sys\nroute=pathlib.Path('/proc/self/cgroup').read_text().strip().rsplit('/',1)[1].removesuffix('.service').split('-')\nbase=pathlib.Path('/var/lib/fsm-containment')/route[2]/('authority-'+route[3])/('exec-'+route[4])\nassert not base.exists()\nfor entry in pathlib.Path('/proc/self/fd').iterdir():\n if int(entry.name)<=2: continue\n try: target=os.readlink(entry)\n except FileNotFoundError: continue\n assert not target.startswith('socket:'), target\ns=socket.socket(socket.AF_UNIX)\ntry: s.connect(str(base/'s'))\nexcept FileNotFoundError: pass\nelse: raise AssertionError('handler reopened exec status route')\nprint('retired')";
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
