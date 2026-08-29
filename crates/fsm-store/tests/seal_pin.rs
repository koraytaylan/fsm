//! The lowest sequence a live derivation still depends on, and the cut it
//! admits.
//!
//! Plan 0017 task 7904. The pin is a pure function of a folded state and a
//! record list, so most of these cases are constructed directly: the property
//! is which record a pending effect needs, not the machinery that produced it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::{Record, RecordKind, seal, zeros};
use fsm_core::replay::StoreState;
use fsm_store::seal_pin::{PinSource, admissible, highest_admissible_cut, pin};
use fsm_store::store::Store;

const CASE_REVIEW: &[u8] =
    include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A directory name no other run of this binary can produce.
///
/// A process id alone is not unique enough: a full `--workspace` run spawns
/// thousands of short-lived processes, ids get reused, and a reused id names a
/// directory a previous run may still be finishing with — which surfaces as a
/// `store/lock` naming *this* process. `crash_harness.rs` learned the same
/// thing and pins it with a test; this is that idiom.
fn invocation_tag() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(tag: &str) -> Self {
        let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("fsm-seal-pin-{tag}-{}-{index}", invocation_tag()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        Self(path)
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

fn hash(byte: u8) -> String {
    format!("sha256:{}", format!("{byte:02x}").repeat(32))
}

fn record(seq: u64, kind: RecordKind, body: BTreeMap<String, Value>) -> Record {
    seal(seq, seq as i64, kind, Value::Obj(body), &zeros())
}

fn instance(status: Status, pending: &[&str]) -> InstanceState {
    InstanceState {
        status,
        configuration: ActiveConfiguration::Sequential {
            leaf: "intake".into(),
        },
        ctx: BTreeMap::new(),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: pending.iter().map(|id| (*id).to_string()).collect(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn state_with(instances: &[(&str, InstanceState)], last_seq: u64) -> StoreState {
    let mut state = StoreState {
        last_seq,
        ..StoreState::default()
    };
    for (id, value) in instances {
        state.instances.insert((*id).into(), value.clone());
    }
    state
}

fn creation(seq: u64, instance_id: &str) -> Record {
    record(
        seq,
        RecordKind::InstanceCreated,
        BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("machine_id".into(), Value::Str("m".into())),
            (
                "request_id".into(),
                Value::Str(format!("create-{instance_id}")),
            ),
            ("state_hash".into(), Value::Str(hash(0x11))),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
            ("leaf".into(), Value::Str("intake".into())),
        ]),
    )
}

fn invocation(seq: u64, parent: &str, child: &str) -> Record {
    record(
        seq,
        RecordKind::InstanceInvoked,
        BTreeMap::from([
            ("parent_instance_id".into(), Value::Str(parent.into())),
            ("slot".into(), Value::Str("review".into())),
            ("child_instance_id".into(), Value::Str(child.into())),
            ("child_machine_id".into(), Value::Str("m".into())),
            ("request_id".into(), Value::Str("invoke".into())),
            ("state_hash".into(), Value::Str(hash(0x22))),
            ("child_state_hash".into(), Value::Str(hash(0x33))),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ]),
    )
}

fn attempt(seq: u64, instance_id: &str, effect_id: &str, number: u64) -> Record {
    record(
        seq,
        RecordKind::EffectAttempted,
        BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("effect_id".into(), Value::Str(effect_id.into())),
            ("request_id".into(), Value::Str(format!("attempt-{seq}"))),
            ("outcome".into(), Value::Str("failed".into())),
            ("attempt".into(), Value::Num(number.to_string())),
            ("state_hash".into(), Value::Str(hash(0x44))),
        ]),
    )
}

fn emitting(seq: u64, instance_id: &str) -> Record {
    record(
        seq,
        RecordKind::EventApplied,
        BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("event".into(), Value::Str("docs_ok".into())),
            ("payload".into(), Value::Obj(BTreeMap::new())),
            ("request_id".into(), Value::Str(format!("send-{seq}"))),
            ("state_hash".into(), Value::Str(hash(0x55))),
            ("exited".into(), Value::Arr(Vec::new())),
            ("entered".into(), Value::Arr(Vec::new())),
            ("source_state".into(), Value::Str("intake".into())),
        ]),
    )
}

