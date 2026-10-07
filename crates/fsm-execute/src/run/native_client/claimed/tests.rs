//! Actual transport retirement precedes held proof reads; literal claims grant no closure.

use super::*;
use crate::run::native_client::{proof_worker::FixtureHook, test_support, worker};
use std::sync::{Arc, mpsc};

enum OriginalPhase {
    Recovery,
    Execution,
}

#[test]
fn native_original_run_deadline_reaches_binding_and_recovery_startup_unchanged() {
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let claim = test_support::original_claim();
    let hash = format!("sha256:{}", "a".repeat(64));
    for recovery in [false, true] {
        let (entered, arrived) = mpsc::channel();
        let _startup =
            crate::run::native_client::startup::FixtureFactory::install(move |prepared| {
                entered.send(prepared.deadline).unwrap();
                Err("fixture original startup refused".into())
            });
        let mut run = if recovery {
            NativeRun::recover(&claim, &hash, Duration::from_secs(10))
        } else {
            NativeRun::start(&claim, &hash, Duration::from_secs(10))
        }
        .unwrap();
        let observed = arrived.recv_timeout(Duration::from_secs(5)).unwrap();
        retire(&mut run);
        assert_eq!(observed, run.deadline);
        assert_eq!(
            run.poll().err().unwrap(),
            "fixture original startup refused"
        );
        drop(run);
        assert_eq!(budget.reserved(), 0);
    }
}

