//! One instance's records legitimately span two definitions, and replay
//! follows it across the boundary the migration record names.
//!
//! The journal a migration produces can only be built by the store's own
//! operations, so this suite lives beside them rather than in the pure
//! crate: `fsm-core` cannot depend on `fsm-store`.
//!
//! Plan 0011 task 5502.

use std::collections::BTreeMap;

use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordKind, seal, zeros};
use fsm_core::replay::{NopSink, StoreState, fold_with, state_root_at};

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

/// The definition an instance starts on: it declares an event the new one
/// drops, and an effect whose name a pending id must still re-derive.
const OLD: &str = r#"{"format":"fsm.machine/1","name":"m_v1","states":[{"name":"intake"},{"name":"triage"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"1"}],"events":[{"name":"go","fields":[]},{"name":"legacy_only","fields":[]}],"effects":[{"name":"notify","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"triage","emit":[{"effect":"notify"}]},{"from":"triage","on":"legacy_only","to":"intake"}]}"#;

/// The corrected definition: it declares an event the old one never had, with
/// a typed field, so a post-migration payload proves which machine validated
/// it.
fn new_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m_v2","states":[{{"name":"intake"}},{{"name":"triage"}}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}},{{"name":"countersign","fields":[{{"name":"who","ty":"str"}}]}}],"effects":[{{"name":"notify","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}},{{"from":"triage","on":"countersign","to":"intake"}}],"supersedes":{{"machine":"{}","states":{{"intake":"intake","triage":"triage"}},"context":{{"score":"ctx.score + 1"}}}}}}"#,
        digest(OLD)
    )
}

use fsm_store::clock::FixedClock;
use fsm_store::store::Store;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-migrep-{}-{n}", std::process::id()));
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

/// Create, step, migrate, step again — the journal this task is about.
fn spanning_journal(directory: &TestDirectory) -> (Vec<Record>, StoreState) {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    let new = new_source();
    for source in [OLD.to_string(), new.clone()] {
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            "m_v1",
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    // A step under the old definition, emitting an effect.
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    store
        .migrate_instance_on(&mut FixedClock::new(7_000, 1), "inst-1", "m_v2", "mig-1")
        .unwrap();
    // And a step whose event only the *new* definition declares.
    store
        .send_event(
            "inst-1",
            "countersign",
            Value::Obj(BTreeMap::from([(
                "who".to_string(),
                Value::Str("desk".into()),
            )])),
            "cs-1",
            None,
        )
        .unwrap();
    (store.records.clone(), store.state.clone())
}

#[test]
fn a_journal_that_spans_two_definitions_folds_clean() {
    let directory = TestDirectory::create();
    let (records, expected) = spanning_journal(&directory);
    let folded = fold_with(records.clone(), &mut NopSink).expect("it folds");
    assert_eq!(folded.instances, expected.instances);
    assert_eq!(
        folded.instance_machines["inst-1"],
        machine_id(&value(&new_source())),
        "the instance is on the new definition"
    );
    // Every state hash the journal claims is reproduced.
    for record in &records {
        if let Some(claimed) = record.body.get("state_hash").and_then(Value::as_str) {
            assert!(claimed.starts_with("sha256:"), "{claimed}");
        }
    }
    // And the root matches an independent fold at the same seq.
    assert_eq!(
        state_root_at(&folded, folded.last_seq),
        state_root_at(&expected, expected.last_seq)
    );
}

#[test]
fn the_superseded_machine_stays_resolvable() {
    let directory = TestDirectory::create();
    let (records, _) = spanning_journal(&directory);
    let folded = fold_with(records, &mut NopSink).unwrap();
    let old_id = machine_id(&value(OLD));
    assert!(
        folded.machines.contains_key(&old_id),
        "pre-migration records replay against it"
    );
    // The effect emitted before the migration is still pending, and its
    // name re-derives from the machine that emitted it.
    let instance = &folded.instances["inst-1"];
    assert_eq!(instance.pending.len(), 1);
    let emitting = folded.machines[&old_id]
        .compiled
        .spec
        .effects
        .iter()
        .any(|effect| effect.name == "notify");
    assert!(emitting, "the old machine still declares the effect");
}

/// Rebuild a journal with one record's body altered, re-sealing the chain.
fn tampered(
    records: &[Record],
    seq: usize,
    edit: impl Fn(&mut BTreeMap<String, Value>),
) -> Vec<Record> {
    let mut previous = zeros();
    let mut out = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let mut body = record.body.as_obj().cloned().unwrap_or_default();
        if index == seq {
            edit(&mut body);
        }
        let sealed = seal(
            record.seq,
            record.ts,
            record.kind,
            Value::Obj(body),
            &previous,
        );
        previous = sealed.hash.clone();
        out.push(sealed);
    }
    out
}

fn migration_index(records: &[Record]) -> usize {
    records
        .iter()
        .position(|record| record.kind == RecordKind::InstanceMigrated)
        .expect("one migration")
}

