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

use super::super::handoff_cases::Handoff;

#[test]
#[ignore = "invoked only as the provisioned unprivileged fresh Runner control"]
fn shared_tick_fresh() {
    shared_tick_handoff(Handoff::Warm);
}

#[test]
#[ignore = "invoked only as the provisioned unprivileged cold handoff control"]
fn shared_tick_cold_handoff() {
    shared_tick_handoff(Handoff::Cold);
}

#[test]
#[ignore = "invoked only as the provisioned conflicting handoff control"]
fn shared_tick_conflicting_handoff() {
    shared_tick_handoff(Handoff::Conflicting);
}

#[test]
#[ignore = "invoked only as the provisioned rejected handoff control"]
fn shared_tick_rejected_handoff() {
    shared_tick_handoff(Handoff::Rejected);
}

fn shared_tick_handoff(case: Handoff) {
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
    let foreign_path = copy_store(path);
    let mut foreign = Store::open(&foreign_path).unwrap();
    assert_eq!(foreign.records, records);
    assert_eq!(foreign.current_execution_claim_hash(&claim).unwrap(), hash);
    assert_ne!(
        std::fs::metadata(path).unwrap().ino(),
        std::fs::metadata(&foreign_path).unwrap().ino()
    );
    // Even an unpinned host must authenticate the claim's physical authority
    // before installing ownership or requesting its first binding transport.
    let mut foreign_runner = Runner::new().unwrap();
    assert_eq!(
        foreign_runner
            .start_native(
                &mut foreign,
                &claim,
                &mut scheduler,
                Duration::from_secs(30)
            )
            .unwrap_err()
            .code,
        "exec/inflight_deferred"
    );
    assert_eq!(foreign.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&foreign.state, &state));
    assert_eq!(scheduler.inflight_effect(effect), Some(&pending));
    let domain = claim.domain().to_value();
    let authority = path.parent().unwrap().join(format!(
        "authority-{}",
        domain.get("generation").unwrap().as_num().unwrap()
    ));
    let allocation = domain.get("allocation").unwrap().as_num().unwrap();
    for name in ["binding", "launch", "entry", "handoff"] {
        assert_eq!(
            std::fs::symlink_metadata(authority.join(format!("{name}-{allocation}.json")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    drop(foreign);
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
    // The original independent writer still holds its lease: a healthy writer
    // on an identical copied prefix must not consume Bound entry permission.
    let mut foreign = Store::open(&foreign_path).unwrap();
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut foreign,
        &mut clock,
        1000,
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("exec/inflight_deferred")),
        "{lines:?}"
    );
    assert_eq!(foreign.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&foreign.state, &state));
    assert_eq!(foreign.current_execution_claim_hash(&claim).unwrap(), hash);
    assert_eq!(scheduler.inflight_effect(effect), Some(&pending));
    for name in ["launch", "entry", "handoff"] {
        assert_eq!(
            std::fs::symlink_metadata(authority.join(format!("{name}-{allocation}.json")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    let original = Store::open_read_only(path).unwrap();
    assert_eq!(original.records, records);
    assert!(fsm_store::snapshot::store_states_eq(
        &original.state,
        &state
    ));
    drop(original);
    drop(foreign);
    std::fs::remove_dir_all(&foreign_path).unwrap();
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
    assert_eq!(writer.state.execution_handoffs.outstanding().count(), 1);
    if case == Handoff::Rejected {
        let key = fsm_execute::rid::event_rid(effect, "docs_ok");
        let handoffs = writer.state.execution_handoffs.clone();
        let _ = writer.send_event(instance, "docs_ok", Value::Obj(BTreeMap::new()), &key, None);
        assert_eq!(
            writer.records.last().unwrap().kind,
            fsm_core::record::RecordKind::EventRejected
        );
        assert!(writer.state.dedup.contains_key(&key));
        assert_eq!(writer.state.execution_handoffs, handoffs);
    }
    if case.cold() {
        drop(runner);
        drop(writer);
        // The new host has no completion, scheduler slot or original table.
        runner = Runner::new().unwrap();
        scheduler = Scheduler::new(HandlerTable::default());
        watcher = Watcher::with_handlers(path.into(), &HandlerTable::default());
        writer = Store::open(path).unwrap();
        assert_eq!(writer.state.execution_handoffs.outstanding().count(), 1);
    }
    let original_ack = writer
        .records
        .iter()
        .find(|record| {
            record.body.get("request_id").and_then(Value::as_str)
                == Some(fsm_execute::rid::ack_rid(effect).as_str())
        })
        .unwrap()
        .clone();
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
    if case.cold() {
        let original = writer
            .state
            .execution_handoffs
            .outstanding()
            .next()
            .unwrap()
            .clone();
        for index in 0..2 {
            let archive = path.join(format!("cold-handoff-archive-{index}"));
            std::fs::create_dir(&archive).unwrap();
            writer
                .seal_and_archive_on(&mut clock, &archive, None)
                .unwrap();
            fsm_store::archive::verify(&archive).unwrap();
            drop(writer);
            drop(runner);
            runner = Runner::new().unwrap();
            scheduler = Scheduler::new(HandlerTable::default());
            watcher = Watcher::with_handlers(path.into(), &HandlerTable::default());
            writer = Store::open(path).unwrap();
            assert_eq!(
                writer
                    .state
                    .execution_handoffs
                    .outstanding()
                    .collect::<Vec<_>>(),
                vec![&original]
            );
            assert_eq!(writer.state.execution.unresolved().count(), 0);
        }
    }
    writer
        .send_event(
            instance,
            "resume",
            Value::Obj(BTreeMap::new()),
            "fresh-resume",
            None,
        )
        .unwrap();
    if case.cold() {
        let before = writer.records.clone();
        let handoffs = writer.state.execution_handoffs.clone();
        drop(writer);
        // Release retires the earlier helper; this phase needs a new actual
        // independent writer rather than reusing its closed stdin transport.
        holder = WriterHolder::start(path.to_str().unwrap());
        holder.acquire();
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
        let mut readonly = Store::open_read_only(path).unwrap();
        tick_with(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            &mut readonly,
            &mut clock,
            1000,
        );
        assert_eq!(readonly.records, before);
        assert_eq!(readonly.state.execution_handoffs, handoffs);
        drop(readonly);
        holder.release();
        writer = Store::open(path).unwrap();
        let copied = copy_store(path);
        let mut foreign = Store::open(&copied).unwrap();
        assert_eq!(foreign.records, before);
        assert_eq!(foreign.state.execution_handoffs, handoffs);
        let mut foreign_runner = Runner::new().unwrap();
        let mut foreign_scheduler = Scheduler::new(HandlerTable::default());
        let mut foreign_watcher = Watcher::with_handlers(copied.clone(), &HandlerTable::default());
        let lines = tick_with(
            &mut foreign_watcher,
            &mut foreign_scheduler,
            &mut foreign_runner,
            &mut pipeline,
            &mut foreign,
            &mut clock,
            1000,
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("exec/inflight_deferred")),
            "{lines:?}"
        );
        assert_eq!(foreign.records, before);
        assert_eq!(foreign.state.execution_handoffs, handoffs);
        assert_eq!(writer.records, before);
        assert_eq!(writer.state.execution_handoffs, handoffs);
        drop(foreign_runner);
        drop(foreign);
        std::fs::remove_dir_all(copied).unwrap();
    }
    if case == Handoff::Conflicting {
        let key = fsm_execute::rid::event_rid(effect, "docs_ok");
        let handoffs = writer.state.execution_handoffs.clone();
        writer
            .send_event(
                instance,
                "note_added",
                Value::Obj(BTreeMap::from([(
                    "text".into(),
                    Value::Str("foreign".into()),
                )])),
                &key,
                None,
            )
            .unwrap();
        assert_eq!(
            writer.records.last().unwrap().kind,
            fsm_core::record::RecordKind::EventApplied
        );
        assert_eq!(writer.state.execution_handoffs, handoffs);
    }
    if case.retained() {
        let handoffs = writer.state.execution_handoffs.clone();
        let before = writer.records.clone();
        let lines = tick_with(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            &mut writer,
            &mut clock,
            1000,
        );
        if case == Handoff::Conflicting {
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("req/request_id_conflict")),
                "{lines:?}"
            );
        } else {
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains("native-handoff parked")
                        || line.contains("error exec/store")),
                "{lines:?}"
            );
        }
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
        assert_eq!(writer.records, before);
        assert_eq!(writer.state.execution_handoffs, handoffs);
        drop(writer);
        let reopened = Store::open_read_only(path).unwrap();
        assert_eq!(reopened.state.execution_handoffs, handoffs);
        assert_eq!(reopened.state.execution.unresolved().count(), 0);
        emit(format_args!(
            "\n{}",
            if case == Handoff::Rejected {
                "FSM_NATIVE_REJECTED_HANDOFF"
            } else {
                "FSM_NATIVE_CONFLICTING_HANDOFF"
            }
        ));
        return;
    }
    let lines = tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    if case.cold() {
        assert!(
            lines
                .iter()
                .any(|line| line.contains("native-handoff advanced")),
            "{lines:?}"
        );
    } else {
        assert!(
            lines
                .iter()
                .any(|line| line.contains("disposition=acked advance=advanced")),
            "{lines:?}"
        );
    }
    assert_eq!(writer.state.execution_handoffs.outstanding().count(), 0);
    let ack = fsm_execute::rid::ack_rid(effect);
    let event = fsm_execute::rid::event_rid(effect, "docs_ok");
    let ack_record = if case.cold() {
        &original_ack
    } else {
        writer
            .records
            .iter()
            .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(&ack))
            .unwrap()
    };
    let event_record = writer
        .records
        .iter()
        .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(&event))
        .unwrap();
    assert!(ack_record.seq < event_record.seq);
    let count = writer.records.len();
    // Resuming re-enters in_review and emits a distinct notify; this check
    // concerns replay of the original completion, not admission of that work.
    assert!(
        writer.state.instances[instance]
            .pending
            .iter()
            .any(|pending| pending != effect)
    );
    scheduler = Scheduler::new(HandlerTable::default());
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
    emit(format_args!(
        "\n{}",
        if case.cold() {
            "FSM_NATIVE_COLD_HANDOFF"
        } else {
            "FSM_NATIVE_FRESH_HANDOFF"
        }
    ));
}

