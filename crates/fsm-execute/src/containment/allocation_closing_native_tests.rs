//! Allocation waits for original closure without retaining the authority lock.

use super::*;

pub(super) fn run() {
    let mut fixture = Fixture::new();
    let domain = fixture.prepare();
    super::super::super::closing::begin(&fixture.directory, 1).unwrap();
    fs::remove_dir(&fixture.groups[0].0).unwrap();
    let before = fixture.counter();
    let started = Instant::now();
    let refused = prepare(&fixture.directory);
    assert_eq!(refused.unwrap_err(), "authority busy");
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(fixture.counter(), before);
    assert!(!fixture.directory.join("allocation-2.json").exists());
    assert!(!fixture.directory.join("closed-1.json").exists());

    let marker = fixture.directory.join("closing-1.json");
    let original = fs::read(&marker).unwrap();
    fs::remove_file(&marker).unwrap();
    assert!(prepare(&fixture.directory).is_err());
    assert_eq!(fixture.counter(), before);
    assert!(!fixture.directory.join("allocation-2.json").exists());
    fs::write(&marker, b"{}").unwrap();
    assert_eq!(
        prepare(&fixture.directory).unwrap_err(),
        "closing allocation identity differs"
    );
    assert_eq!(fixture.counter(), before);
    assert!(!fixture.directory.join("allocation-2.json").exists());
    fs::write(&marker, original).unwrap();

    let mut waits = 0;
    let next = super::super::prepare_with_contention_probe(&fixture.directory, || {
        waits += 1;
        assert_eq!(fixture.counter(), before);
        assert!(!fixture.directory.join("allocation-2.json").exists());
        // Reacquisition by real closure proves preparation released its lock;
        // a missing cgroup alone has never authorized the new allocation.
        super::super::super::closure::complete(&fixture.directory, 1).unwrap();
    })
    .unwrap();
    assert_eq!(waits, 1);
    assert_eq!(number(&next, "allocation").unwrap(), 2);
    assert_eq!(
        read_value(&fixture.directory.join("closed-1.json"), true).unwrap(),
        object([
            ("format", Value::Str("fsm.native-domain-closed/1".into())),
            ("domain", domain),
        ])
    );
    let path = cgroup(&origin(&fixture.directory).unwrap(), 2).unwrap();
    fixture
        .groups
        .push((path, next.get("cgroup").unwrap().clone()));
    fixture.cleanup().unwrap();
}
