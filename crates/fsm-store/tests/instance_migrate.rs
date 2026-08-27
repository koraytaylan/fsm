//! One record moves one instance, and everything it claims can be checked
//! from the journal alone.
//!
//! Plan 0011 task 5501.

// Rows hand back the store's own `ErrorObj`, which is how the code under
// test reports a failure. Boxing it would only make every assertion
// dereference to read a code.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::migrate::preview::preview;
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{NopSink, fold_with};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-migop-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
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

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

const OLD: &str = r#"{"format":"fsm.machine/1","name":"review_v1","states":[{"name":"intake"},{"name":"triage"},{"name":"awaiting_countersign"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"3"}],"events":[{"name":"go","fields":[]},{"name":"stall","fields":[]}],"effects":[{"name":"notify","fields":[]}],"deadlines":[{"name":"old_timer","from":"triage","after":"dur(60, s)","to":"intake"}],"transitions":[{"from":"intake","on":"go","to":"triage","emit":[{"effect":"notify"}]},{"from":"intake","on":"stall","to":"awaiting_countersign"}]}"#;

fn new_with(states: &str, extra_states: &str, extra_transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"review_v2","states":[{{"name":"intake"}},{{"name":"triage"}}{extra_states}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"effects":[{{"name":"notify","fields":[]}}],"deadlines":[{{"name":"new_timer","from":"triage","after":"dur(30, s)","to":"intake"}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}{extra_transitions}],"supersedes":{{"machine":"{}","states":{states},"context":{{"score":"ctx.score + 1"}}}}}}"#,
        digest(OLD)
    )
}

const MAPPED: &str = r#"{"intake":"intake","triage":"triage"}"#;

/// A store holding both definitions and one instance on the old one.
fn ready(directory: &TestDirectory, new_source: &str) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(OLD), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(new_source), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "review_v1",
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn migrated_record(store: &Store) -> &Record {
    store
        .records
        .iter()
        .find(|record| record.kind == RecordKind::InstanceMigrated)
        .expect("one migration record")
}

#[test]
fn one_record_moves_the_instance_and_commits_its_new_state() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    let before = store.records.len();
    let response = store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    assert_eq!(store.records.len(), before + 1);
    assert_eq!(
        response.get("migrated").and_then(Value::as_bool),
        Some(true)
    );

    let record = migrated_record(&store);
    assert_eq!(
        record.body.get("from_machine_id").and_then(Value::as_str),
        Some(machine_id(&value(OLD)).as_str())
    );
    assert_eq!(
        record.body.get("to_machine_id").and_then(Value::as_str),
        Some(machine_id(&value(&source)).as_str())
    );
    assert!(record.body.get("configuration_before").is_some());
    assert!(record.body.get("configuration_after").is_some());

    let instance = &store.state.instances["inst-1"];
    assert_eq!(instance.configuration.sequential_leaf(), Some("intake"));
    assert_eq!(instance.ctx["score"].canonical_string(), "4");
    assert_eq!(
        store.state.instance_machines["inst-1"],
        machine_id(&value(&source)),
        "the instance is on the new definition now"
    );
    // And the history names the record that moved it.
    let history = store.history_page("inst-1", 0, 50, false, true).unwrap();
    let kinds: Vec<String> = history
        .get("entries")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("kind").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    assert!(
        kinds.iter().any(|kind| kind == "InstanceMigrated"),
        "{kinds:?}"
    );
}

#[test]
fn the_target_must_declare_it_supersedes_this_instances_machine() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    // A third machine that supersedes nothing.
    let unrelated = r#"{"format":"fsm.machine/1","name":"unrelated","states":[{"name":"intake"}],"initial":"intake","context":[],"events":[],"transitions":[]}"#;
    store
        .define_machine_on(
            &mut FixedClock::new(2_000, 1),
            value(unrelated),
            false,
            false,
        )
        .unwrap();
    let before = store.state.instances["inst-1"].clone();
    let error = store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "unrelated",
            "mig-1",
        )
        .expect_err("no supersede link");
    assert_eq!(error.code, "req/migrate_not_superseded");
    assert_eq!(store.state.instances["inst-1"], before, "nothing moved");
}