#[test]
fn a_store_with_nothing_pending_has_no_pin_and_admits_a_head_cut() {
    // The case that keeps the feature useful: a workflow running for a year
    // but idle at a gate holds no records hostage, because its whole history
    // is derivable from the base.
    let state = state_with(&[("idle", instance(Status::Running, &[]))], 40_000);
    let records = vec![creation(3, "idle")];
    assert_eq!(pin(&state, &records), None);
    assert_eq!(highest_admissible_cut(&state, &records), None);
    assert!(admissible(40_000, &state, &records).is_ok());
}

#[test]
fn a_pending_effect_pins_below_its_emitting_record() {
    let state = state_with(
        &[("live", instance(Status::Running, &["live/900/0"]))],
        40_000,
    );
    let records = vec![creation(3, "live"), emitting(900, "live")];
    let found = pin(&state, &records).expect("a pending effect pins");
    assert_eq!(found.seq, 900);
    assert_eq!(found.source, PinSource::EmittingRecord);
    assert_eq!(found.instance_id, "live");
    assert_eq!(found.effect_id, "live/900/0");
    assert_eq!(found.highest_admissible_cut(), 899);
}

#[test]
fn a_creation_emitted_effect_pins_below_the_creation_record() {
    // `{instance}/0/{k}` carries a literal zero: the id is composed before the
    // record's own sequence is known, so what it needs is the creation record
    // wherever it landed — never journal seq 0, which is genesis.
    let state = state_with(
        &[("live", instance(Status::Running, &["live/0/0"]))],
        40_000,
    );
    let records = vec![creation(17, "live")];
    let found = pin(&state, &records).expect("a creation-time effect pins");
    assert_eq!(found.seq, 17);
    assert_eq!(found.source, PinSource::CreationRecord);
}

#[test]
fn a_childs_creation_emitted_effect_pins_below_its_invocation_record() {
    // There is no `instance_created` for a child: its whole existence is
    // derived from the `instance_invoked` record. A reader that needs the
    // former has already lost, and so has a pin that looks for one.
    let state = state_with(
        &[("child", instance(Status::Running, &["child/0/0"]))],
        40_000,
    );
    let records = vec![creation(3, "parent"), invocation(21, "parent", "child")];
    let found = pin(&state, &records).expect("a child's creation-time effect pins");
    assert_eq!(found.seq, 21);
    assert_eq!(found.source, PinSource::CreationRecord);
    assert_eq!(found.instance_id, "child");
}

#[test]
fn attempts_pin_below_the_earliest_of_them_not_the_latest() {
    // `attempt_state` derives the count from **all** the attempt records, so
    // losing the earliest lowers the count and an exhausted effect retries
    // again — `exec/retries_exhausted` would never fire.
    let state = state_with(
        &[("live", instance(Status::Running, &["live/900/0"]))],
        40_000,
    );
    let records = vec![
        creation(3, "live"),
        emitting(900, "live"),
        attempt(910, "live", "live/900/0", 1),
        attempt(920, "live", "live/900/0", 2),
        attempt(930, "live", "live/900/0", 3),
    ];
    let found = pin(&state, &records).expect("attempts pin");
    // 900 is lower still, so the emitting record wins here; remove it and the
    // earliest attempt is what remains.
    assert_eq!(found.seq, 900);
    let creation_only = vec![
        creation(3, "live"),
        attempt(910, "live", "live/0/0", 1),
        attempt(920, "live", "live/0/0", 2),
    ];
    let state = state_with(
        &[("live", instance(Status::Running, &["live/0/0"]))],
        40_000,
    );
    let found = pin(&state, &creation_only).expect("attempts pin");
    assert_eq!(
        found.seq, 3,
        "the creation record is lower than any attempt"
    );
    let late_creation = vec![
        creation(950, "live"),
        attempt(910, "live", "live/0/0", 1),
        attempt(920, "live", "live/0/0", 2),
    ];
    let found = pin(&state, &late_creation).expect("attempts pin");
    assert_eq!(found.seq, 910, "the earliest attempt bounds the cut");
    assert_eq!(found.source, PinSource::AttemptRecord);
}

