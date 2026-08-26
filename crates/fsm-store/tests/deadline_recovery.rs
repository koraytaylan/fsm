//! Crash-recovery proofs for durable deadline polls.
//!
//! Deadline polls claim request IDs and publish state-bearing records. These
//! tests exercise those records through the same public classify, repair,
//! reopen, and replay paths used after a process interruption.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::RecordKind;
use fsm_core::replay::{NopSink, StoreState, fold_with};
use fsm_store::clock::FixedClock;
use fsm_store::journal_io::{
    JournalHealth, classify, load_records, repair_truncate_torn_tail, verify,
};
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-store-{test_name}-{}-{sequence}",
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

fn parallel_deadline_definition() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"deadline_recovery",
            "context":[{"name":"fires","ty":"int","init":"0"}],
            "events":[],
            "regions":[
                {
                    "name":"review",
                    "states":[
                        {"name":"reviewing"},
                        {"name":"review_timed_out","terminal":true}
                    ],
                    "initial":"reviewing"
                },
                {
                    "name":"audit",
                    "states":[
                        {"name":"auditing"},
                        {"name":"audit_timed_out","terminal":true}
                    ],
                    "initial":"auditing"
                }
            ],
            "transitions":[],
            "deadlines":[
                {
                    "name":"review_timeout",
                    "from":"reviewing",
                    "after":"dur(10, ms)",
                    "to":"review_timed_out",
                    "do":[{"target":"fires","value":"ctx.fires + 1"}]
                },
                {
                    "name":"audit_timeout",
                    "from":"auditing",
                    "after":"dur(20, ms)",
                    "to":"audit_timed_out"
                }
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("deadline recovery fixture is valid JSON")
}

fn deadline_schedule_failure_definition() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"deadline_schedule_failure",
            "context":[{"name":"wait","ty":"duration","init":"1"}],
            "events":[{"name":"go","fields":[]}],
            "states":[
                {"name":"a"},
                {"name":"b"},
                {"name":"done","terminal":true}
            ],
            "initial":"a",
            "transitions":[{
                "from":"a",
                "on":"go",
                "to":"b",
                "do":[{"target":"wait","value":"-ctx.wait"}]
            }],
            "deadlines":[
                {
                    "name":"enter_b",
                    "from":"a",
                    "after":"dur(1, ms)",
                    "to":"b",
                    "do":[{"target":"wait","value":"-ctx.wait"}]
                },
                {
                    "name":"b_timeout",
                    "from":"b",
                    "after":"ctx.wait",
                    "to":"done"
                }
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("schedule-failure fixture is valid JSON")
}