#[test]
fn a_refused_migration_is_journaled_and_replays() {
    let directory = TestDirectory::create();
    // A mapping that does not cover `awaiting_countersign`.
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    store
        .send_event(
            "inst-1",
            "stall",
            Value::Obj(BTreeMap::new()),
            "stall-1",
            None,
        )
        .unwrap();
    let before = store.records.len();
    let error = store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .expect_err("the leaf is unmapped");
    assert_eq!(error.code, "req/migrate_unmapped");
    assert_eq!(
        store.records.len(),
        before + 1,
        "an attempt somebody made is in the trail"
    );
    let rejection = store.records.last().unwrap();
    assert_eq!(rejection.kind, RecordKind::RequestRejected);
    assert_eq!(
        rejection.body.get("operation").and_then(Value::as_str),
        Some("migrate")
    );
    // The key is claimed, so the retry replays the same refusal.
    let again = store
        .migrate_instance_on(
            &mut FixedClock::new(8_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .expect_err("the refusal replays");
    assert_eq!(again.code, "req/migrate_unmapped");

    // A settled instance refuses the same way.
    store.cancel_instance("inst-1", "cancel-1").unwrap();
    let error = store
        .migrate_instance_on(
            &mut FixedClock::new(9_000, 1),
            "inst-1",
            "review_v2",
            "mig-2",
        )
        .expect_err("settled");
    assert_eq!(error.code, "req/migrate_settled");
}

#[test]
fn a_retry_replays_and_a_different_target_is_refused() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    let first = store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    let records = store.records.len();
    let again = store
        .migrate_instance_on(
            &mut FixedClock::new(8_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    assert_eq!(again.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(store.records.len(), records);
    assert_eq!(again.get("seq"), first.get("seq"));

    // The same key aimed elsewhere is a different request.
    let error = store
        .migrate_instance_on(
            &mut FixedClock::new(9_000, 1),
            "inst-1",
            "review_v1",
            "mig-1",
        )
        .expect_err("a different target under the same key");
    assert_eq!(error.code, "req/request_id_conflict");
}

#[test]
fn a_retry_replays_from_the_journal_after_a_restart() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    let first = store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    let records = store.records.len();
    drop(store);

    let mut reopened = Store::open(directory.path()).unwrap();
    let replayed = reopened
        .migrate_instance_on(
            &mut FixedClock::new(8_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    assert_eq!(reopened.records.len(), records, "nothing was written");
    assert_eq!(
        replayed.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    for field in ["instance_id", "from_machine_id", "to_machine_id", "seq"] {
        assert_eq!(replayed.get(field), first.get(field), "{field}");
    }
}

#[test]
fn the_record_says_what_the_preview_predicted_at_the_same_moment() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();

    let from = store.state.machines[&machine_id(&value(OLD))].clone();
    let to = store.state.machines[&machine_id(&value(&source))].clone();
    let state = store.state.instances["inst-1"].clone();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let predicted = preview(
        &from.compiled,
        &to.compiled,
        &to.tree,
        &state,
        7_000,
        &mut budget,
    );

    store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    let record = migrated_record(&store);
    assert_eq!(
        record.body.get("rescheduled_deadlines"),
        Some(&fsm_core::migrate::apply::rescheduled_value(
            &predicted.report.rescheduled_deadlines
        )),
        "the record says what the preview said"
    );
    // The pending effect from the old machine survived, and can still be acked.
    assert_eq!(store.state.instances["inst-1"].pending.len(), 1);
    let effect_id = store.state.instances["inst-1"].pending[0].clone();
    store
        .ack_effect_outcome(&effect_id, "inst-1", "ack-1", "ok", None)
        .or_else(|_| store.ack_effect_outcome("inst-1", &effect_id, "ack-1", "ok", None))
        .expect("an ack against a carried effect still resolves");
}

#[test]
fn a_reacting_migration_records_its_microsteps_and_a_quiet_one_does_not() {
    let directory = TestDirectory::create();
    let reacting = new_with(
        MAPPED,
        r#",{"name":"settled"}"#,
        r#",{"from":"triage","to":"settled"}"#,
    );
    let mut store = ready(&directory, &reacting);
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    let record = migrated_record(&store);
    assert!(record.body.get("microsteps").is_some());
    assert_eq!(
        store.state.instances["inst-1"]
            .configuration
            .sequential_leaf(),
        Some("settled"),
        "configuration_after is the post-reaction one"
    );

    // A migration with no reaction writes no key at all.
    let directory = TestDirectory::create();
    let quiet = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &quiet);
    store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    assert!(migrated_record(&store).body.get("microsteps").is_none());
}

#[test]
fn the_journal_folds_to_the_same_state() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let mut store = ready(&directory, &source);
    store
        .migrate_instance_on(
            &mut FixedClock::new(7_000, 1),
            "inst-1",
            "review_v2",
            "mig-1",
        )
        .unwrap();
    let expected = store.state.instances.clone();
    let machines = store.state.instance_machines.clone();
    let records = store.records.clone();
    drop(store);
    let folded = fold_with(records, &mut NopSink).expect("the journal folds");
    assert_eq!(folded.instances, expected);
    assert_eq!(
        folded.instance_machines, machines,
        "replay follows the instance onto its new definition"
    );
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.state.instances, expected);
}

#[test]
fn a_read_only_store_refuses_to_migrate() {
    let directory = TestDirectory::create();
    let source = new_with(MAPPED, "", "");
    let store = ready(&directory, &source);
    drop(store);
    let mut reader = Store::open_read_only(directory.path()).unwrap();
    let error = reader
        .migrate_instance("inst-1", "review_v2", "mig-1")
        .expect_err("read-only");
    assert_eq!(error.code, "io/write");
}