fn copy_store(source: &std::path::Path) -> std::path::PathBuf {
    fn copy_entry(source: &std::path::Path, target: &std::path::Path, depth: usize) {
        assert!(depth < 32, "fixture directory depth exceeds bound");
        let metadata = std::fs::symlink_metadata(source).unwrap();
        if metadata.is_dir() {
            std::fs::create_dir(target).unwrap();
            let entries: Vec<_> = std::fs::read_dir(source).unwrap().take(4097).collect();
            assert!(entries.len() <= 4096, "fixture directory exceeds bound");
            for entry in entries {
                let entry = entry.unwrap();
                copy_entry(&entry.path(), &target.join(entry.file_name()), depth + 1);
            }
        } else {
            assert!(
                metadata.is_file(),
                "fixture must contain only regular files"
            );
            std::fs::copy(source, target).unwrap();
        }
    }
    // Snapshot top-level entries before creating the nested destination, so
    // copying never descends into its own output in the unprivileged fixture.
    let entries: Vec<_> = std::fs::read_dir(source).unwrap().take(4097).collect();
    assert!(entries.len() <= 4096, "fixture directory exceeds bound");
    let target = source.join("foreign-writer-prefix");
    std::fs::create_dir(&target).unwrap();
    for entry in entries {
        let entry = entry.unwrap();
        copy_entry(&entry.path(), &target.join(entry.file_name()), 0);
    }
    target
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
