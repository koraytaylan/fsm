//! Real fresh allocation, writer-held claiming and entry through public ticks.

use super::{WriterHolder, emit};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::{
    config::HandlerTable,
    run::{Pipeline, Runner},
    sched::Scheduler,
    service::{tick_reporting, tick_with},
    watch::Watcher,
};
use fsm_store::{clock::FixedClock, store::Store};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, Instant},
};

#[test]
#[ignore = "invoked only as an independent unprivileged native writer"]
fn writer_lease_only() {
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    emit(format_args!("\nFSM_NATIVE_WRITER_WAITING"));
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 1);
    assert_eq!(byte, [1]);
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let store = Store::open(Path::new(&path)).unwrap();
    let records = store.records.clone();
    let state = store.state.clone();
    let head = (store.journal.last_seq, store.journal.last_hash.clone());
    emit(format_args!("\nFSM_NATIVE_WRITER_READY"));
    std::io::stdout().flush().unwrap();
    assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 0);
    assert_eq!(store.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    assert_eq!(
        (store.journal.last_seq, store.journal.last_hash.clone()),
        head
    );
}

#[test]
#[ignore = "invoked only as an independent unprivileged cancelling writer"]
fn writer_cancel_pending() {
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let mut byte = [0];
    emit(format_args!("\nFSM_NATIVE_WRITER_WAITING"));
    std::io::stdout().flush().unwrap();
    assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 1);
    assert_eq!(byte, [1]);
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let mut store = Store::open(Path::new(&path)).unwrap();
    let before = store.records.len();
    emit(format_args!("\nFSM_NATIVE_WRITER_READY"));
    std::io::stdout().flush().unwrap();
    assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 1);
    assert_eq!(byte, [2]);
    if let Ok(encoded) = std::env::var("FSM_NATIVE_TEST_COMPETING_DOMAIN") {
        assert!(encoded.len() <= 8192);
        let domain = fsm_core::record::execution::NativeDomain::from_value(
            &parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap(),
        )
        .unwrap();
        let table =
            HandlerTable::parse(&std::env::var("FSM_NATIVE_TEST_HANDLER_TABLE").unwrap()).unwrap();
        let effect = store.state.instances["instance"].pending[0].clone();
        Pipeline
            .claim_native_handler(
                &mut store,
                &mut FixedClock::new(1000, 1),
                &effect,
                &table.handlers["notify"],
                &domain,
                "native-independent-winning-claim",
            )
            .unwrap();
    } else {
        store
            .cancel_instance("instance", "native-independent-cancel")
            .unwrap();
    }
    assert_eq!(store.records.len(), before + 1);
    let records = store.records.clone();
    emit(format_args!("\nFSM_NATIVE_WRITER_CANCELLED"));
    std::io::stdout().flush().unwrap();
    assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 0);
    assert_eq!(store.records, records);
}

