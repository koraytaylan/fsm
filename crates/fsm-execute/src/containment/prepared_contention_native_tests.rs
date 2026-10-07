//! Real authority-lock contention preserves the original prepared domain.

use super::*;
use std::sync::mpsc;

pub(super) fn run() {
    let mut fixture = Fixture::new();
    let domain = fixture.prepare();
    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    assert_eq!(
        super::super::super::authority_lock(&fixture.directory).unwrap_err(),
        "authority busy"
    );
    let started = Instant::now();
    assert_eq!(
        super::super::super::closure::discard_prepared(&fixture.directory, &domain).unwrap_err(),
        "prepared cleanup authority deadline"
    );
    assert!(started.elapsed() >= Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_original_retained(&fixture, &domain);
    drop(lock);

    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    let directory = fixture.directory.clone();
    let original = domain.clone();
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (result_sender, result_receiver) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        entered_sender.send(()).unwrap();
        let result = super::super::super::closure::discard_prepared(&directory, &original);
        result_sender.send(result).unwrap();
    });
    entered_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        result_receiver.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    assert_original_retained(&fixture, &domain);
    drop(lock);
    assert_eq!(
        result_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        domain
    );
    worker.join().unwrap();
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

fn assert_original_retained(fixture: &Fixture, domain: &Value) {
    assert_eq!(
        super::super::super::closing::recorded_domain(&fixture.directory, 1).unwrap(),
        *domain
    );
    assert!(fixture.groups[0].0.exists());
    assert_eq!(
        identity(&fs::symlink_metadata(&fixture.groups[0].0).unwrap()),
        fixture.groups[0].1
    );
    for name in ["closing-1.json", "closed-1.json", "binding-1.json"] {
        assert!(!fixture.directory.join(name).exists());
    }
}
