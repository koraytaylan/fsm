//! Completed closure waits only before mutation and retains original authority.

use super::*;

pub(super) fn run() {
    let mut fixture = Fixture::new();
    let domain = fixture.prepare();
    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    let started = Instant::now();
    let refused = super::super::super::closure::complete(&fixture.directory, 1);
    drop(lock);
    assert_eq!(refused.unwrap_err(), "closure authority deadline");
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_original(&fixture, &domain);

    let mut lock = Some(super::super::super::authority_lock(&fixture.directory).unwrap());
    let mut swapped = None;
    let result =
        super::super::super::closure::complete_with_contention_probe(&fixture.directory, 1, || {
            assert!(swapped.is_none());
            swapped = Some(binding_contention_cases::NamespaceSwap::new(
                &fixture.directory,
            ));
            drop(lock.take());
        });
    swapped.as_mut().unwrap().restore().unwrap();
    assert_eq!(result.unwrap_err(), "closure authority identity differs");
    assert_original(&fixture, &domain);

    let mut lock = Some(super::super::super::authority_lock(&fixture.directory).unwrap());
    let mut acquisitions = 0;
    super::super::super::closure::complete_with_contention_probe(&fixture.directory, 1, || {
        assert_original(&fixture, &domain);
        acquisitions += 1;
        drop(lock.take());
    })
    .unwrap();
    assert_eq!(
        acquisitions, 1,
        "closure must retry its refused acquisition"
    );
    assert!(!fixture.groups[0].0.exists());
    assert_eq!(
        read_value(&fixture.directory.join("closed-1.json"), true).unwrap(),
        object([
            ("format", Value::Str("fsm.native-domain-closed/1".into())),
            ("domain", domain),
        ])
    );
    fixture.cleanup().unwrap();
}

fn assert_original(fixture: &Fixture, domain: &Value) {
    assert_eq!(
        super::super::super::closing::recorded_domain(&fixture.directory, 1).unwrap(),
        *domain
    );
    assert_eq!(
        identity(&fs::symlink_metadata(&fixture.groups[0].0).unwrap()),
        fixture.groups[0].1
    );
    for name in ["closing-1.json", "closed-1.json", "binding-1.json"] {
        assert!(!fixture.directory.join(name).exists());
    }
}
