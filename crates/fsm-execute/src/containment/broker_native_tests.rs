//! Actual provisioned binary and independent unprivileged socket clients.

use super::super::super::{broker_endpoint, identity, number, object, read_value, text};
use super::{Fixture, cgroup, claim_binding, origin};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::{Store, VerifiedClosure};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[path = "broker_disconnect_native_tests.rs"]
mod disconnect_cases;

pub(super) fn disconnect() {
    disconnect_cases::run();
}

const CLIENT: &str = r#"import errno,json,os,socket,sys
uid=int(sys.argv[1])
os.setgroups([])
os.setgid(uid)
os.setuid(uid)
assert os.getuid()==uid and os.geteuid()==uid and os.getgroups()==[]
route=json.load(open(sys.argv[2]+'/route.json'))
path=sys.argv[2]+'/s-'+str(route['epoch'])
s=socket.socket(socket.AF_UNIX)
s.settimeout(5)
if sys.argv[3]=='deny':
    try:
        s.connect(path)
    except OSError as error:
        assert error.errno==errno.EACCES
        sys.exit(0)
    raise AssertionError('unauthorized client connected')
from pathlib import Path
base=Path(sys.argv[2])
encoded=sys.argv[3].encode()
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

struct Daemon(Child);

impl Daemon {
    fn start(directory: &Path) -> Self {
        let namespace = directory.parent().unwrap().file_name().unwrap();
        Self(
            Command::new("/usr/bin/python3")
                .args([
                    "-c",
                    "import os,sys;os.umask(0o077);os.execv(sys.argv[1],sys.argv[1:])",
                ])
                .arg("/usr/libexec/fsm-containment-authority")
                .arg("serve")
                .arg(namespace)
                .arg("1")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }

    fn ready(directory: &Path, epoch: u64) -> Self {
        let mut daemon = Self::start(directory);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(
                daemon.0.try_wait().unwrap().is_none(),
                "broker exited before publication"
            );
            if read_value(&directory.join("broker/route.json"), true)
                .is_ok_and(|route| number(&route, "epoch").unwrap() == epoch)
            {
                return daemon;
            }
            assert!(
                Instant::now() < deadline,
                "broker route publication timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn refused(directory: &Path) {
        let mut daemon = Self::start(directory);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = daemon.0.try_wait().unwrap() {
                assert!(!status.success());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "uncertain broker startup was not refused"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

fn invoke_client(base: &Path, uid: u32, request: &str) -> std::process::Output {
    Command::new("/usr/bin/python3")
        .args(["-c", CLIENT])
        .arg(uid.to_string())
        .arg(base)
        .arg(request)
        .output()
        .unwrap()
}

fn client(base: &Path, uid: u32, request: &str) -> Vec<u8> {
    let result = invoke_client(base, uid, request);
    assert!(
        result.status.success(),
        "unprivileged broker client failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    if request == "deny" {
        return result.stdout;
    }
    assert!(result.stdout.len() >= 4 && result.stdout.len() <= 65540);
    let length = u32::from_be_bytes(result.stdout[..4].try_into().unwrap()) as usize;
    assert_eq!(length, result.stdout.len() - 4);
    result.stdout[4..].to_vec()
}

fn request(base: &Path, action: &str, payload: Value) -> Value {
    let request = object([
        ("format", Value::Str("fsm.native-request/1".into())),
        ("action", Value::Str(action.into())),
        ("payload", payload),
    ]);
    let bytes = client(
        base,
        65534,
        std::str::from_utf8(&canon_bytes(&request)).unwrap(),
    );
    parse(&bytes, &JsonLimits::DEFAULT).unwrap()
}

pub(super) fn run() {
    let table = parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#, &JsonLimits::DEFAULT).unwrap();
    let mut fixture = Fixture::new_for_table(table);
    for uid in [0, 61184, 65519, u32::MAX] {
        assert!(broker_endpoint::provision(&fixture.directory, uid).is_err());
        assert!(!fixture.directory.join("broker").exists());
    }
    broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    assert!(broker_endpoint::provision(&fixture.directory, 65534).is_err());
    let base = fixture.directory.join("broker");
    let counter_path = base.join("counter.json");
    let zero = fs::read(&counter_path).unwrap();
    let saved = base.join("fixture-counter.saved");
    fs::rename(&counter_path, &saved).unwrap();
    Daemon::refused(&fixture.directory);
    assert!(!base.join("epoch-1.json").exists());
    fs::rename(&saved, &counter_path).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 1);
    let first_socket = base.join("s-1");
    let first_identity = identity(&fs::symlink_metadata(&first_socket).unwrap());
    assert_eq!(fs::symlink_metadata(&first_socket).unwrap().uid(), 65534);
    assert_eq!(
        fs::symlink_metadata(&first_socket).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(base.join("route.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o444
    );
    let counter = fs::read(&counter_path).unwrap();
    Daemon::refused(&fixture.directory);
    assert_eq!(fs::read(&counter_path).unwrap(), counter);
    assert!(client(&base, 65533, "deny").is_empty());
    assert!(client(&base, 61184, "deny").is_empty());
    let prohibited = object([
        ("format", Value::Str("fsm.native-request/1".into())),
        ("action", Value::Str("authorize".into())),
        ("payload", Value::Null),
    ]);
    let refused = invoke_client(
        &base,
        65534,
        std::str::from_utf8(&canon_bytes(&prohibited)).unwrap(),
    );
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("outside policy"));
    assert_eq!(fs::read(&counter_path).unwrap(), counter);
    let prepared = request(&base, "prepare", Value::Null);
    assert_eq!(prepared.get("ok"), Some(&Value::Bool(true)));
    let domain = NativeDomain::from_value(prepared.get("result").unwrap()).unwrap();
    let group = cgroup(&origin(&fixture.directory).unwrap(), 1).unwrap();
    fixture
        .groups
        .push((group, domain.to_value().get("cgroup").unwrap().clone()));
    let (binding, effect) = claim_binding(&fixture, &domain);
    let execution = disconnect_cases::complete(&fixture.directory, &binding);
    assert_eq!(
        read_value(&fixture.directory.join("binding-1.json"), true).unwrap(),
        binding
    );
    assert_eq!(
        execution.get("ok"),
        Some(&Value::Bool(true)),
        "{execution:?}"
    );
    let original_claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let completion = fsm_execute::run::native_client::NativeCompletion::verify(
        &execution,
        &original_claim,
        text(&binding, "journal_claim").unwrap(),
    )
    .unwrap();
    assert_eq!(
        completion.candidate().get("status"),
        Some(&Value::Num("0".into()))
    );
    assert_eq!(completion.failure_class(), None);
    assert!(
        completion
            .proof()
            .matches_claim(&original_claim, text(&binding, "journal_claim").unwrap())
    );
    let result = execution.get("result").unwrap();
    assert_eq!(result.get("claim"), binding.get("claim"));
    assert_eq!(result.get("journal_claim"), binding.get("journal_claim"));
    assert_eq!(
        result.get("candidate").unwrap().get("status"),
        Some(&Value::Num("0".into()))
    );
    VerifiedClosure::read(Path::new(text(result, "receipt").unwrap())).unwrap();
    assert_eq!(
        request(&base, "execute", Value::Num("1".into())).get("ok"),
        Some(&Value::Bool(false))
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
    drop(daemon);
    let daemon = Daemon::ready(&fixture.directory, 2);
    assert_eq!(
        identity(&fs::symlink_metadata(&first_socket).unwrap()),
        first_identity
    );
    assert_eq!(
        request(&base, "bind", binding).get("ok"),
        Some(&Value::Bool(false))
    );
    drop(daemon);
    let current = fs::read(&counter_path).unwrap();
    fs::write(&counter_path, &zero).unwrap();
    Daemon::refused(&fixture.directory);
    assert_eq!(fs::read(&counter_path).unwrap(), zero);
    fs::write(&counter_path, &current).unwrap();
    let history = base.join("epoch-2.json");
    let saved_history = base.join("fixture-history.saved");
    fs::rename(&history, &saved_history).unwrap();
    Daemon::refused(&fixture.directory);
    assert_eq!(fs::read(&counter_path).unwrap(), current);
    assert!(!base.join("epoch-3.json").exists());
    fs::rename(&saved_history, &history).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 3);
    assert_eq!(
        identity(&fs::symlink_metadata(&first_socket).unwrap()),
        first_identity
    );
    drop(daemon);
    fixture.cleanup().unwrap();
}
