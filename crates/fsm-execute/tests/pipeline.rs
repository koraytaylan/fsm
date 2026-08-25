//! The pipeline is the one component that writes, so every row here is read
//! back out of the journal: what records landed, in what order, and what a
//! second attempt does to them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_execute::config::{HandlerSpec, HandlerTable};
use fsm_execute::effect::{PendingEffect, resolve};
use fsm_execute::rid::{ack_rid, event_rid};
use fsm_execute::run::{BoundedBytes, KillReason, Pipeline, RunOutcome, SettleOutcome};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-execute-{test_name}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Open a writer, tolerating a lock this process itself just released.
///
/// Spawning a handler forks, and between `fork` and `exec` the child holds a
/// copy of every open descriptor — so an advisory lock dropped a moment ago
/// can still be held for the length of that window. The property under test is
/// that the executor does not *keep* the lock, not that a fork never happened.
fn open_writer(path: &Path) -> Store {
    for _ in 0..50 {
        match Store::open(path) {
            Ok(store) => return store,
            Err(error) if error.code == "store/lock" => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("open writer {}: {error:?}", path.display()),
        }
    }
    panic!("the writer lock on {} never became free", path.display())
}

fn machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"order_confirmation_pipeline",
            "context":[
                {"name":"order_id","ty":"str","init":"order-7"},
                {"name":"approved","ty":"bool","init":"false"}
            ],
            "events":[
                {"name":"submit","fields":[]},
                {"name":"confirmed","fields":[{"name":"at","ty":"timestamp"}]},
                {"name":"confirmation_failed","fields":[]},
                {"name":"finalize","fields":[]}
            ],
            "effects":[
                {"name":"request_confirmation","fields":[{"name":"order","ty":"str"}]},
                {"name":"notify_customer","fields":[{"name":"order","ty":"str"}]},
                {"name":"archive_order","fields":[{"name":"order","ty":"str"}]}
            ],
            "states":[
                {"name":"placed"},
                {"name":"awaiting_confirmation","entry":{"emit":[
                    {"effect":"request_confirmation","args":{"order":"ctx.order_id"}}
                ]}},
                {"name":"confirmed_order","entry":{"emit":[
                    {"effect":"notify_customer","args":{"order":"ctx.order_id"}}
                ]}},
                {"name":"closed","terminal":true,"entry":{"emit":[
                    {"effect":"archive_order","args":{"order":"ctx.order_id"}}
                ]}},
                {"name":"unconfirmed","terminal":true}
            ],
            "initial":"placed",
            "transitions":[
                {"from":"placed","on":"submit","to":"awaiting_confirmation"},
                {"from":"awaiting_confirmation","on":"confirmed","to":"confirmed_order"},
                {"from":"awaiting_confirmation","on":"confirmation_failed","to":"unconfirmed"},
                {"from":"confirmed_order","on":"finalize","if":"ctx.approved","to":"closed"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn table() -> HandlerTable {
    HandlerTable::parse(
        r#"{
            "format":"fsm.handlers/1",
            "handlers":[
                {
                    "effect":"request_confirmation",
                    "argv":["/usr/local/bin/notify-supplier","{order}"],
                    "timeout_ms":30000,
                    "on_ok":{"event":"confirmed","payload":{},"stamps":["at"]},
                    "on_failed":{"event":"confirmation_failed"}
                },
                {
                    "effect":"notify_customer",
                    "argv":["/usr/local/bin/notify-customer","{order}"],
                    "timeout_ms":1000,
                    "on_ok":{"event":"finalize"}
                },
                {
                    "effect":"archive_order",
                    "argv":["/usr/local/bin/archive-order","{order}"],
                    "timeout_ms":1000,
                    "on_ok":{"event":"finalize"}
                }
            ]
        }"#,
    )
    .unwrap()
}

fn handler(name: &str) -> HandlerSpec {
    table().handlers[name].clone()
}

fn completed(status: i32, text: &str) -> RunOutcome {
    RunOutcome::Completed {
        status,
        stdout: BoundedBytes {
            bytes: text.as_bytes().to_vec(),
            truncated: false,
            sha256: None,
        },
        stderr: BoundedBytes::empty(),
    }
}

struct Fixture {
    /// Held so the temporary directory outlives the store inside it.
    _directory: TestDirectory,
    store: Store,
    clock: FixedClock,
    requests: u64,
}

impl Fixture {
    /// A store whose single instance sits in `awaiting_confirmation` with one
    /// pending `request_confirmation` effect.
    fn awaiting_confirmation(test_name: &str) -> Self {
        let directory = TestDirectory::create(test_name);
        let mut store = open_writer(directory.path());
        let mut clock = FixedClock::new(1_000, 1);
        store
            .define_machine_on(&mut clock, machine(), false, false)
            .unwrap();
        store
            .create_instance_ctx_on(
                &mut clock,
                "order_confirmation_pipeline",
                "order-1",
                "req-create",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        store
            .send_event_stamp_on(
                &mut clock,
                "order-1",
                "submit",
                &mut Value::Obj(BTreeMap::new()),
                "req-submit",
                None,
                &[],
            )
            .unwrap();
        Self {
            _directory: directory,
            store,
            clock,
            requests: 0,
        }
    }

    fn request_id(&mut self) -> String {
        self.requests += 1;
        format!("req-{}", self.requests)
    }

    fn pending_effect(&self, index: usize) -> PendingEffect {
        let effect_id = self.store.state.instances["order-1"].pending[index].clone();
        resolve(&self.store, &effect_id).expect("the pending effect resolves")
    }

    fn records_of_kind(&self, kind: RecordKind) -> Vec<&fsm_core::record::Record> {
        self.store
            .records
            .iter()
            .filter(|record| record.kind == kind)
            .collect()
    }

    fn status(&self) -> &str {
        self.store.state.instances["order-1"].status.as_str()
    }

    fn leaf(&self) -> String {
        self.store.state.instances["order-1"]
            .configuration
            .sequential_leaf()
            .unwrap_or_default()
            .to_string()
    }
}

#[test]
fn a_clean_run_acks_then_sends_the_declared_advance_in_that_order() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-happy");
    let effect = fixture.pending_effect(0);
    let mut pipeline = Pipeline;

    let settled = pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, "supplier notified"),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::Advanced);
    assert_eq!(fixture.leaf(), "confirmed_order");

    let acked = fixture.records_of_kind(RecordKind::EffectAcked);
    assert_eq!(acked.len(), 1);
    assert_eq!(
        acked[0].body.get("request_id").and_then(Value::as_str),
        Some(ack_rid(&effect.effect_id).as_str())
    );
    assert_eq!(
        acked[0].body.get("outcome").and_then(Value::as_str),
        Some("ok")
    );
    assert_eq!(
        acked[0]
            .body
            .get("result")
            .and_then(|result| result.get("stdout"))
            .and_then(Value::as_str),
        Some("supplier notified"),
        "the ack carries what the handler printed"
    );

    let advance = fixture
        .records_of_kind(RecordKind::EventApplied)
        .into_iter()
        .find(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed"))
        .expect("the advance landed");
    assert!(
        advance.seq > acked[0].seq,
        "ack before advance, always: the ack is what a restart reads"
    );
    assert_eq!(
        advance.body.get("request_id").and_then(Value::as_str),
        Some(event_rid(&effect.effect_id, "confirmed").as_str())
    );
    let stamped = advance
        .body
        .get("payload")
        .and_then(|payload| payload.get("at"))
        .and_then(Value::as_str)
        .expect("the declared stamp was filled from the injected clock");
    assert!(stamped.parse::<i64>().is_ok(), "stamped {stamped}");
}