#[test]
fn native_original_run_deadline_reaches_execution_startup_unchanged() {
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let mut run = original_run(OriginalPhase::Execution);
    run.require_writer_entry();
    let (entered, arrived) = mpsc::channel();
    let _startup = crate::run::native_client::startup::FixtureFactory::install(move |prepared| {
        entered.send(prepared.deadline).unwrap();
        Err("fixture execution startup refused".into())
    });
    let claim = run.claim.clone();
    let hash = run.journal_claim.clone();
    run.launch_bound(&claim, &hash).unwrap();
    let observed = arrived.recv_timeout(Duration::from_secs(5)).unwrap();
    retire(&mut run);
    assert_eq!(observed, run.deadline);
    assert_eq!(
        run.poll().err().unwrap(),
        "fixture execution startup refused"
    );
    drop(run);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_bound_standalone_run_selects_worker_mode_before_successor_startup() {
    let mut run = original_run(OriginalPhase::Execution);
    assert!(run.request.ticket.is_none());
    run.require_writer_entry();
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let _startup = crate::run::native_client::startup::FixtureFactory::install(|_| {
        Err("fixture execution startup refused".into())
    });
    let claim = run.claim.clone();
    let hash = run.journal_claim.clone();
    run.launch_bound(&claim, &hash).unwrap();
    assert!(run.request.worker.is_some() && run.request.inline.is_none());
    assert_eq!(budget.reserved(), 1);
    let until = Instant::now() + Duration::from_secs(5);
    while !run.reap().unwrap() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        run.poll().err().unwrap(),
        "fixture execution startup refused"
    );
    drop(run);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_bound_execution_reuses_its_original_slot_when_the_pool_is_full() {
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let mut run = original_run(OriginalPhase::Execution);
    run.require_writer_entry();
    let original = Arc::clone(run.request.ticket.as_ref().unwrap());
    let deadline = run.deadline;
    let mut occupied = Vec::new();
    for _ in 0..127 {
        occupied.push(worker::reserve_current().unwrap().unwrap());
    }
    assert_eq!(budget.reserved(), 128);
    assert!(worker::reserve_current().is_err());
    let _startup = crate::run::native_client::startup::FixtureFactory::install(|_| {
        Err("fixture execution startup refused".into())
    });
    let claim = run.claim.clone();
    let hash = run.journal_claim.clone();
    let launch = run.launch_bound(&claim, &hash);
    if launch.is_ok() {
        let until = Instant::now() + Duration::from_secs(5);
        while !run.reap().unwrap() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(
        launch.is_ok(),
        "bound attempt lost its reserved launch: {launch:?}"
    );
    assert!(Arc::ptr_eq(run.request.ticket.as_ref().unwrap(), &original));
    assert_eq!(run.deadline, deadline);
    assert_eq!(run.claim, claim);
    assert_eq!(run.journal_claim, hash);
    assert_eq!(budget.reserved(), 128);
    assert_eq!(
        run.poll().err().unwrap(),
        "fixture execution startup refused"
    );
    assert_eq!(run.progress().phase, NativeRunPhase::Uncertain);
    assert!(run.progress().helper.not_started);
    drop(run);
    assert_eq!(
        budget.reserved(),
        128,
        "retained original ticket still owns capacity"
    );
    drop(original);
    assert_eq!(budget.reserved(), 127);
    drop(occupied);
    assert_eq!(budget.reserved(), 0);
}

fn original_run(phase: OriginalPhase) -> NativeRun {
    let claim = test_support::original_claim();
    let hash = format!("sha256:{}", "a".repeat(64));
    let _startup = test_support::completed_transport();
    match phase {
        OriginalPhase::Recovery => {
            NativeRun::recover(&claim, &hash, Duration::from_secs(10)).unwrap()
        }
        OriginalPhase::Execution => {
            let mut run = NativeRun::start(&claim, &hash, Duration::from_secs(10)).unwrap();
            let until = Instant::now() + Duration::from_secs(5);
            while run.progress().phase != NativeRunPhase::Bound {
                assert!(run.poll().unwrap().is_none());
                assert!(Instant::now() < until, "original binding response missing");
                std::thread::sleep(Duration::from_millis(1));
            }
            run
        }
    }
}

fn wait_for_proof(
    run: &mut NativeRun,
    arrived: &mpsc::Receiver<std::thread::ThreadId>,
) -> std::thread::ThreadId {
    let until = Instant::now() + Duration::from_secs(5);
    while run.verification.is_none() {
        assert!(run.poll().unwrap().is_none());
        assert!(Instant::now() < until, "original proof worker missing");
        std::thread::sleep(Duration::from_millis(1));
    }
    arrived.recv_timeout(Duration::from_secs(5)).unwrap()
}

fn retire(run: &mut NativeRun) {
    let until = Instant::now() + Duration::from_secs(5);
    while !run.reap().unwrap() {
        assert!(Instant::now() < until, "original proof worker not joined");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_proof_recovery_poll_and_cancel_do_not_wait_for_a_held_original_reader() {
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let mut run = original_run(OriginalPhase::Recovery);
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let _hook = FixtureHook::install(move || {
        entered.send(std::thread::current().id()).unwrap();
        held.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let reader = wait_for_proof(&mut run, &arrived);
    let before = run.progress();
    let pending = run.poll();
    let retired = run.reap().unwrap();
    run.cancel().unwrap();
    let after_cancel = run.progress();
    let reserved = budget.reserved();
    release.send(()).unwrap();
    retire(&mut run);
    assert_ne!(reader, std::thread::current().id());
    assert!(before.helper.stdout_eof && before.helper.stderr_eof && !before.helper.reaped);
    assert!(matches!(pending, Ok(None)) && !retired);
    assert_eq!(after_cancel.phase, NativeRunPhase::Uncertain);
    assert!(!after_cancel.helper.is_retired());
    assert_eq!(reserved, 1, "proof read must reuse the original slot");
    assert_eq!(
        run.poll().err().unwrap(),
        "native run cancelled; claim remains uncertain"
    );
    assert!(run.progress().helper.is_retired());
    assert_eq!(budget.reserved(), 1);
    drop(run);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_proof_execution_refusal_keeps_the_original_generation_and_slot() {
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let mut run = original_run(OriginalPhase::Execution);
    let original = run.claim.clone();
    let hash = run.journal_claim.clone();
    let _execution_startup = test_support::completed_transport();
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let _hook = FixtureHook::install(move || {
        entered.send(std::thread::current().id()).unwrap();
        held.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let reader = wait_for_proof(&mut run, &arrived);
    let before = run.progress();
    let pending = run.poll();
    let retired = run.reap().unwrap();
    let reserved = budget.reserved();
    release.send(()).unwrap();
    retire(&mut run);
    let refusal = run.poll().err().unwrap();
    assert_ne!(reader, std::thread::current().id());
    assert_eq!(before.phase, NativeRunPhase::Executing);
    assert!(before.helper.stdout_eof && before.helper.stderr_eof && !before.helper.reaped);
    assert!(matches!(pending, Ok(None)) && !retired);
    assert_eq!(refusal, "native completion is not an object");
    assert_eq!(run.claim, original);
    assert_eq!(run.journal_claim, hash);
    assert_eq!(run.progress().phase, NativeRunPhase::Uncertain);
    assert_eq!(reserved, 1);
    assert_eq!(budget.reserved(), 1);
    drop(run);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_proof_published_refusal_is_not_delivered_before_the_original_reader_join() {
    let budget = Arc::new(worker::Budget::default());
    let _scope = worker::Scope::enter(Some(&budget));
    let mut run = original_run(OriginalPhase::Recovery);
    let (entered, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let _hook = FixtureHook::after_result(move || {
        entered.send(std::thread::current().id()).unwrap();
        held.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let reader = wait_for_proof(&mut run, &arrived);
    let pending = run.poll();
    let retired = run.reap().unwrap();
    release.send(()).unwrap();
    retire(&mut run);
    let refusal = run.poll().err().unwrap();
    assert_ne!(reader, std::thread::current().id());
    assert!(matches!(pending, Ok(None)) && !retired);
    assert_eq!(refusal, "native completion is not an object");
    assert_eq!(run.progress().phase, NativeRunPhase::Uncertain);
    assert_eq!(budget.reserved(), 1);
}
