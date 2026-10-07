//! Timing/cancellation guards supplement original-run wiring; unit values grant no proof.

use super::*;
use crate::run::native_client::worker::{Budget, Scope, reserve_current};
use std::{
    sync::{atomic::AtomicUsize, mpsc},
    time::Duration,
};

fn receive(worker: &mut ProofWorker<()>) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match worker.poll() {
            Ok(Some(())) => return Ok(()),
            Err(error) => return Err(error),
            Ok(None) => {
                assert!(Instant::now() < until);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[test]
fn native_proof_expired_dispatch_never_calls_the_evidence_reader() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let called = Arc::clone(&calls);
    let mut worker = ProofWorker::start(
        move || {
            called.fetch_add(1, Ordering::AcqRel);
            Ok(())
        },
        Arc::clone(&ticket),
        Instant::now(),
    )
    .unwrap();
    assert_eq!(
        receive(&mut worker).unwrap_err(),
        "native proof deadline; original ownership remains uncertain"
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert!(worker.reap());
    assert_eq!(budget.reserved(), 1);
    drop(worker);
    assert_eq!(budget.reserved(), 1);
    drop(ticket);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_proof_result_after_deadline_is_refused_after_the_reader_returns() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut worker = ProofWorker::start(
        move || {
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        },
        ticket,
        deadline,
    )
    .unwrap();
    arrived.recv_timeout(Duration::from_secs(5)).unwrap();
    let pending = worker.poll().unwrap().is_none();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    release.send(()).unwrap();
    let refused = receive(&mut worker);
    assert!(pending);
    assert_eq!(
        refused.unwrap_err(),
        "native proof deadline; original ownership remains uncertain"
    );
    assert!(worker.reap());
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_proof_cancel_after_reader_entry_suppresses_its_returned_result() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let mut worker = ProofWorker::start(
        move || {
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(())
        },
        ticket,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    arrived.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.cancel();
    let pending = worker.poll().unwrap().is_none();
    release.send(()).unwrap();
    let refused = receive(&mut worker);
    assert!(pending);
    assert_eq!(
        refused.unwrap_err(),
        "native proof cancelled; original ownership remains uncertain"
    );
    assert!(worker.reap());
    assert_eq!(budget.reserved(), 0);
}
