//! Positive production gate startup and entry; never permanent closure.

use super::super::super::{authorize, bind, enrollment, launch, manager, object, read_value};
use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::Store;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::process::{Child, Stdio};
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
    let namespace = super::super::super::text(&material, "namespace").unwrap();
    let generation = super::super::super::number(&material, "generation")
        .unwrap()
        .to_string();
    let allocation = super::super::super::number(&material, "allocation")
        .unwrap()
        .to_string();
    let unit = format!("fsm-containment-{namespace}-{generation}-{allocation}.service");
    let (binding, effect) = claim_binding(&fixture, &domain);
    bind(&fixture.directory, &binding).unwrap();
    let intent_path = fixture.directory.join("launch-1.json");
    // Partial intent from a failed prior submission cannot arm another gate.
    fs::write(&intent_path, b"partial native launch intent").unwrap();
    assert!(
        launch::begin(
            &fixture.directory,
            1,
            [Stdio::null(), Stdio::null(), Stdio::null()]
        )
        .unwrap_err()
        .contains("already submitted or uncertain")
    );
    assert_eq!(
        fs::read(&intent_path).unwrap(),
        b"partial native launch intent"
    );
    assert!(
        fs::read_to_string(fixture.groups[0].0.join("cgroup.events"))
            .unwrap()
            .lines()
            .any(|line| line == "populated 0")
    );
    // Explicit repair of this test-owned injected partial record, not cold
    // production recovery or permission to recycle a failed launch.
    fs::remove_file(&intent_path).unwrap();
    let (child, bound) = launch::begin(
        &fixture.directory,
        1,
        [Stdio::null(), Stdio::null(), Stdio::null()],
    )
    .unwrap();
    assert_eq!(bound, Duration::from_millis(5100));
    let mut gate = Gate {
        fixture: &fixture,
        child,
    };
    let intent = read_value(&intent_path, true).unwrap();
    assert_eq!(intent.get("binding"), Some(&binding));
    assert!(
        launch::begin(
            &fixture.directory,
            1,
            [Stdio::null(), Stdio::null(), Stdio::null()]
        )
        .unwrap_err()
        .contains("already submitted or uncertain")
    );
    assert_eq!(read_value(&intent_path, true).unwrap(), intent);
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
