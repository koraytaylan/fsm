//! Production startup callers refuse a live physical domain replacement.

use super::super::super::super::{authorize, bind, exec_status, identity, object};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::Store;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn binding_refuses_live_replacement_cgroup_identity() {
    refuse_replacement(Caller::Binding);
}

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn exec_status_refuses_live_replacement_cgroup_identity() {
    refuse_replacement(Caller::ExecStatus);
}

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn authorization_refuses_live_replacement_cgroup_identity() {
    refuse_replacement(Caller::Authorization);
}

enum Caller {
    Binding,
    ExecStatus,
    Authorization,
}

fn refuse_replacement(caller: Caller) {
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    if matches!(caller, Caller::Authorization) {
        bind(&fixture.directory, &binding).unwrap();
    }
    let binding_path = fixture.directory.join("binding-1.json");
    let original_binding = fs::read(&binding_path).ok();
    let records = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    let original = &fixture.groups[0].0;
    let original_identity = identity(&fs::symlink_metadata(original).unwrap());
    let replacement = original.with_extension("binding-identity-replacement");
    fs::create_dir(&replacement).unwrap();
    let replacement_identity = identity(&fs::symlink_metadata(&replacement).unwrap());
    assert_ne!(original_identity, replacement_identity);
    // Cgroup v2 cannot rename the original: a fixture-owned bind mount keeps
    // it intact beneath a physically distinct, independently observed group.
    mounted(
        "/usr/bin/mount",
        &[Path::new("--bind"), &replacement, original],
    );
    assert_eq!(
        identity(&fs::symlink_metadata(original).unwrap()),
        replacement_identity
    );
    let mut sentinel = Command::new("/usr/bin/sleep")
        .arg("300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    fs::write(replacement.join("cgroup.procs"), sentinel.id().to_string()).unwrap();
    let membership = format!(
        "0::/system.slice/{}\n",
        replacement.file_name().unwrap().to_str().unwrap()
    );
    let process_group = format!("/proc/{}/cgroup", sentinel.id());
    assert_eq!(fs::read_to_string(&process_group).unwrap(), membership);
    let refusal = match caller {
        Caller::Binding => bind(&fixture.directory, &binding).unwrap_err(),
        Caller::Authorization => {
            authorize::publish(&fixture.directory, &grant_request(&binding)).unwrap_err()
        }
        Caller::ExecStatus => match exec_status::Listener::create(
            &fixture.directory,
            1,
            &binding,
            &fsm_execute::config::HandlerKind::Process,
        ) {
            Err(error) => error,
            Ok(_) => panic!("exec status accepted live replacement cgroup identity"),
        },
    };
    assert_eq!(refusal, "native cgroup identity differs");
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    assert!(sentinel.try_wait().unwrap().is_none());
    assert_eq!(fs::read_to_string(&process_group).unwrap(), membership);
    assert_eq!(
        identity(&fs::symlink_metadata(original).unwrap()),
        replacement_identity
    );
    for name in [
        "launch-1.json",
        "entry-1.json",
        "closing-1.json",
        "closed-1.json",
        "exec-1",
        "exec-status-1.json",
    ] {
        assert!(!fixture.directory.join(name).exists());
    }
    assert_eq!(fs::read(&binding_path).ok(), original_binding);
    // Only the child and overlay created by this fixture are retired after
    // proving that production binding neither admitted nor targeted them.
    sentinel.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while sentinel.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "binding sentinel retirement deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    mounted("/usr/bin/umount", &[original]);
    assert_eq!(
        identity(&fs::symlink_metadata(&replacement).unwrap()),
        replacement_identity
    );
    fs::remove_dir(replacement).unwrap();
    assert_eq!(
        identity(&fs::symlink_metadata(original).unwrap()),
        original_identity
    );
    // Restoring the same original resource permits the same production caller;
    // no missing domain or fixture cleanup is promoted to closure.
    match caller {
        Caller::Binding => bind(&fixture.directory, &binding).unwrap(),
        Caller::Authorization => {
            authorize::publish(&fixture.directory, &grant_request(&binding)).unwrap();
            assert!(fixture.directory.join("entry-1.json").exists());
            assert_eq!(fs::read(&binding_path).ok(), original_binding);
        }
        Caller::ExecStatus => {
            let listener = exec_status::Listener::create(
                &fixture.directory,
                1,
                &binding,
                &fsm_execute::config::HandlerKind::Process,
            )
            .unwrap();
            assert!(fixture.directory.join("exec-1/s").exists());
            assert!(fixture.directory.join("exec-status-1.json").exists());
            drop(listener);
        }
    }
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    fixture.cleanup().unwrap();
}

fn mounted(executable: &str, arguments: &[&Path]) {
    let mut child = Command::new(executable).args(arguments).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "fixture mount command failed");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("fixture mount command deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn grant_request(binding: &Value) -> Value {
    let grant = object([
        ("format", Value::Str("fsm.native-entry/1".into())),
        ("claim", binding.get("claim").unwrap().clone()),
        (
            "journal_claim",
            binding.get("journal_claim").unwrap().clone(),
        ),
        ("argv", Value::Arr(vec![Value::Str("/bin/true".into())])),
    ]);
    object([("grant", grant), ("group_id", Value::Num("1".into()))])
}
