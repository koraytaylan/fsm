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
    refuse_unavailable_socket(&fixture, &session, &effect);
    refuse_reused_cgroup(&fixture, &session, &effect, &group);
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

fn refuse_reused_cgroup(
    fixture: &Fixture,
    session: &orphan_recovery::Session<'_>,
    effect: &str,
    group: &Path,
) {
    let original = fs::symlink_metadata(group).unwrap();
    let substitute = group.with_extension("replacement");
    assert!(!substitute.exists());
    // Cgroup v2 forbids rename: overlay only this fixture's recorded pathname
    // with a distinct empty kernel cgroup, preserving the original underneath.
    fs::create_dir(&substitute).unwrap();
    assert!(
        std::process::Command::new("/usr/bin/mount")
            .arg("--bind")
            .arg(&substitute)
            .arg(group)
            .status()
            .unwrap()
            .success()
    );
    let replacement = fs::symlink_metadata(group).unwrap();
    assert_ne!(
        (replacement.dev(), replacement.ino()),
        (original.dev(), original.ino())
    );
    session.refuse_pre_run_reused_cgroup();
    assert_unresolved(fixture, effect);
    for name in [
        "binding-1.json",
        "launch-1.json",
        "closing-1.json",
        "closed-1.json",
    ] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    let retained = fs::symlink_metadata(group).unwrap();
    assert_eq!(
        (retained.dev(), retained.ino()),
        (replacement.dev(), replacement.ino())
    );
    // Unmount only the fixture overlay, then remove its independently identified
    // empty cgroup; the original kernel allocation remains unchanged underneath.
    assert!(
        std::process::Command::new("/usr/bin/umount")
            .arg(group)
            .status()
            .unwrap()
            .success()
    );
    let substitute_identity = fs::symlink_metadata(&substitute).unwrap();
    assert_eq!(
        (substitute_identity.dev(), substitute_identity.ino()),
        (replacement.dev(), replacement.ino())
    );
    fs::remove_dir(&substitute).unwrap();
    let restored = fs::symlink_metadata(group).unwrap();
    assert_eq!(
        (
            restored.dev(),
            restored.ino(),
            restored.uid(),
            restored.mode()
        ),
        (
            original.dev(),
            original.ino(),
            original.uid(),
            original.mode()
        )
    );
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

fn refuse_unavailable_socket(
    fixture: &Fixture,
    session: &orphan_recovery::Session<'_>,
    effect: &str,
) {
    let route = read_value(&fixture.directory.join("broker/route.json"), true).unwrap();
    let epoch = number(&route, "epoch").unwrap();
    let socket = fixture.directory.join(format!("broker/s-{epoch}"));
    let original = fs::symlink_metadata(&socket).unwrap();
    assert_eq!(original.mode() & 0o7777, 0o600);
    // Remove access only to this fixture's original endpoint; retain the broker
    // and inode so refusal cannot be mistaken for proof that the domain died.
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o0)).unwrap();
    session.refuse_pre_run_socket();
    assert_unresolved(fixture, effect);
    for name in [
        "binding-1.json",
        "launch-1.json",
        "entry-1.json",
        "closing-1.json",
        "closed-1.json",
    ] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    let restored = fs::symlink_metadata(&socket).unwrap();
    assert_eq!(restored.dev(), original.dev());
    assert_eq!(restored.ino(), original.ino());
    assert_eq!(restored.uid(), original.uid());
    assert_eq!(restored.mode(), original.mode());
    refuse_missing_socket(fixture, session, effect, &socket);
}

fn refuse_missing_socket(
    fixture: &Fixture,
    session: &orphan_recovery::Session<'_>,
    effect: &str,
    socket: &Path,
) {
    let original = fs::symlink_metadata(socket).unwrap();
    let preserved = socket.with_extension("preserved");
    assert!(!preserved.exists());
    // Withdraw the recorded native endpoint while preserving its physical
    // socket and broker; endpoint absence cannot establish domain retirement.
    fs::rename(socket, &preserved).unwrap();
    assert_eq!(
        fs::symlink_metadata(socket).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    session.refuse_pre_run_missing_socket();
    assert_unresolved(fixture, effect);
    for name in [
        "binding-1.json",
        "launch-1.json",
        "closing-1.json",
        "closed-1.json",
    ] {
        assert!(fs::symlink_metadata(fixture.directory.join(name)).is_err());
    }
    fs::rename(&preserved, socket).unwrap();
    let restored = fs::symlink_metadata(socket).unwrap();
    assert_eq!(
        (
            restored.dev(),
            restored.ino(),
            restored.uid(),
            restored.mode()
        ),
        (
            original.dev(),
            original.ino(),
            original.uid(),
            original.mode()
        )
    );
}
