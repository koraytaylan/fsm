//! Public recovery of an original owned allocation that never reached binding.

use super::*;

pub(super) fn run() {
    let mut fixture = Fixture::new_for_operator(super::super::approved_table());
    let session = orphan_recovery::Session::new(&fixture);
    let domain = super::super::super::prepare_owned(&fixture.directory).unwrap();
    let group =
        super::super::cgroup(&super::super::origin(&fixture.directory).unwrap(), 1).unwrap();
    // The session borrows the fixture; retain the exact cgroup after it retires.
    let group_identity = domain.get("cgroup").unwrap().clone();
    let domain = NativeDomain::from_value(&domain).unwrap();
    // Keep fixture ownership independent of the production acquisition guard
    // so neutralizing that guard cannot also remove the arranged live owner.
    let guard = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(fixture.directory.join("owner-1.LOCK"))
        .unwrap();
    guard.try_lock().unwrap();
    let (_, effect) = claim_binding(&fixture, &domain);
    let before = fs::read(fixture.directory.join("prepared-1.json")).unwrap();
    session.refuse_pre_run_owner();
    assert_unresolved(&fixture, &effect);
    for name in [
        "binding-1.json",
        "launch-1.json",
        "entry-1.json",
        "closing-1.json",
        "closed-1.json",
    ] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    assert_eq!(
        fs::read(fixture.directory.join("prepared-1.json")).unwrap(),
        before
    );
    drop(guard);
    let lease = fixture.directory.join("owner-1.LOCK");
    let original_lease = fs::symlink_metadata(&lease).unwrap();
    fs::set_permissions(&lease, fs::Permissions::from_mode(0o640)).unwrap();
    session.refuse_pre_run_identity();
    assert_unresolved(&fixture, &effect);
    for name in ["binding-1.json", "closing-1.json", "closed-1.json"] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    fs::set_permissions(&lease, fs::Permissions::from_mode(0o600)).unwrap();
    let restored_lease = fs::symlink_metadata(&lease).unwrap();
    assert_eq!(restored_lease.dev(), original_lease.dev());
    assert_eq!(restored_lease.ino(), original_lease.ino());
    refuse_changed_boot(&fixture, &session, &effect);
    session.resume();
    assert_eq!(
        fs::read(fixture.directory.join("prepared-1.json")).unwrap(),
        before
    );
    for name in ["launch-1.json", "entry-1.json"] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    assert!(fixture.directory.join("binding-1.json").is_file());
    drop(session);
    fixture.groups.push((group, group_identity));
    fixture.cleanup().unwrap();
}

fn refuse_changed_boot(fixture: &Fixture, session: &orphan_recovery::Session<'_>, effect: &str) {
    let route = fixture.directory.join("broker/route.json");
    let original_bytes = fs::read(&route).unwrap();
    let original_identity = fs::symlink_metadata(&route).unwrap();
    let Value::Obj(mut changed) = read_value(&route, true).unwrap() else {
        unreachable!()
    };
    let Value::Obj(configuration) = changed.get_mut("configuration").unwrap() else {
        unreachable!()
    };
    let wrong_boot = Value::Str("00000000-0000-0000-0000-000000000000".into());
    assert_ne!(configuration.get("boot"), Some(&wrong_boot));
    configuration.insert("boot".into(), wrong_boot);
    fs::write(&route, fsm_core::canon::canon_bytes(&Value::Obj(changed))).unwrap();
    session.refuse_pre_run_boot();
    assert_unresolved(fixture, effect);
    for name in ["binding-1.json", "closing-1.json", "closed-1.json"] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    fs::write(&route, &original_bytes).unwrap();
    let restored_identity = fs::symlink_metadata(&route).unwrap();
    assert_eq!(restored_identity.dev(), original_identity.dev());
    assert_eq!(restored_identity.ino(), original_identity.ino());
    assert_eq!(fs::read(&route).unwrap(), original_bytes);
}
