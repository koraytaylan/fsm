//! Public startup and cancellation use an actual held child, without native authority.

use super::*;
use crate::run::native_client::startup::{FixtureFactory, Startup};

const NAMESPACE: &str = "00000000000000000000000000000000";

fn prepare_message() -> Value {
    parse(
        br#"{"format":"fsm.native-request/1","action":"prepare","payload":null}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn native_worker_public_constructor_and_cancel_return_while_startup_is_held() {
    let budget = Arc::new(Budget::default());
    let reserved = Arc::clone(&budget);
    let (entered, startup_entered) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let (created, original) = mpsc::channel();
    let (returned, construction) = mpsc::channel();
    let caller = std::thread::spawn(move || {
        let _scope = Scope::enter(Some(&reserved));
        let _factory = FixtureFactory::install(move |prepared| {
            entered.send(std::thread::current().id()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            let (mut inline, gate, output) = held_transport();
            inline.deadline = prepared.deadline;
            created.send((gate, output)).unwrap();
            Ok(inline)
        });
        let owner = std::thread::current().id();
        let request =
            NativeRequest::start(NAMESPACE, 1, &prepare_message(), Duration::from_secs(10));
        returned.send((owner, request)).unwrap();
    });
    let executing = startup_entered
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let mut early = construction.recv_timeout(Duration::from_secs(1)).ok();
    let before_release = early.is_some();
    let observation = early.as_mut().map(|(_, request)| {
        let request = request.as_mut().unwrap();
        let progress = request.progress();
        let pending = request.poll().unwrap().is_none();
        let retired = request.reap().unwrap();
        request.cancel().unwrap();
        (progress, pending, retired)
    });
    // Always release startup and retire its real child before assertions,
    // including a source with synchronous construction deliberately restored.
    release.send(()).unwrap();
    let (owner, request) =
        early.unwrap_or_else(|| construction.recv_timeout(Duration::from_secs(5)).unwrap());
    let mut request = request.unwrap();
    caller.join().unwrap();
    let (_gate, output) = original.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(output);
    request.cancel().unwrap();
    let cancellation = receive(&mut request).unwrap_err();
    assert!(request.reap().unwrap());
    let final_progress = request.progress();
    assert!(before_release, "public constructor waited for startup");
    assert_ne!(executing, owner, "helper startup executed on its owner");
    let (progress, pending, retired) = observation.unwrap();
    assert!(!progress.not_started && !progress.is_retired());
    assert!(pending && !retired);
    assert_eq!(
        cancellation,
        "native client cancelled; claim remains uncertain"
    );
    assert!(!final_progress.not_started);
    assert!(final_progress.reaped && final_progress.stdout_eof && final_progress.stderr_eof);
    assert_eq!(budget.0.load(Ordering::Acquire), 1);
    drop(request);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_public_startup_refusal_retires_only_the_empty_transport() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let _factory = FixtureFactory::install(|_| Err("fixture helper startup refused".into()));
    let mut request =
        NativeRequest::start(NAMESPACE, 1, &prepare_message(), Duration::from_secs(1)).unwrap();
    assert_eq!(
        receive(&mut request).unwrap_err(),
        "fixture helper startup refused"
    );
    assert!(request.reap().unwrap());
    let progress = request.progress();
    assert!(progress.not_started && progress.is_retired());
    assert!(!progress.reaped && !progress.stdout_eof && !progress.stderr_eof);
    assert_eq!(budget.0.load(Ordering::Acquire), 1);
    drop(request);
    assert_eq!(budget.0.load(Ordering::Acquire), 0);
}

#[test]
fn native_worker_startup_refusal_is_unobserved_until_the_original_worker_is_joined() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let ticket = reserve_current().unwrap().unwrap();
    let _factory = FixtureFactory::install(|_| Err("fixture helper startup refused".into()));
    let prepared = super::super::super::PreparedRequest::prepare(
        NAMESPACE,
        1,
        &prepare_message(),
        Duration::from_secs(1),
    )
    .unwrap();
    let (arrived, observed) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let worker = Worker::start_prepared_after_retirement(
        Startup::new(prepared),
        Arc::clone(&ticket),
        move || {
            arrived.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
        },
    )
    .unwrap();
    let mut request = NativeRequest {
        inline: None,
        worker: Some(worker),
        ticket: Some(ticket),
    };
    let reached = observed.recv_timeout(Duration::from_secs(5)).is_ok();
    let before = request.progress();
    let retired = request.reap().unwrap();
    let early = request.poll();
    release.send(()).unwrap();
    let refusal = receive(&mut request).unwrap_err();
    assert!(reached);
    assert!(!before.not_started && !before.is_retired());
    assert!(!retired && matches!(early, Ok(None)));
    assert_eq!(refusal, "fixture helper startup refused");
    assert!(request.reap().unwrap());
    assert!(request.progress().not_started);
}

#[test]
fn native_worker_public_expired_startup_never_calls_the_helper_factory() {
    let budget = Arc::new(Budget::default());
    let _scope = Scope::enter(Some(&budget));
    let calls = Arc::new(AtomicUsize::new(0));
    let called = Arc::clone(&calls);
    let _factory = FixtureFactory::install(move |_| {
        called.fetch_add(1, Ordering::AcqRel);
        Err("fixture helper must not start".into())
    });
    let mut request =
        NativeRequest::start(NAMESPACE, 1, &prepare_message(), Duration::from_nanos(1)).unwrap();
    let refusal = receive(&mut request).unwrap_err();
    assert_eq!(
        refusal,
        "native client deadline before startup; ownership remains uncertain"
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert!(request.reap().unwrap());
    let progress = request.progress();
    assert!(progress.not_started);
    assert!(!progress.reaped && !progress.stdout_eof && !progress.stderr_eof);
}

#[test]
fn native_worker_standalone_constructor_keeps_synchronous_startup_refusal() {
    let original = std::thread::current().id();
    let _scope = Scope::enter(None);
    let _factory = FixtureFactory::install(move |_| {
        assert_eq!(std::thread::current().id(), original);
        Err("fixture synchronous refusal".into())
    });
    let refusal = NativeRequest::start(NAMESPACE, 1, &prepare_message(), Duration::from_secs(1));
    assert!(matches!(refusal, Err(error) if error == "fixture synchronous refusal"));
}