fn create_schedule_failure_instance(directory: &Path) -> Store {
    let mut store = Store::open(directory).expect("open empty test store");
    let mut define_clock = FixedClock::new(1, 1);
    store
        .define_machine_on(
            &mut define_clock,
            deadline_schedule_failure_definition(),
            false,
            false,
        )
        .expect("define schedule-failure machine");
    let mut create_clock = FixedClock::new(100, 1);
    store
        .create_instance_ctx_on(
            &mut create_clock,
            "deadline_schedule_failure",
            "case-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("create schedule-failure instance");
    store
}

fn create_scheduled_instance(directory: &Path) -> Store {
    let mut store = Store::open(directory).expect("open empty test store");
    let mut define_clock = FixedClock::new(1, 1);
    store
        .define_machine_on(
            &mut define_clock,
            parallel_deadline_definition(),
            false,
            false,
        )
        .expect("define deadline machine");

    let mut create_clock = FixedClock::new(100, 1);
    store
        .create_instance_ctx_on(
            &mut create_clock,
            "deadline_recovery",
            "case-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("create scheduled instance");
    assert_eq!(create_clock.now, 101, "creation reads its clock once");
    assert_eq!(
        store
            .state
            .instances
            .get("case-1")
            .expect("created instance")
            .deadlines,
        BTreeMap::from([
            ("audit_timeout".to_string(), 120),
            ("review_timeout".to_string(), 110),
        ])
    );
    store
}

fn last_record_cut(bytes: &[u8]) -> (usize, Vec<u8>) {
    assert_eq!(bytes.last(), Some(&b'\n'), "journal records end in LF");
    let last_record_end = bytes.len() - 1;
    let last_record_start = bytes[..last_record_end]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |position| position + 1);
    let cut = last_record_start + (last_record_end - last_record_start) / 2;
    assert!(
        cut > last_record_start,
        "deadline record has a non-empty prefix"
    );
    (last_record_start, bytes[last_record_start..cut].to_vec())
}

fn assert_persisted_state_eq(expected: &StoreState, actual: &StoreState) {
    assert_eq!(actual.last_seq, expected.last_seq);
    assert_eq!(actual.last_hash, expected.last_hash);
    assert_eq!(actual.instances, expected.instances);
    assert_eq!(actual.instance_machines, expected.instance_machines);
    assert_eq!(actual.dedup, expected.dedup);
    assert_eq!(
        actual.machines.keys().collect::<Vec<_>>(),
        expected.machines.keys().collect::<Vec<_>>()
    );
}

#[test]
fn torn_deadline_application_repairs_to_scheduled_prefix_and_can_apply_once() {
    let directory = TestDirectory::create("deadline-torn-tail");
    let mut store = create_scheduled_instance(directory.path());
    let before_poll = store
        .state
        .instances
        .get("case-1")
        .expect("created instance")
        .clone();
    let before_poll_seq = store.state.last_seq;
    let segment_path = directory
        .path()
        .join("journal")
        .join(&store.journal.seg_name);

    let mut poll_clock = FixedClock::new(110, 1);
    let applied = store
        .poll_instance_deadline_on(&mut poll_clock, "case-1", "poll-torn", None)
        .expect("apply due deadline");
    assert_eq!(
        applied.get("deadline_applied").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        store.records.last().map(|record| record.kind),
        Some(RecordKind::DeadlineApplied)
    );
    drop(store);

    let mut segment_bytes = fs::read(&segment_path).expect("read journal segment");
    let (last_record_start, quarantined_prefix) = last_record_cut(&segment_bytes);
    segment_bytes.truncate(last_record_start + quarantined_prefix.len());
    fs::write(&segment_path, &segment_bytes).expect("simulate interrupted append");

    match classify(directory.path()) {
        JournalHealth::TornTail {
            segment,
            offset,
            bytes,
        } => {
            assert_eq!(
                segment_path.file_name().and_then(|name| name.to_str()),
                Some(segment.as_str())
            );
            assert_eq!(offset, last_record_start as u64);
            assert_eq!(bytes, quarantined_prefix.len() as u64);
        }
        health => panic!("partial deadline record must be TornTail, got {health:?}"),
    }

    let repair = repair_truncate_torn_tail(directory.path()).expect("repair torn deadline tail");
    assert_eq!(repair.truncated_to_seq, before_poll_seq);
    assert_eq!(repair.bytes, quarantined_prefix.len() as u64);
    assert_eq!(
        fs::read(&repair.quarantined).expect("read quarantined deadline bytes"),
        quarantined_prefix
    );
    assert!(matches!(classify(directory.path()), JournalHealth::Ok));

    let mut recovered = Store::open(directory.path()).expect("open repaired store");
    assert_eq!(recovered.state.last_seq, before_poll_seq);
    assert_eq!(
        recovered.state.instances.get("case-1"),
        Some(&before_poll),
        "repair retains the pre-poll configuration and both schedules"
    );
    assert!(
        !recovered.state.dedup.contains_key("poll-torn"),
        "the torn request claim is not retained"
    );

    let mut replacement_clock = FixedClock::new(110, 1);
    let replacement = recovered
        .poll_instance_deadline_on(&mut replacement_clock, "case-1", "poll-torn", None)
        .expect("reapply the repaired-away request");
    assert_eq!(
        replacement.get("deadline_applied").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(recovered.state.last_seq, before_poll_seq + 1);

    let committed_seq = recovered.state.last_seq;
    let mut duplicate_clock = FixedClock::new(999, 1);
    let duplicate = recovered
        .poll_instance_deadline_on(&mut duplicate_clock, "case-1", "poll-torn", None)
        .expect("retry committed deadline poll");
    assert_eq!(duplicate_clock.now, 999, "dedup precedes the clock read");
    assert_eq!(recovered.state.last_seq, committed_seq);
    assert_eq!(
        duplicate.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn durable_deadline_application_reopens_and_folds_exactly() {
    let directory = TestDirectory::create("deadline-durable-fold");
    let mut store = create_scheduled_instance(directory.path());
    let mut poll_clock = FixedClock::new(110, 1);
    let applied = store
        .poll_instance_deadline_on(&mut poll_clock, "case-1", "poll-durable", None)
        .expect("apply first due deadline");
    assert_eq!(
        applied.get("deadline").and_then(Value::as_str),
        Some("review_timeout")
    );

    let expected_instance = InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([
                ("audit".to_string(), "auditing".to_string()),
                ("review".to_string(), "review_timed_out".to_string()),
            ]),
        },
        ctx: BTreeMap::from([("fires".to_string(), Val::Int(1))]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::from([("audit_timeout".to_string(), 120)]),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    assert_eq!(
        store.state.instances.get("case-1"),
        Some(&expected_instance)
    );
    let expected_state = store.state.clone();
    let durable_seq = store.state.last_seq;
    let durable_slot = store
        .state
        .dedup
        .get("poll-durable")
        .expect("deadline poll claims request ID")
        .clone();
    assert_eq!(durable_slot.seq, durable_seq);
    assert!(
        durable_slot.fp.is_some(),
        "current records persist fingerprints"
    );
    drop(store);

    assert!(matches!(verify(directory.path()).health, JournalHealth::Ok));
    let reopened = Store::open(directory.path()).expect("reopen durable deadline record");
    assert_persisted_state_eq(&expected_state, &reopened.state);
    assert_eq!(
        reopened.state.instances.get("case-1"),
        Some(&expected_instance)
    );
    assert_eq!(
        reopened.state.dedup.get("poll-durable"),
        Some(&durable_slot)
    );

    let records = load_records(directory.path()).expect("load durable journal");
    let folded = fold_with(records, &mut NopSink).expect("fold durable deadline record");
    assert_persisted_state_eq(&expected_state, &folded);
    assert_eq!(folded.instances.get("case-1"), Some(&expected_instance));
    assert_eq!(folded.dedup.get("poll-durable"), Some(&durable_slot));

    drop(reopened);
    let mut reopened = Store::open(directory.path()).expect("reopen for idempotent retry");
    let mut retry_clock = FixedClock::new(999, 1);
    let duplicate = reopened
        .poll_instance_deadline_on(&mut retry_clock, "case-1", "poll-durable", None)
        .expect("replay durable deadline response");
    assert_eq!(retry_clock.now, 999, "durable dedup skips the clock read");
    assert_eq!(reopened.state.last_seq, durable_seq);
    assert_eq!(
        duplicate.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        duplicate.get("deadline").and_then(Value::as_str),
        Some("review_timeout")
    );
}

#[test]
fn event_schedule_rejection_with_source_only_details_reopens_exactly() {
    let directory = TestDirectory::create("event-schedule-rejection");
    let mut store = create_schedule_failure_instance(directory.path());
    let before = store.state.instances["case-1"].clone();
    let mut event_clock = FixedClock::new(100, 1);
    let original = store
        .send_event_stamp_on(
            &mut event_clock,
            "case-1",
            "go",
            &mut Value::Obj(BTreeMap::new()),
            "event-rejected",
            None,
            &[],
        )
        .expect_err("negative entered-state deadline rejects the event");
    assert_eq!(original.code, "run/action_error");
    assert_eq!(
        original.details.get("source_state").and_then(Value::as_str),
        Some("b")
    );
    assert!(original.details.get("transition_idx").is_none());
    assert_eq!(store.state.instances["case-1"], before);
    assert_eq!(
        store.records.last().map(|record| record.kind),
        Some(RecordKind::EventRejected)
    );
    drop(store);

    let mut reopened = Store::open(directory.path()).expect("replay source-only event rejection");
    assert_eq!(reopened.state.instances["case-1"], before);
    let mut retry_clock = FixedClock::new(999, 1);
    let duplicate = reopened
        .send_event_stamp_on(
            &mut retry_clock,
            "case-1",
            "go",
            &mut Value::Obj(BTreeMap::new()),
            "event-rejected",
            None,
            &[],
        )
        .expect_err("retry replays the rejection");
    assert_eq!(duplicate, original.mark_duplicate());
    assert_eq!(retry_clock.now, 999, "dedup precedes the clock read");
}

#[test]
fn completed_parallel_instance_status_gates_unknown_event_and_replays_rejection() {
    let directory = TestDirectory::create("completed-parallel-unknown-event");
    let mut store = create_scheduled_instance(directory.path());
    store
        .poll_instance_deadline_on(&mut FixedClock::new(110, 1), "case-1", "review-poll", None)
        .expect("first region completes");
    store
        .poll_instance_deadline_on(&mut FixedClock::new(120, 1), "case-1", "audit-poll", None)
        .expect("second region completes");
    assert_eq!(store.state.instances["case-1"].status, Status::Completed);

    let mut send_clock = FixedClock::new(200, 1);
    let original = store
        .send_event_stamp_on(
            &mut send_clock,
            "case-1",
            "undeclared",
            &mut Value::Obj(BTreeMap::new()),
            "after-completion",
            None,
            &[],
        )
        .expect_err("status gate precedes unknown-event validation");
    assert_eq!(original.code, "run/instance_completed");
    assert_eq!(
        send_clock.now, 201,
        "durable rejection reads the clock once"
    );
    assert_eq!(
        store.records.last().map(|record| record.kind),
        Some(RecordKind::EventRejected)
    );
    let rejected_seq = store.state.last_seq;
    drop(store);

    let mut reopened = Store::open(directory.path()).expect("replay completed rejection");
    assert_eq!(reopened.state.instances["case-1"].status, Status::Completed);
    let mut retry_clock = FixedClock::new(999, 1);
    let duplicate = reopened
        .send_event_stamp_on(
            &mut retry_clock,
            "case-1",
            "undeclared",
            &mut Value::Obj(BTreeMap::new()),
            "after-completion",
            None,
            &[],
        )
        .expect_err("retry replays the state-dependent rejection");
    assert_eq!(duplicate, original.mark_duplicate());
    assert_eq!(reopened.state.last_seq, rejected_seq);
    assert_eq!(retry_clock.now, 999, "dedup precedes the clock read");
}

#[test]
fn deadline_schedule_rejection_with_source_only_details_reopens_exactly() {
    let directory = TestDirectory::create("deadline-schedule-rejection");
    let mut store = create_schedule_failure_instance(directory.path());
    let before = store.state.instances["case-1"].clone();
    let mut poll_clock = FixedClock::new(101, 1);
    let original = store
        .poll_instance_deadline_on(&mut poll_clock, "case-1", "deadline-rejected", None)
        .expect_err("negative entered-state deadline rejects the due transition");
    assert_eq!(original.code, "run/action_error");
    assert_eq!(
        original.details.get("source_state").and_then(Value::as_str),
        Some("b")
    );
    assert!(original.details.get("transition_idx").is_none());
    assert_eq!(store.state.instances["case-1"], before);
    assert_eq!(
        store.records.last().map(|record| record.kind),
        Some(RecordKind::DeadlineRejected)
    );
    drop(store);

    let mut reopened =
        Store::open(directory.path()).expect("replay source-only deadline rejection");
    assert_eq!(reopened.state.instances["case-1"], before);
    let mut retry_clock = FixedClock::new(999, 1);
    let duplicate = reopened
        .poll_instance_deadline_on(&mut retry_clock, "case-1", "deadline-rejected", None)
        .expect_err("retry replays the rejection");
    assert_eq!(duplicate, original.mark_duplicate());
    assert_eq!(retry_clock.now, 999, "dedup precedes the clock read");
}