#[test]
#[ignore = "invoked only as the provisioned unprivileged native admission control"]
fn shared_tick_admission() {
    assert_eq!(std::fs::metadata("/proc/self").unwrap().uid(), 65534);
    let path = std::env::var("FSM_NATIVE_TEST_STORE").unwrap();
    let path = Path::new(&path);
    let authority = std::env::var("FSM_NATIVE_TEST_AUTHORITY").unwrap();
    let authority = Path::new(&authority);
    let encoded = std::env::var("FSM_NATIVE_TEST_HANDLER_TABLE").unwrap();
    let table = HandlerTable::parse(&encoded).unwrap();
    let mut writer = Store::open(path).unwrap();
    let mut clock = FixedClock::new(1000, 1);
    writer
        .define_machine_on(
            &mut clock,
            parse(
                include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
            false,
            false,
        )
        .unwrap();
    writer
        .create_instance_ctx_on(
            &mut clock,
            "case_review",
            "instance",
            "admission-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    writer
        .send_event_stamp_on(
            &mut clock,
            "instance",
            "docs_ok",
            &mut Value::Obj(BTreeMap::new()),
            "admission-send",
            None,
            &[],
        )
        .unwrap();
    let effect = writer.state.instances["instance"].pending[0].clone();
    let pending = fsm_execute::effect::resolve(&writer, &effect).unwrap();
    let records = writer.records.clone();
    let state = writer.state.clone();
    let mut watcher = Watcher::with_handlers(path.into(), &table);
    assert_eq!(table.max_inflight, 1);
    let service_table = table.clone();
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new_native().unwrap();
    let mut pipeline = Pipeline;
    drop(writer);

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
    assert!(scheduler.inflight_effect(&effect).is_none());
    absent(
        authority,
        &["allocation", "binding", "launch", "entry", "handoff"],
    );
    drop(readonly);

    let cancelled = std::env::var_os("FSM_NATIVE_TEST_CANCEL_PRECLAIM").is_some();
    let competing = std::env::var_os("FSM_NATIVE_TEST_COMPETING_DOMAIN").is_some();
    let mut holder = if cancelled || competing {
        WriterHolder::start_test(
            path.to_str().unwrap(),
            "authority::allocator::native_tests::supervisor_probe::fresh_admission::writer_cancel_pending",
        )
    } else {
        WriterHolder::lease_only(path.to_str().unwrap())
    };
    holder.acquire();
    wait_until(|| {
        assert!(holder.child.try_wait().unwrap().is_none());
        tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            path,
            &mut clock,
            1000,
        )
        .writer_unavailable
    });
    assert!(scheduler.inflight_effect(&effect).is_none());
    absent(
        authority,
        &["allocation", "binding", "launch", "entry", "handoff"],
    );
    let mut readonly = Store::open_read_only(path).unwrap();
    assert_eq!(readonly.records, records);
    assert!(
        readonly
            .state
            .execution
            .claim_for("instance", &effect)
            .is_none()
    );
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
    drop(readonly);
    if cancelled || competing {
        holder.input.as_mut().unwrap().write_all(&[2]).unwrap();
        holder.wait_marker(b"FSM_NATIVE_WRITER_CANCELLED");
        wait_until(|| {
            assert!(holder.child.try_wait().unwrap().is_none());
            let outcome = tick_reporting(
                &mut watcher,
                &mut scheduler,
                &mut runner,
                &mut pipeline,
                path,
                &mut clock,
                1000,
            );
            assert!(!outcome.writer_unavailable);
            scheduler.inflight_effect(&effect).is_none()
        });
        if competing {
            for _ in 0..3 {
                assert!(holder.child.try_wait().unwrap().is_none());
                let report = tick_reporting(
                    &mut watcher,
                    &mut scheduler,
                    &mut runner,
                    &mut pipeline,
                    path,
                    &mut clock,
                    1000,
                );
                assert!(!report.writer_unavailable);
                assert!(scheduler.inflight_effect(&effect).is_none());
            }
        }
        absent(authority, &["binding", "launch", "entry", "handoff"]);
        let original = Store::open_read_only(path).unwrap();
        assert_eq!(original.records.len(), records.len() + 1);
        assert_eq!(&original.records[..records.len()], records.as_slice());
        if competing {
            let claim = original
                .state
                .execution
                .claim_for("instance", &effect)
                .unwrap();
            let encoded = std::env::var("FSM_NATIVE_TEST_COMPETING_DOMAIN").unwrap();
            assert_eq!(
                claim.domain().to_value(),
                parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap()
            );
            assert!(
                original.state.instances["instance"]
                    .pending
                    .contains(&effect)
            );
            assert_eq!(
                original.records.last().unwrap().kind,
                fsm_core::record::RecordKind::ExecutionClaimed
            );
        } else {
            assert!(original.state.execution.unresolved().next().is_none());
        }
        assert!(
            !original
                .state
                .dedup
                .contains_key(&fsm_execute::rid::ack_rid(&effect))
        );
        assert!(
            !original
                .state
                .dedup
                .contains_key(&fsm_execute::rid::event_rid(&effect, "docs_ok"))
        );
        drop(original);
        holder.release();
        emit(format_args!(
            "\n{}",
            if competing {
                "FSM_NATIVE_PRECLAIM_COMPETITION"
            } else {
                "FSM_NATIVE_PRECLAIM_CANCELLATION"
            }
        ));
        return;
    }
    holder.release();

    let mut writer = Store::open(path).unwrap();
    // Writer refusal released the unclaimed slot, so the healthy tick must
    // reserve again and observe bounded asynchronous preparation before claim.
    wait_until(|| {
        let lines = tick_with(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            &mut writer,
            &mut clock,
            1000,
        );
        if writer
            .state
            .execution
            .claim_for("instance", &effect)
            .is_some()
        {
            assert!(
                lines.iter().any(|line| line.starts_with("native-claimed ")),
                "{lines:?}"
            );
            return true;
        }
        assert_eq!(writer.records, records);
        assert_eq!(
            fsm_execute::effect::resolve(&writer, &effect).unwrap(),
            pending
        );
        assert!(fsm_store::snapshot::store_states_eq(&writer.state, &state));
        false
    });
    let claim = writer
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let namespace = std::env::var("FSM_NATIVE_TEST_NAMESPACE").unwrap();
    let lease_probe = super::owned_preparation::held_lease(&namespace, claim.domain());
    let claimed_records = writer.records.clone();
    let hash = writer.current_execution_claim_hash(&claim).unwrap();
    assert_eq!(
        claim.domain().to_value().get("allocation"),
        Some(&Value::Num("1".into()))
    );
    assert_eq!(
        claimed_records
            .iter()
            .filter(|record| record.kind == fsm_core::record::RecordKind::ExecutionClaimed)
            .count(),
        1
    );
    absent(authority, &["launch", "entry", "handoff"]);
    drop(writer);

    holder = WriterHolder::lease_only(path.to_str().unwrap());
    holder.acquire();
    wait_until(|| {
        assert!(holder.child.try_wait().unwrap().is_none());
        tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            path,
            &mut clock,
            1000,
        )
        .writer_unavailable
    });
    absent(authority, &["launch", "entry", "handoff"]);
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
    assert_eq!(readonly.records, claimed_records);
    assert_eq!(readonly.current_execution_claim_hash(&claim).unwrap(), hash);
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
    assert_eq!(writer.records, claimed_records);
    drop(writer);

    holder = WriterHolder::lease_only(path.to_str().unwrap());
    holder.acquire();
    wait_until(|| {
        assert!(holder.child.try_wait().unwrap().is_none());
        tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            path,
            &mut clock,
            1000,
        )
        .writer_unavailable
    });
    let readonly = Store::open_read_only(path).unwrap();
    assert_eq!(readonly.records, claimed_records);
    assert_eq!(readonly.current_execution_claim_hash(&claim).unwrap(), hash);
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
            .any(|line| line.contains("disposition=acked advance=advanced")),
        "{lines:?}"
    );
    assert!(
        writer
            .state
            .execution
            .claim_for("instance", &effect)
            .is_none()
    );
    assert!(scheduler.inflight_effect(&effect).is_none());
    lease_probe.try_lock().unwrap();
    let acknowledgement = fsm_execute::rid::ack_rid(&effect);
    let event = fsm_execute::rid::event_rid(&effect, "docs_ok");
    let acknowledgement_seq = writer
        .records
        .iter()
        .find(|record| {
            record.body.get("request_id").and_then(Value::as_str) == Some(acknowledgement.as_str())
        })
        .unwrap()
        .seq;
    let event_seq = writer
        .records
        .iter()
        .find(|record| {
            record.body.get("request_id").and_then(Value::as_str) == Some(event.as_str())
        })
        .unwrap()
        .seq;
    assert!(acknowledgement_seq < event_seq);
    let settled_records = writer.records.clone();
    tick_with(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        &mut writer,
        &mut clock,
        1000,
    );
    assert_eq!(writer.records, settled_records);

    // Reuse the same one-slot scheduler and Runner after durable consumption;
    // a newly constructed host would not prove release of the original slot.
    writer
        .create_instance_ctx_on(
            &mut clock,
            "case_review",
            "second-instance",
            "admission-second-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    writer
        .send_event_stamp_on(
            &mut clock,
            "second-instance",
            "docs_ok",
            &mut Value::Obj(BTreeMap::new()),
            "admission-second-send",
            None,
            &[],
        )
        .unwrap();
    let second = writer.state.instances["second-instance"].pending[0].clone();
    assert_ne!(second, effect);
    drop(writer);
    wait_until(|| {
        let report = tick_reporting(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            path,
            &mut clock,
            1000,
        );
        assert!(!report.writer_unavailable);
        let current = Store::open_read_only(path).unwrap();
        current
            .state
            .dedup
            .contains_key(&fsm_execute::rid::event_rid(&second, "docs_ok"))
    });
    assert!(scheduler.inflight_effect(&second).is_none());
    let current = Store::open_read_only(path).unwrap();
    assert!(current.state.execution.unresolved().next().is_none());
    assert_eq!(
        current
            .records
            .iter()
            .filter(|record| record.kind == fsm_core::record::RecordKind::ExecutionClaimed)
            .count(),
        2
    );
    let acknowledgement = fsm_execute::rid::ack_rid(&second);
    let event = fsm_execute::rid::event_rid(&second, "docs_ok");
    let sequence = |request: &str| {
        current
            .records
            .iter()
            .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(request))
            .unwrap()
            .seq
    };
    assert!(sequence(&acknowledgement) < sequence(&event));
    drop(current);
    public_service_run(path, service_table, &mut clock);
    emit(format_args!("\nFSM_NATIVE_FRESH_ADMISSION"));
}

