//! Original authority contention cannot publish or burn a native allocation.

use super::*;
use std::sync::mpsc;

pub(super) fn run() {
    let mut fixture = Fixture::new();
    identity_after_contention(&fixture);
    let before = fixture.counter();
    let records = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    let started = Instant::now();
    let result = prepare(&fixture.directory);
    let elapsed = started.elapsed();
    drop(lock);
    assert_eq!(result.unwrap_err(), "authority busy");
    assert!(elapsed >= Duration::from_secs(2));
    assert!(elapsed < Duration::from_secs(3));
    assert_unallocated(&fixture, &before, &records);

    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    let directory = fixture.directory.clone();
    let (entered, entering) = mpsc::channel();
    let (returned, returning) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut entered = Some(entered);
        returned
            .send(super::super::prepare_with_contention_probe(
                &directory,
                || {
                    if let Some(entered) = entered.take() {
                        entered.send(()).unwrap();
                    }
                },
            ))
            .unwrap();
    });
    let contended = entering.recv_timeout(Duration::from_secs(1)).is_ok();
    let early = returning.recv_timeout(Duration::from_millis(100));
    let waited = matches!(&early, Err(mpsc::RecvTimeoutError::Timeout));
    assert_unallocated(&fixture, &before, &records);
    drop(lock);
    let result = match early {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            returning.recv_timeout(Duration::from_secs(2)).unwrap()
        }
        Err(error) => panic!("original allocation worker disconnected: {error}"),
    };
    worker.join().unwrap();
    let domain = result.unwrap();
    let path = cgroup(
        &origin(&fixture.directory).unwrap(),
        number(&domain, "allocation").unwrap(),
    )
    .unwrap();
    fixture
        .groups
        .push((path, domain.get("cgroup").unwrap().clone()));
    assert!(
        contended && waited,
        "prepare must wait before publishing its allocation"
    );
    assert_eq!(number(&domain, "allocation").unwrap(), 1);
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 1);
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    assert!(!fixture.directory.join("binding-1.json").exists());
    super::super::super::closure::discard_prepared(&fixture.directory, &domain).unwrap();
    fixture.cleanup().unwrap();
}

fn identity_after_contention(fixture: &Fixture) {
    let lock = super::super::super::authority_lock(&fixture.directory).unwrap();
    let directory = fixture.directory.clone();
    let (entered, entering) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut entered = Some(entered);
        super::super::prepare_with_contention_probe(&directory, || {
            if let Some(entered) = entered.take() {
                entered.send(()).unwrap();
                resumed.recv_timeout(Duration::from_secs(1)).unwrap();
            }
        })
    });
    let contended = entering.recv_timeout(Duration::from_secs(1)).is_ok();
    if !contended {
        drop(lock);
        let _ = worker.join();
        panic!("prepare did not reach the original contended lock");
    }
    let mut swapped = super::binding_contention_cases::NamespaceSwap::new(&fixture.directory);
    drop(lock);
    resume.send(()).unwrap();
    let result = worker.join().unwrap();
    let unpublished = fs::read_dir(&fixture.directory).unwrap().next().is_none();
    swapped.restore().unwrap();
    assert_eq!(result.unwrap_err(), "authority identity differs");
    assert!(
        unpublished,
        "a replacement authority must receive no allocation state"
    );
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 0);
}

fn assert_unallocated(fixture: &Fixture, before: &Value, records: &[fsm_core::record::Record]) {
    assert_eq!(fixture.counter(), *before);
    for prefix in ["allocation", "prepared", "binding", "launch", "entry"] {
        assert!(!fixture.directory.join(format!("{prefix}-1.json")).exists());
    }
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
}