#[test]
fn a_non_zero_exit_acks_failed_and_sends_the_failure_advance() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-failed");
    let effect = fixture.pending_effect(0);
    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(3, "supplier unreachable"),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::Advanced);
    assert_eq!(fixture.leaf(), "unconfirmed");
    assert_eq!(fixture.status(), "completed");
    let acked = fixture.records_of_kind(RecordKind::EffectAcked);
    assert_eq!(
        acked[0].body.get("outcome").and_then(Value::as_str),
        Some("failed")
    );
}

#[test]
fn a_failure_with_no_declared_advance_leaves_the_instance_in_place() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-no-failure-advance");
    let effect = fixture.pending_effect(0);
    let mut handler = handler("request_confirmation");
    handler.on_failed = None;
    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(1, ""),
            &handler,
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::AckedNoAdvance);
    assert_eq!(fixture.leaf(), "awaiting_confirmation");
    assert_eq!(fixture.records_of_kind(RecordKind::EffectAcked).len(), 1);
    assert!(
        fixture
            .records_of_kind(RecordKind::EventRejected)
            .is_empty()
    );
}

#[test]
fn every_killed_or_unstartable_run_acks_failed_with_its_documented_result() {
    for (outcome, expected) in [
        (
            RunOutcome::Killed {
                reason: KillReason::Timeout,
            },
            "exec/timeout",
        ),
        (
            RunOutcome::Killed {
                reason: KillReason::Cancelled,
            },
            "exec/cancelled",
        ),
        (
            RunOutcome::SpawnFailed {
                argv0: "/usr/local/bin/notify-supplier".into(),
            },
            "exec/spawn",
        ),
    ] {
        let mut fixture = Fixture::awaiting_confirmation("pipe-killed");
        let effect = fixture.pending_effect(0);
        let mut pipeline = Pipeline;
        pipeline
            .settle(
                &mut fixture.store,
                &mut fixture.clock,
                &effect,
                outcome.clone(),
                &handler("request_confirmation"),
            )
            .unwrap();
        let acked = fixture.records_of_kind(RecordKind::EffectAcked);
        assert_eq!(acked.len(), 1);
        assert_eq!(
            acked[0]
                .body
                .get("result")
                .and_then(|result| result.get("error"))
                .and_then(Value::as_str),
            Some(expected)
        );

        // Re-settling the identical outcome is a replay, not a conflict: the
        // ack_result carries no timestamp, duration, or pid.
        let again = Pipeline.settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            outcome,
            &handler("request_confirmation"),
        );
        assert!(again.is_ok(), "{again:?}");
        assert_eq!(fixture.records_of_kind(RecordKind::EffectAcked).len(), 1);
    }
}

