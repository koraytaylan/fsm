//! Provisioned original-binding refusal and bound-before-entry closure.

use super::emit;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::{
    config::{HandlerKind, HandlerSpec, HandlerTable, Retry},
    run::{
        Runner,
        native_client::{NativeRequest, NativeShutdown},
    },
    sched::{Directive, Scheduler},
    watch::Observation,
};
use fsm_store::store::Store;
use std::{
    collections::BTreeMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[test]
#[ignore = "invoked only by the provisioned unprivileged claimed closure control"]
fn bound_claimed_closure() {
    original_bound_closure();
    emit(format_args!("\nFSM_NATIVE_BOUND_CLOSURE"));
}

struct BoundClosure {
    data_dir: PathBuf,
    claim: Claim,
    shutdown: NativeShutdown,
}

#[test]
#[ignore = "invoked only by the provisioned unprivileged interrupted closure control"]
fn bound_claimed_interruption() {
    let closed = original_bound_closure();
    let mut clock = fsm_store::clock::FixedClock::new(2000, 1);
    let mut readonly = Store::open_read_only(&closed.data_dir).unwrap();
    let count = readonly.records.len();
    assert_eq!(
        closed
            .shutdown
            .settle_interrupted(&mut readonly, &mut clock)
            .unwrap_err()
            .code,
        "exec/mode"
    );
    assert_eq!(readonly.records.len(), count);
    drop(readonly);
    let mut writer = Store::open(&closed.data_dir).unwrap();
    let instance = writer.state.instances[closed.claim.effect().0].clone();
    let count = writer.records.len();
    let response = closed
        .shutdown
        .settle_interrupted(&mut writer, &mut clock)
        .unwrap();
    assert_eq!(
        response
            .get("execution")
            .and_then(|body| body.get("disposition"))
            .and_then(Value::as_str),
        Some("interrupted")
    );
    assert_eq!(writer.records.len(), count + 2);
    assert_eq!(
        writer.records[count].kind,
        fsm_core::record::RecordKind::ExecutionStopped
    );
    assert_eq!(
        writer.records[count + 1].kind,
        fsm_core::record::RecordKind::ExecutionSettled
    );
    assert_eq!(writer.state.instances[closed.claim.effect().0], instance);
    assert!(
        writer
            .state
            .execution
            .claim_for(closed.claim.effect().0, closed.claim.effect().1)
            .is_none()
    );
    let replay = closed
        .shutdown
        .settle_interrupted(&mut writer, &mut clock)
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(writer.records.len(), count + 2);
    drop(writer);
    let mut cold = Store::open(&closed.data_dir).unwrap();
    let replay = closed
        .shutdown
        .settle_interrupted(&mut cold, &mut clock)
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(cold.records.len(), count + 2);
    assert_eq!(cold.state.instances[closed.claim.effect().0], instance);
    emit(format_args!("\nFSM_NATIVE_BOUND_INTERRUPTION"));
}

fn original_bound_closure() -> BoundClosure {
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let encoded = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    let binding = parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let path = Path::new(&path);
    let mut writer = Store::open(path).unwrap();
    let records = writer.records.clone();
    let hash = writer.current_execution_claim_hash(&claim).unwrap();
    assert_eq!(
        binding.get("journal_claim"),
        Some(&Value::Str(hash.clone()))
    );
    let pending = fsm_execute::effect::resolve(&writer, claim.effect().1).unwrap();
    let mut table = HandlerTable::default();
    table.handlers.insert(
        pending.effect_name.clone(),
        HandlerSpec {
            effect: pending.effect_name.clone(),
            kind: HandlerKind::Process,
            argv: vec!["/bin/false".into()],
            timeout_ms: 30000,
            on_ok: None,
            on_failed: None,
            retry: Retry::default(),
        },
    );
    let mut scheduler = Scheduler::new(table);
    assert!(matches!(
        scheduler
            .on_observation(
                &Observation {
                    pending: vec![pending],
                    ..Observation::default()
                },
                1000
            )
            .as_slice(),
        [Directive::Start { .. }]
    ));
    let mut runner = Runner::new().unwrap();
    runner
        .start_native(&mut writer, &claim, &mut scheduler, Duration::from_secs(10))
        .unwrap();
    let domain = claim.domain().to_value();
    let namespace = domain.get("namespace").unwrap().as_str().unwrap();
    let generation = domain
        .get("generation")
        .unwrap()
        .as_num()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let allocation = domain.get("allocation").unwrap().as_num().unwrap();
    let authority = Path::new("/var/lib/fsm-containment")
        .join(namespace)
        .join(format!("authority-{generation}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !authority
        .join(format!("binding-{allocation}.json"))
        .exists()
    {
        assert!(Instant::now() < deadline);
        assert!(runner.finished_effects().is_empty());
        std::thread::sleep(Duration::from_millis(5));
    }
    // Observe only; no shared tick may enter the bound owner.
    assert!(runner.finished_effects().is_empty());
    let Value::Obj(mut wrong_binding) = binding.clone() else {
        unreachable!()
    };
    wrong_binding.insert(
        "journal_claim".into(),
        Value::Str(format!(
            "sha256:{}{}",
            if &hash[7..8] == "a" { "b" } else { "a" },
            &hash[8..]
        )),
    );
    assert_ne!(
        wrong_binding.get("journal_claim"),
        binding.get("journal_claim")
    );
    let request = Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.native-request/1".into())),
        ("action".into(), Value::Str("close-claimed".into())),
        ("payload".into(), Value::Obj(wrong_binding)),
    ]));
    let mut refused =
        NativeRequest::start(namespace, generation, &request, Duration::from_secs(5)).unwrap();
    let response = loop {
        if let Some(response) = refused.poll().unwrap() {
            break response;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(response.get("ok"), Some(&Value::Bool(false)));
    assert_eq!(
        response.get("result").and_then(Value::as_str),
        Some("claimed closure original protected binding differs")
    );
    assert!(
        !authority
            .join(format!("closing-{allocation}.json"))
            .exists()
    );
    assert!(!authority.join(format!("closed-{allocation}.json")).exists());
    let snapshot = Store::open_read_only(path).unwrap();
    let mut shutdown = NativeShutdown::start(&snapshot, &claim, Duration::from_secs(5)).unwrap();
    let proof = loop {
        if let Some(proof) = shutdown.poll().unwrap() {
            break proof;
        }
        assert!(Instant::now() < deadline);
        assert!(runner.finished_effects().is_empty());
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(proof.matches_claim(&claim, &hash));
    proof.check_store(path).unwrap();
    let helper = shutdown.progress();
    assert!(helper.reaped && helper.stdout_eof && helper.stderr_eof);
    for name in [
        "launch",
        "entry",
        "handoff",
        "manager-stopped",
        "manager-retired",
    ] {
        assert!(!authority.join(format!("{name}-{allocation}.json")).exists());
    }
    assert_eq!(writer.records, records);
    assert_eq!(
        writer
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1),
        Some(&claim)
    );
    assert!(
        writer.state.instances[claim.effect().0]
            .pending
            .contains(&claim.effect().1.to_owned())
    );
    drop(snapshot);
    drop(writer);
    let cold = Store::open_read_only(path).unwrap();
    assert_eq!(
        cold.state
            .execution
            .claim_for(claim.effect().0, claim.effect().1),
        Some(&claim)
    );
    BoundClosure {
        data_dir: path.to_owned(),
        claim,
        shutdown,
    }
}
