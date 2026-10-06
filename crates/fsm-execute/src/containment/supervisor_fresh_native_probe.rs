//! Genuine published-claim installation and writer-gated entry through Runner.

use super::{WriterHolder, emit};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use fsm_execute::{
    config::{HandlerKind, HandlerSpec, HandlerTable, Retry},
    run::{Pipeline, Runner},
    sched::{Directive, Scheduler},
    service::{tick_reporting, tick_with},
    watch::{Observation, Watcher},
};
use fsm_store::{clock::FixedClock, store::Store};
use std::{
    collections::BTreeMap,
    os::unix::fs::MetadataExt,
    time::{Duration, Instant},
};

#[test]
#[ignore = "invoked only as the provisioned unprivileged fresh Runner control"]
fn shared_tick_fresh() {
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let path = std::path::Path::new(&path);
    let encoded = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    let binding = parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let (instance, effect) = claim.effect();
    let mut writer = Store::open(path).unwrap();
    let pending = fsm_execute::effect::resolve(&writer, effect).unwrap();
    let records = writer.records.clone();
    let state = writer.state.clone();
    let hash = writer.current_execution_claim_hash(&claim).unwrap();
    let mut table = HandlerTable {
        max_inflight: 1,
        ..HandlerTable::default()
    };
    // Reserve locally without executing this deliberately different handler.
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
                    pending: vec![pending.clone()],
                    ..Observation::default()
                },
                1000
            )
            .as_slice(),
        [Directive::Start { .. }]
    ));
    let mut watcher = Watcher::with_handlers(path.into(), &HandlerTable::default());
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(1000, 1);
    runner
        .start_native(&mut writer, &claim, &mut scheduler, Duration::from_secs(30))
        .unwrap();
    assert_eq!(
        runner
            .start_native(&mut writer, &claim, &mut scheduler, Duration::from_secs(30))
            .unwrap_err()
            .code,
        "exec/inflight_deferred"
    );
    assert_eq!(writer.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&writer.state, &state));
    drop(writer);
    let mut holder = WriterHolder::start(path.to_str().unwrap());
    holder.acquire();
    wait_for_writer_contention(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        path,
        &mut clock,
        &mut holder,
    );
    let domain = claim.domain().to_value();
    let authority = path.parent().unwrap().join(format!(
        "authority-{}",
        domain.get("generation").unwrap().as_num().unwrap()
    ));
    let allocation = domain.get("allocation").unwrap().as_num().unwrap();
    for _ in 0..3 {
        let outcome = tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            path,
            &mut clock,
            1000,
        );
        assert!(outcome.writer_unavailable);
        for name in ["launch", "entry", "handoff"] {
            assert_eq!(
                std::fs::symlink_metadata(authority.join(format!("{name}-{allocation}.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    let mut readonly = Store::open_read_only(path).unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut readonly,
        &mut clock,
        1000,
    );
    assert!(
        lines.iter().any(|line| line.contains("exec/mode")),
        "{lines:?}"
    );
    assert_eq!(readonly.records, records);
    assert!(fsm_store::snapshot::store_states_eq(
        &readonly.state,
        &state
    ));
    assert_eq!(scheduler.inflight_effect(effect), Some(&pending));
    drop(readonly);
    holder.release();
    let mut writer = Store::open(path).unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("native-launched ")),
        "{lines:?}"
    );
    assert_eq!(writer.records, records);
    drop(writer);
    // Execution proceeds without a writer, but its result cannot settle yet.
    holder = WriterHolder::start(path.to_str().unwrap());
    holder.acquire();
    wait_for_writer_contention(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        path,
        &mut clock,
        &mut holder,
    );
    let mut readonly = Store::open_read_only(path).unwrap();
    assert_eq!(readonly.current_execution_claim_hash(&claim).unwrap(), hash);
    assert_eq!(readonly.records, records);
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut readonly,
        &mut clock,
        1000,
    );
    assert!(
        lines.iter().any(|line| line.contains("exec/mode")),
        "{lines:?}"
    );
    assert_eq!(readonly.records, records);
    assert_eq!(scheduler.inflight_effect(effect), Some(&pending));
    drop(readonly);
    holder.release();
    let mut writer = Store::open(path).unwrap();
    writer
        .send_event(
            instance,
            "suspend",
            Value::Obj(BTreeMap::new()),
            "fresh-suspend",
            None,
        )
        .unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("disposition=acked advance=deferred")),
        "{lines:?}"
    );
    assert!(writer.state.execution.claim_for(instance, effect).is_none());
    assert!(scheduler.inflight_effect(effect).is_none());
    let settled = writer.records.clone();
    for _ in 0..3 {
        tick_with(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            &mut writer,
            &mut clock,
            1000,
        );
    }
    assert_eq!(writer.records, settled);
    writer
        .send_event(
            instance,
            "resume",
            Value::Obj(BTreeMap::new()),
            "fresh-resume",
            None,
        )
        .unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("disposition=acked advance=advanced")),
        "{lines:?}"
    );
    let ack = fsm_execute::rid::ack_rid(effect);
    let event = fsm_execute::rid::event_rid(effect, "docs_ok");
    let ack_record = writer
        .records
        .iter()
        .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(&ack))
        .unwrap();
    let event_record = writer
        .records
        .iter()
        .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(&event))
        .unwrap();
    assert!(ack_record.seq < event_record.seq);
    let count = writer.records.len();
    tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert_eq!(writer.records.len(), count);
    drop(writer);
    let cold = Store::open_read_only(path).unwrap();
    assert!(cold.state.dedup.contains_key(&ack) && cold.state.dedup.contains_key(&event));
    assert!(cold.state.execution.claim_for(instance, effect).is_none());
    emit(format_args!("\nFSM_NATIVE_FRESH_HANDOFF"));
}

#[allow(clippy::too_many_arguments)]
fn wait_for_writer_contention(
    watcher: &mut Watcher,
    scheduler: &mut Scheduler,
    runner: &mut Runner,
    pipeline: &mut Pipeline,
    path: &std::path::Path,
    clock: &mut FixedClock,
    holder: &mut WriterHolder,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "fresh Runner readiness deadline");
        assert!(holder.child.try_wait().unwrap().is_none());
        if tick_reporting(watcher, scheduler, runner, pipeline, path, clock, 1000)
            .writer_unavailable
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