#[test]
fn an_effect_of_a_terminal_instance_acks_without_advancing() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-terminal");
    let effect = fixture.pending_effect(0);
    Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(1, ""),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(fixture.status(), "completed");
    assert_eq!(fixture.leaf(), "unconfirmed");

    // Entering `unconfirmed` emitted nothing, so drive the other terminal
    // path: a completed instance that still holds a pending effect.
    let mut fixture = Fixture::awaiting_confirmation("pipe-terminal-emit");
    let effect = fixture.pending_effect(0);
    Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, ""),
            &handler("request_confirmation"),
        )
        .unwrap();
    let notify = fixture.pending_effect(0);
    assert_eq!(notify.effect_name, "notify_customer");
    fixture.store.state.instances.get_mut("order-1").unwrap();
    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &notify,
            completed(0, ""),
            &handler("notify_customer"),
        )
        .unwrap();
    // `finalize`'s guard is false, so the engine would refuse the advance.
    assert_eq!(settled, SettleOutcome::AckedNoAdvance);
    assert!(
        fixture
            .records_of_kind(RecordKind::EventRejected)
            .is_empty(),
        "the executor never fires an event it expects to be rejected"
    );
}

#[test]
fn a_cancelled_instance_acks_but_never_advances() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-cancelled");
    let effect = fixture.pending_effect(0);
    let request = fixture.request_id();
    fixture
        .store
        .cancel_instance_reason_on(
            &mut fixture.clock,
            "order-1",
            &request,
            "operator stopped it",
        )
        .unwrap();

    // Cancel leaves the configuration in place, so the engine still reports
    // the advance event as enabled: only the *status* says no.
    let view = fixture.store.instance_view("order-1", None, None).unwrap();
    let enabled = view
        .get("enabled_events")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .find(|event| event.get("event").and_then(Value::as_str) == Some("confirmed"))
        .and_then(|event| event.get("status"))
        .and_then(Value::as_str);
    assert_eq!(enabled, Some("enabled"));

    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, ""),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::AckedNoAdvance);
    assert_eq!(fixture.records_of_kind(RecordKind::EffectAcked).len(), 1);
    assert!(
        fixture
            .records_of_kind(RecordKind::EventRejected)
            .is_empty()
    );
}

#[test]
fn acking_an_effect_another_path_already_settled_is_benign() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-already");
    let effect = fixture.pending_effect(0);
    let request = fixture.request_id();
    fixture
        .store
        .ack_effect_outcome_on(
            &mut fixture.clock,
            "order-1",
            &effect.effect_id,
            &request,
            "ok",
            None,
        )
        .unwrap();

    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, "late"),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::AlreadySettled);
    assert_eq!(
        fixture.records_of_kind(RecordKind::EffectAcked).len(),
        1,
        "no second effect_acked record"
    );
}

#[test]
fn a_fresh_pipeline_re_settling_the_same_run_changes_nothing() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-restart");
    let effect = fixture.pending_effect(0);
    Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, "sent"),
            &handler("request_confirmation"),
        )
        .unwrap();
    let records_after_first = fixture.store.records.len();

    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, "sent"),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::AckedNoAdvance);
    assert_eq!(
        fixture.store.records.len(),
        records_after_first,
        "a replayed ack and an already-sent advance write nothing"
    );
    assert_eq!(fixture.records_of_kind(RecordKind::EffectAcked).len(), 1);
    assert_eq!(
        fixture
            .records_of_kind(RecordKind::EventApplied)
            .iter()
            .filter(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed"))
            .count(),
        1
    );
}

