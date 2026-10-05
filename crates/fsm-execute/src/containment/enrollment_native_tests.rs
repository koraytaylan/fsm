//! Positive production gate entry; launch transport remains administrative.

use super::super::super::{authorize, bind, enrollment, manager, number, object, text};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::Store;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Gate<'a> {
    fixture: &'a Fixture,
    child: Child,
}

impl Drop for Gate<'_> {
    fn drop(&mut self) {
        // Production submission validates the owned inode before touching it;
        // an already absent domain is not manufactured into closure evidence.
        let _ = super::super::super::termination::request(&self.fixture.directory, 1);
        let _ = self.child.kill();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Ok(Some(_)) | Err(_) => break,
            }
        }
    }
}

pub(super) fn run() {
    let mut fixture = Fixture::new();
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let material = domain.to_value();
    let namespace = text(&material, "namespace").unwrap();
    let generation = number(&material, "generation").unwrap().to_string();
    let allocation = number(&material, "allocation").unwrap().to_string();
    let unit = format!("fsm-containment-{namespace}-{generation}-{allocation}.service");
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let mut command = Command::new("/usr/bin/systemd-run");
    command
        .args(["--quiet", "--collect", "--pipe", "--service-type=exec"])
        .arg(format!("--unit={unit}"));
    for property in [
        "DynamicUser=yes",
        "ProtectControlGroups=yes",
        "ProtectHome=yes",
        "ProtectProc=invisible",
        "RestrictNamespaces=yes",
        "NoNewPrivileges=yes",
        "CapabilityBoundingSet=",
        "Delegate=no",
        "ExitType=cgroup",
        "KillMode=control-group",
        "KillSignal=SIGKILL",
        "Restart=no",
        "TimeoutStopSec=2s",
        "RuntimeMaxSec=8s",
    ] {
        command.arg(format!("--property={property}"));
    }
    let mut gate = Gate {
        fixture: &fixture,
        child: command
            .args([
                "/usr/libexec/fsm-containment-authority",
                "gate",
                namespace,
                &generation,
                &allocation,
            ])
            .env_clear()
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if manager::properties(&unit, &["ActiveState", "SubState"]).is_ok_and(|fields| {
            fields["ActiveState"] == "active" && fields["SubState"] == "running"
        }) {
            break;
        }
        assert!(
            gate.child.try_wait().unwrap().is_none(),
            "gate exited before enrollment"
        );
        assert!(Instant::now() < deadline, "gate enrollment timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Exact executable/argv/proc/security validation proves this is still the
    // gate rather than the approved handler, before any grant exists.
    let group = enrollment::group(&material).unwrap();
    assert!(!fixture.directory.join("entry-1.json").exists());
    assert!(gate.child.try_wait().unwrap().is_none());
    let grant = object([
        ("format", Value::Str("fsm.native-entry/1".into())),
        ("claim", binding.get("claim").unwrap().clone()),
        (
            "journal_claim",
            binding.get("journal_claim").unwrap().clone(),
        ),
        ("argv", Value::Arr(vec![Value::Str("/bin/true".into())])),
    ]);
    authorize::publish_enrolled(&fixture.directory, &object([("grant", grant)])).unwrap();
    let metadata = fs::symlink_metadata(fixture.directory.join("entry-1.json")).unwrap();
    assert_eq!(
        (metadata.uid(), metadata.gid(), metadata.mode() & 0o777),
        (0, group, 0o440)
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = gate.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "production gate did not execute approved true handler"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "approved gate handler exit timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!fixture.directory.join("closed-1.json").exists());
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
    );
    drop(store);
    drop(gate);
    fixture.cleanup().unwrap();
}