fn public_service_run(path: &Path, table: HandlerTable, clock: &mut FixedClock) {
    let mut writer = Store::open(path).unwrap();
    writer
        .create_instance_ctx_on(
            clock,
            "case_review",
            "service-instance",
            "service-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    writer
        .send_event_stamp_on(
            clock,
            "service-instance",
            "docs_ok",
            &mut Value::Obj(BTreeMap::new()),
            "service-send",
            None,
            &[],
        )
        .unwrap();
    let effect = writer.state.instances["service-instance"].pending[0].clone();
    let prefix = writer.records.clone();
    drop(writer);
    let mut lines = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fsm_execute::service::run(
            fsm_execute::service::RunConfig {
                data_dir: path,
                table,
                poll_interval_ms: 5,
                contention: fsm_execute::service::Contention::Retry,
            },
            clock,
            &mut |line| {
                assert!(
                    lines.len() < 32,
                    "public service run diagnostics exceeded bound"
                );
                lines.push(line.to_owned());
                if line.starts_with("native-settled ") && line.contains(&effect) {
                    std::panic::panic_any("public-service-complete");
                }
            },
        )
        .unwrap();
    }));
    assert_eq!(
        *result.unwrap_err().downcast::<&str>().unwrap(),
        "public-service-complete"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("native-claimed ") && line.contains(&effect))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("native-launched ") && line.contains(&effect))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("disposition=acked advance=advanced"))
    );
    let current = Store::open_read_only(path).unwrap();
    assert_eq!(&current.records[..prefix.len()], prefix.as_slice());
    assert!(current.state.execution.unresolved().next().is_none());
    assert_eq!(
        current
            .records
            .iter()
            .filter(|record| record.kind == fsm_core::record::RecordKind::ExecutionClaimed)
            .count(),
        3
    );
    let sequence = |request: &str| {
        current
            .records
            .iter()
            .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(request))
            .unwrap()
            .seq
    };
    assert!(
        sequence(&fsm_execute::rid::ack_rid(&effect))
            < sequence(&fsm_execute::rid::event_rid(&effect, "docs_ok"))
    );
}

fn absent(authority: &Path, names: &[&str]) {
    let allocation = if std::env::var_os("FSM_NATIVE_TEST_COMPETING_DOMAIN").is_some() {
        2
    } else {
        1
    };
    for name in names {
        assert_eq!(
            std::fs::symlink_metadata(authority.join(format!("{name}-{allocation}.json")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "fresh admission readiness deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