#[test]
fn the_pin_is_the_minimum_across_instances_and_names_the_one_responsible() {
    let state = state_with(
        &[
            ("later", instance(Status::Running, &["later/900/0"])),
            ("earlier", instance(Status::Running, &["earlier/400/0"])),
        ],
        40_000,
    );
    let records = vec![
        creation(3, "later"),
        creation(4, "earlier"),
        emitting(400, "earlier"),
        emitting(900, "later"),
    ];
    let found = pin(&state, &records).expect("the lower of the two pins");
    assert_eq!(found.seq, 400);
    assert_eq!(found.instance_id, "earlier");
}

#[test]
fn a_settled_instances_outstanding_effects_do_not_pin() {
    // A cancelled instance's effects are never retried, so the records their
    // execution would be derived from are not load-bearing.
    for status in [Status::Cancelled, Status::Completed] {
        let state = state_with(&[("gone", instance(status, &["gone/900/0"]))], 40_000);
        let records = vec![creation(3, "gone"), emitting(900, "gone")];
        assert_eq!(pin(&state, &records), None, "{status:?} pinned the cut");
    }
}

#[test]
fn an_acked_effect_does_not_pin_even_with_attempts_above_the_cut() {
    // An ack removes the id from `pending`, and only a pending effect pins.
    let state = state_with(&[("live", instance(Status::Running, &[]))], 40_000);
    let records = vec![
        creation(3, "live"),
        emitting(900, "live"),
        attempt(910, "live", "live/900/0", 1),
    ];
    assert_eq!(pin(&state, &records), None);
}

#[test]
fn an_inadmissible_cut_is_refused_and_the_cut_its_hint_names_is_accepted() {
    let state = state_with(
        &[("live", instance(Status::Running, &["live/900/0"]))],
        40_000,
    );
    let records = vec![creation(3, "live"), emitting(900, "live")];
    let error = admissible(40_000, &state, &records).expect_err("a head cut is refused");
    assert_eq!(error.code, "store/archive_refused");
    let named = error
        .details
        .get("highest_admissible_cut")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse::<u64>().ok())
        .expect("the refusal names the highest admissible cut");
    assert_eq!(named, 899);
    assert!(
        error.hint.contains(&named.to_string()),
        "a hint nobody can act on is prose: {}",
        error.hint
    );
    // The number it names is usable, which is the whole point of naming one.
    assert!(admissible(named, &state, &records).is_ok());
    assert!(
        admissible(named + 1, &state, &records).is_err(),
        "the boundary is exactly where the hint says it is"
    );
}

#[test]
fn the_refusal_names_the_instance_the_effect_and_which_scan_pinned_it() {
    let state = state_with(
        &[("live", instance(Status::Running, &["live/0/0"]))],
        40_000,
    );
    let records = vec![creation(17, "live")];
    let error = admissible(40_000, &state, &records).expect_err("a head cut is refused");
    for (field, expected) in [
        ("instance_id", "live"),
        ("effect_id", "live/0/0"),
        ("source", "creation_record"),
    ] {
        assert_eq!(
            error.details.get(field).and_then(Value::as_str),
            Some(expected),
            "the refusal does not name {field}"
        );
    }
}

#[test]
fn the_pin_takes_no_lock_and_writes_nothing() {
    // A monitoring session must be able to ask this while a writer holds the
    // store, which is what makes `--dry-run` a read-only question.
    let directory = TestDirectory::create("readonly");
    let mut store = Store::open(directory.path()).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    store
        .create_instance("case_review", "live", "create-live", None)
        .expect("create succeeds");
    let before: Vec<_> = fs::read_dir(directory.path())
        .expect("the directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    // The writer is still open and holding the lock while this runs.
    assert_eq!(pin(&store.state, &store.records), None);
    assert!(admissible(store.state.last_seq, &store.state, &store.records).is_ok());
    let after: Vec<_> = fs::read_dir(directory.path())
        .expect("the directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(before, after, "the pin changed the data directory");
}
