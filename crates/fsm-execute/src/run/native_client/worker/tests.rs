//! Real held child/stream observations are not native domain closure evidence.

use super::*;
use crate::run::native_client::{NativeRequest, test_support::*};
use fsm_core::json::{JsonLimits, parse};
use std::{collections::BTreeMap, io::Write, sync::mpsc, time::Instant};

mod startup;

#[test]
fn native_successor_refuses_startup_before_original_helper_retirement() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let (inline, _gate, output) = held_transport();
    drop(output);
    let mut request = NativeRequest {
        inline: Some(inline),
        worker: None,
        ticket: None,
    };
    assert!(request.poll().unwrap().is_none());
    let _startup = crate::run::native_client::startup::FixtureFactory::install(|_| {
        Err("fixture successor startup refused".into())
    });
    let message = parse(
        br#"{"format":"fsm.native-request/1","action":"prepare","payload":null}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let refused = request.successor(
        "00000000000000000000000000000000",
        1,
        &message,
        Instant::now() + Duration::from_secs(1),
    );
    request.cancel().unwrap();
    assert!(receive(&mut request).is_err());
    assert_eq!(
        refused.err().unwrap(),
        "native original transport has not retired before successor startup"
    );
    assert_eq!(budget.reserved(), 1);
    drop(request);
    assert_eq!(budget.reserved(), 0);
}

#[test]
fn native_worker_polling_and_cancellation_return_before_held_child_release() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let (inline, _gate, response) = held_transport();
    drop(response);
    let mut request = NativeRequest {
        inline: Some(inline),
        worker: None,
        ticket: None,
    };
    assert!(request.poll().unwrap().is_none());
    assert!(request.worker.is_some());
    assert!(!request.reap().unwrap());
    assert!(!request.progress().reaped);
    request.cancel().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !request.reap().unwrap() {
        assert!(Instant::now() < until, "original helper not retired");
        std::thread::sleep(Duration::from_millis(1));
    }
    let progress = request.progress();
    assert!(progress.reaped && progress.stdout_eof && progress.stderr_eof);
    assert!(receive(&mut request).is_err());
    assert_eq!(budget.0.load(Ordering::Acquire), 1);
    drop(request);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_actual_reap_and_eofs_do_not_retire_an_unjoined_worker() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let (inline, mut gate, mut output) = held_transport();
    frame(&mut output, &response());
    drop(output);
    let (arrived, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let worker = Worker::start_after_retirement(inline, Arc::clone(&ticket), move || {
        arrived.send(()).unwrap();
        held.recv().unwrap();
    })
    .unwrap_or_else(|_| panic!("worker startup failed"));
    let mut request = NativeRequest {
        inline: None,
        worker: Some(worker),
        ticket: Some(ticket),
    };
    gate.write_all(b"release\n").unwrap();
    let reached = observed.recv_timeout(Duration::from_secs(5)).is_ok();
    let before = request.progress();
    let retired = request.reap().unwrap();
    let early = request.poll().unwrap();
    // Release the actual worker before assertions, including sensitivity failures.
    release.send(()).unwrap();
    let delivered = receive(&mut request).unwrap();
    assert!(reached);
    assert!(before.stdout_eof && before.stderr_eof && !before.reaped);
    assert!(!retired && early.is_none());
    assert_eq!(delivered, response());
    assert!(request.reap().unwrap());
    assert_eq!(budget.0.load(Ordering::Acquire), 1);
    drop(request);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

fn storage_boundary(extra: usize) -> Value {
    // Parsed short keys allocate eight bytes, and the format string allocates
    // 32; numeric token storage is exact, allowing a one-byte boundary change.
    let value_bytes = std::mem::size_of::<Value>();
    let entry_bytes = 4096 + 8 + value_bytes;
    let count = (RESPONSE_STORAGE - value_bytes - 32) / entry_bytes - 4;
    let digits = RESPONSE_STORAGE - (value_bytes + 32 + (count + 4) * entry_bytes) + extra;
    let mut result = BTreeMap::new();
    for index in 0..count {
        result.insert(format!("key{index:04}"), Value::Null);
    }
    result.insert("padding".into(), Value::Num("1".repeat(digits)));
    Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.native-response/1".into())),
        ("ok".into(), Value::Bool(true)),
        ("result".into(), Value::Obj(result)),
    ]))
}

#[test]
fn native_worker_public_poll_pins_exact_response_storage_and_one_byte_over() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    for extra in [0, 1] {
        let expected = storage_boundary(extra);
        let (inline, mut gate, mut output) = held_transport();
        frame(&mut output, &expected);
        drop(output);
        let mut request = NativeRequest {
            inline: Some(inline),
            worker: None,
            ticket: None,
        };
        assert!(request.poll().unwrap().is_none());
        gate.write_all(b"release\n").unwrap();
        let result = receive(&mut request);
        assert!(request.reap().unwrap());
        match extra {
            0 => assert_eq!(result.unwrap(), expected),
            _ => assert_eq!(
                result.unwrap_err(),
                "native client response retained storage exceeds bound"
            ),
        }
        drop(request);
    }
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_public_start_refuses_the_next_slot_before_helper_startup() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let tickets = (0..WORKER_SLOTS)
        .map(|_| reserve_current().unwrap().unwrap())
        .collect::<Vec<_>>();
    let request = parse(
        br#"{"format":"fsm.native-request/1","action":"prepare","payload":null}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let refusal = NativeRequest::start(
        "00000000000000000000000000000000",
        1,
        &request,
        Duration::from_secs(1),
    );
    assert!(matches!(refusal, Err(error) if error == "native transport worker capacity exhausted"));
    assert_eq!(budget.0.load(Ordering::Acquire), WORKER_SLOTS);
    drop(tickets);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_panic_after_actual_retirement_discards_the_published_response() {
    if std::env::var("FSM_NATIVE_UNWIND_TEST").ok().as_deref() == Some("adoption") {
        std::panic::set_hook(Box::new(
            crate::run::native_client::filter_native_worker_panics(|_| std::process::abort()),
        ));
    }
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let (inline, mut gate, mut output) = held_transport();
    frame(&mut output, &response());
    drop(output);
    let worker = Worker::start_after_retirement(inline, Arc::clone(&ticket), || {
        panic!("fixture panic after actual helper reap and EOF");
    })
    .unwrap_or_else(|_| panic!("worker startup failed"));
    let mut request = NativeRequest {
        inline: None,
        worker: Some(worker),
        ticket: Some(ticket),
    };
    gate.write_all(b"release\n").unwrap();
    assert_eq!(
        receive(&mut request).unwrap_err(),
        "native transport worker panicked; ownership remains uncertain"
    );
    assert!(request.reap().unwrap());
    let progress = request.progress();
    assert!(!progress.not_started && progress.reaped && progress.stdout_eof && progress.stderr_eof);
    assert_eq!(budget.reserved(), 1);
}