#[test]
fn every_journaled_claim_is_checked() {
    let directory = TestDirectory::create();
    let (records, _) = spanning_journal(&directory);
    let at = migration_index(&records);

    let cases: Vec<(&str, Box<dyn Fn(&mut BTreeMap<String, Value>)>)> = vec![
        (
            "configuration_after",
            Box::new(|body: &mut BTreeMap<String, Value>| {
                body.insert(
                    "configuration_after".into(),
                    Value::Obj(BTreeMap::from([
                        ("kind".into(), Value::Str("sequential".into())),
                        ("leaf".into(), Value::Str("intake".into())),
                    ])),
                );
            }),
        ),
        (
            "rescheduled_deadlines",
            Box::new(|body: &mut BTreeMap<String, Value>| {
                body.insert(
                    "rescheduled_deadlines".into(),
                    Value::Arr(vec![Value::Obj(BTreeMap::from([(
                        "deadline".to_string(),
                        Value::Str("invented".into()),
                    )]))]),
                );
            }),
        ),
        (
            "dropped_history",
            Box::new(|body: &mut BTreeMap<String, Value>| {
                body.insert(
                    "dropped_history".into(),
                    Value::Arr(vec![Value::Str("box/ghost".into())]),
                );
            }),
        ),
        (
            "from_machine_id",
            Box::new(|body: &mut BTreeMap<String, Value>| {
                body.insert(
                    "from_machine_id".into(),
                    Value::Str(machine_id(&value(&new_source()))),
                );
            }),
        ),
        (
            "state_hash",
            Box::new(|body: &mut BTreeMap<String, Value>| {
                body.insert(
                    "state_hash".into(),
                    Value::Str(format!("sha256:{}", "f".repeat(64))),
                );
            }),
        ),
    ];
    for (field, edit) in cases {
        let altered = tampered(&records, at, edit);
        assert!(
            fold_with(altered, &mut NopSink).is_err(),
            "altering {field} was not caught"
        );
    }
}

#[test]
fn a_microsteps_key_is_checked_in_both_directions() {
    // This migration reacts, so its record carries microsteps; deleting
    // the key must fail as surely as altering it.
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    let reacting = new_source().replace(
        r#"{"from":"intake","on":"go","to":"triage"}"#,
        r#"{"from":"intake","on":"go","to":"triage"},{"from":"triage","to":"intake"}"#,
    );
    for source in [OLD.to_string(), reacting] {
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            "m_v1",
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    store
        .migrate_instance_on(&mut FixedClock::new(7_000, 1), "inst-1", "m_v2", "mig-1")
        .unwrap();
    let records = store.records.clone();
    let at = migration_index(&records);
    assert!(
        records[at].body.get("microsteps").is_some(),
        "the migration reacted"
    );
    let without = tampered(&records, at, |body| {
        body.remove("microsteps");
    });
    assert!(
        fold_with(without, &mut NopSink).is_err(),
        "a reaction the record does not mention is a reaction nobody can audit"
    );
}

#[test]
fn a_post_migration_payload_is_validated_by_the_new_definition() {
    let directory = TestDirectory::create();
    let (records, state) = spanning_journal(&directory);
    // `countersign` exists only in the new machine, and it applied.
    assert!(
        records
            .iter()
            .any(|record| record.body.get("event").and_then(Value::as_str) == Some("countersign")),
        "the post-migration event is in the journal"
    );
    assert_eq!(
        state.instances["inst-1"].configuration.sequential_leaf(),
        Some("intake")
    );
    // And the pre-migration event that the new machine dropped still
    // replays, because it was applied against the machine that had it.
    let folded = fold_with(records, &mut NopSink).unwrap();
    assert_eq!(folded.instances, state.instances);
}

#[test]
fn a_view_reports_the_definition_the_instance_is_actually_on() {
    let directory = TestDirectory::create();
    let (_, _) = spanning_journal(&directory);
    let store = Store::open(directory.path()).unwrap();
    let view = store.instance_view("inst-1", None, None).unwrap();
    assert_eq!(
        view.get("machine")
            .and_then(|machine| machine.get("name"))
            .and_then(Value::as_str),
        Some("m_v2")
    );
    let events: Vec<&str> = view
        .get("enabled_events")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("event").and_then(Value::as_str))
        .collect();
    assert!(events.contains(&"go"), "{events:?}");
    assert!(
        !events.contains(&"legacy_only"),
        "the old definition's events are gone: {events:?}"
    );
}

#[test]
fn a_migration_from_a_machine_the_instance_was_not_on_fails_the_fold() {
    let directory = TestDirectory::create();
    let (records, _) = spanning_journal(&directory);
    let at = migration_index(&records);
    let altered = tampered(&records, at, |body| {
        body.insert(
            "from_machine_id".into(),
            Value::Str("m_ghost@sha256:".to_string() + &"a".repeat(64)),
        );
    });
    assert!(
        fold_with(altered, &mut NopSink).is_err(),
        "a record claiming to migrate from a machine the instance was not on is corruption"
    );
}