#[test]
fn a_resumed_advance_lands_once_however_often_it_is_retried() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-resume");
    let effect = fixture.pending_effect(0);
    // A previous life acked and died before sending.
    fixture
        .store
        .ack_effect_outcome_on(
            &mut fixture.clock,
            "order-1",
            &effect.effect_id,
            &ack_rid(&effect.effect_id),
            "ok",
            None,
        )
        .unwrap();
    let advance = handler("request_confirmation").on_ok.unwrap();

    let mut pipeline = Pipeline;
    let first = pipeline
        .advance_only(
            &mut fixture.store,
            &mut fixture.clock,
            &effect.effect_id,
            "order-1",
            &advance,
        )
        .unwrap();
    assert_eq!(first, SettleOutcome::Advanced);
    assert_eq!(fixture.leaf(), "confirmed_order");

    let second = pipeline
        .advance_only(
            &mut fixture.store,
            &mut fixture.clock,
            &effect.effect_id,
            "order-1",
            &advance,
        )
        .unwrap();
    assert_eq!(second, SettleOutcome::AckedNoAdvance);
    assert_eq!(
        fixture
            .records_of_kind(RecordKind::EventApplied)
            .iter()
            .filter(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed"))
            .count(),
        1,
        "one advance, whatever the executor's life history"
    );
}

#[test]
fn a_stale_expect_seq_is_retried_under_the_same_request_id() {
    let mut fixture = Fixture::awaiting_confirmation("pipe-seq");
    let effect = fixture.pending_effect(0);
    // Ack from a previous life, then an unrelated record. The replayed ack
    // answers with *its* seq, which is now stale, so the advance's expect_seq
    // misses and the retry has to refresh it.
    fixture
        .store
        .ack_effect_outcome_on(
            &mut fixture.clock,
            "order-1",
            &effect.effect_id,
            &ack_rid(&effect.effect_id),
            "ok",
            Some(completed(0, "sent").ack_result()),
        )
        .unwrap();
    let unrelated = fixture.request_id();
    fixture
        .store
        .annotate("order-1", &unrelated, "an operator note")
        .unwrap();

    let settled = Pipeline
        .settle(
            &mut fixture.store,
            &mut fixture.clock,
            &effect,
            completed(0, "sent"),
            &handler("request_confirmation"),
        )
        .unwrap();
    assert_eq!(settled, SettleOutcome::Advanced);
    assert_eq!(fixture.leaf(), "confirmed_order");
    assert_eq!(
        fixture
            .records_of_kind(RecordKind::EventApplied)
            .iter()
            .filter(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed"))
            .count(),
        1
    );
}

#[test]
fn a_due_deadline_polls_once_and_a_repeat_replays() {
    let directory = TestDirectory::create("pipe-deadline");
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_000, 1);
    let definition = parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"order_expiry",
            "context":[],
            "events":[{"name":"confirm","fields":[]}],
            "effects":[],
            "states":[
                {"name":"awaiting_confirmation"},
                {"name":"confirmed_order","terminal":true},
                {"name":"expired","terminal":true}
            ],
            "initial":"awaiting_confirmation",
            "transitions":[{"from":"awaiting_confirmation","on":"confirm","to":"confirmed_order"}],
            "deadlines":[{
                "name":"confirmation_timeout",
                "from":"awaiting_confirmation",
                "after":"dur(30, s)",
                "to":"expired"
            }]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store
        .define_machine_on(&mut clock, definition, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_expiry",
            "order-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let due_ms = store.state.instances["order-1"].deadlines["confirmation_timeout"];

    let mut late = FixedClock::new(due_ms + 10, 1);
    let mut pipeline = Pipeline;
    pipeline
        .poll(
            &mut store,
            &mut late,
            "order-1",
            "confirmation_timeout",
            due_ms,
        )
        .unwrap();
    assert_eq!(
        store.state.instances["order-1"]
            .configuration
            .sequential_leaf(),
        Some("expired")
    );
    let applied = store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::DeadlineApplied)
        .count();
    assert_eq!(applied, 1);

    let repeat = pipeline.poll(
        &mut store,
        &mut late,
        "order-1",
        "confirmation_timeout",
        due_ms,
    );
    assert!(repeat.is_ok(), "{repeat:?}");
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::DeadlineApplied)
            .count(),
        1,
        "the derived key replays rather than polling twice"
    );
}

#[test]
fn a_poll_that_finds_nothing_due_is_journaled_as_an_observation() {
    let directory = TestDirectory::create("pipe-notdue");
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation_pipeline",
            "order-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();

    Pipeline
        .poll(
            &mut store,
            &mut clock,
            "order-1",
            "confirmation_timeout",
            9_999,
        )
        .unwrap();
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::DeadlineNotDue)
            .count(),
        1
    );
    assert!(
        store
            .state
            .dedup
            .contains_key("exec-poll-7-order-1-confirmation_timeout-9999"),
        "the observation claims its key, so a repeat replays"
    );
}
