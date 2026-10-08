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
    let (_, effect) = claim_binding(&fixture, &domain);
    super::super::broker_cases::disconnect_cases::permit_operator_store(&fixture.store);
    let before = fs::read(fixture.directory.join("prepared-1.json")).unwrap();
    let guard =
        super::super::super::super::owner_lease::acquire(&fixture.directory, 1, 65534).unwrap();
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
