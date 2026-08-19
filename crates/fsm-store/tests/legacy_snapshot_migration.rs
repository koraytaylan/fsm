//! Snapshot compatibility for definitions admitted before the aggregate
//! expression-evaluation ceiling and strict history shapes were introduced.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordKind, limits_value, seal, zeros};
use fsm_core::replay::{STATE_ROOT_FORMAT, StoreState, StoredMachine};
use fsm_core::spec::{accepted_identity, compile_accepted, compile_accepted_historical_unchecked};
use fsm_core::tree::Tree;
use fsm_store::clock::FixedClock;
use fsm_store::journal_io::STORE_VERSION;
use fsm_store::snapshot::{materialize_state_root_at, snap_dir, state_to_snapshot};
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-store-legacy-snapshot-{}-{sequence}",
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

fn oversized_legacy_definition() -> Value {
    let conjunction = (0..16).map(|_| "true").collect::<Vec<_>>().join(" and ");
    let guard = format!("not ({conjunction})");
    let events = (0..128)
        .map(|index| format!(r#"{{"name":"e{index}","fields":[]}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let transitions = (0..128)
        .map(|index| {
            format!(r#"{{"from":"waiting","on":"e{index}","if":"{guard}","to":"waiting"}}"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"{{"format":"fsm.machine/1","name":"legacy_eval","context":[],"events":[{events}],"states":[{{"name":"waiting"}}],"initial":"waiting","transitions":[{transitions}],"invariants":[{{"name":"extra","expr":"true","mode":"monitor"}}]}}"#
    );
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("legacy definition is valid JSON")
}

fn malformed_legacy_history_definition() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"legacy_history_shapes",
            "states":[
                {"name":"start"},
                {"name":"top_history","history":"deep"},
                {"name":"child_box","initial":"child_start","states":[
                    {"name":"child_start"},
                    {"name":"child_history","history":"deep","initial":"buried","states":[{"name":"buried"}]}
                ]},
                {"name":"terminal_box","initial":"terminal_start","states":[
                    {"name":"terminal_start"},
                    {"name":"terminal_history","history":"shallow","terminal":true}
                ]},
                {"name":"initial_box","initial":"initial_start","states":[
                    {"name":"initial_start"},
                    {"name":"initial_history","history":"deep","initial":"missing"}
                ]}
            ],
            "initial":"start",
            "context":[],
            "events":[],
            "transitions":[]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("legacy malformed-history definition is valid JSON")
}

fn current_parallel_deadline_definition() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"current_parallel_deadline",
            "context":[],
            "events":[],
            "regions":[
                {
                    "name":"review",
                    "initial":"reviewing",
                    "states":[{"name":"reviewing"},{"name":"review_done"}]
                },
                {
                    "name":"audit",
                    "initial":"auditing",
                    "states":[{"name":"auditing"},{"name":"audit_done"}]
                }
            ],
            "transitions":[],
            "deadlines":[{
                "name":"review_timeout",
                "from":"reviewing",
                "after":"dur(10, ms)",
                "to":"review_done"
            }]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("current parallel deadline definition is valid JSON")
}

fn genesis(historical: bool) -> Record {
    let mut limits = limits_value();
    if historical {
        let Value::Obj(object) = &mut limits else {
            unreachable!("limits are an object")
        };
        object.remove("max_regions");
        object.remove("max_deadlines");
        object.remove("max_eval_ticks");
    }
    seal(
        0,
        0,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), limits),
        ])),
        &zeros(),
    )
}

fn machine_record(genesis: &Record, definition: &Value) -> (Record, String) {
    let machine_id = accepted_identity(definition).1;
    let record = seal(
        1,
        1,
        RecordKind::MachineDefined,
        Value::Obj(BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition.clone()),
        ])),
        &genesis.hash,
    );
    (record, machine_id)
}

fn write_store(directory: &Path, version: &str, records: &[Record]) {
    let journal = directory.join("journal");
    fs::create_dir(&journal).expect("create journal directory");
    let mut bytes = Vec::new();
    for record in records {
        bytes.extend(record.to_line());
    }
    fs::write(journal.join("seg-00000000000000000000.jsonl"), bytes)
        .expect("write journal segment");
    fs::write(directory.join("VERSION"), format!("{version}\n")).expect("write VERSION");
}

fn assert_current_limit_rejects(definition: &Value) {
    let findings = compile_accepted(definition).expect_err("definition exceeds current limit");
    assert_eq!(
        findings
            .iter()
            .filter(|finding| finding.code == "def/limit_eval")
            .count(),
        1
    );
}

#[test]
fn version_seven_legacy_definition_survives_checkpoint_snapshot_and_fast_reopen() {
    let directory = TestDirectory::create();
    let definition = oversized_legacy_definition();
    assert_current_limit_rejects(&definition);

    let genesis = genesis(true);
    let (defined, machine_id) = machine_record(&genesis, &definition);
    write_store(directory.path(), "7", &[genesis, defined]);

    let mut migrated = Store::open(directory.path()).expect("migrate VERSION7 journal");
    assert_eq!(
        fs::read_to_string(directory.path().join("VERSION"))
            .expect("read migrated VERSION")
            .trim(),
        STORE_VERSION
    );
    assert!(migrated.state.machines.contains_key(&machine_id));
    assert!(!migrated.opened_from_snapshot);

    let records_before_retry = migrated.records.len();
    let existing = migrated
        .define_machine_on(&mut FixedClock::new(2, 1), definition.clone(), false, false)
        .expect("an identical grandfathered definition remains idempotent");
    assert!(!existing.created);
    assert_eq!(existing.machine_id, machine_id);
    assert_eq!(migrated.records.len(), records_before_retry);

    let mut distinct_definition = definition.clone();
    let Value::Obj(definition_object) = &mut distinct_definition else {
        panic!("definition object")
    };
    definition_object.insert("name".into(), Value::Str("legacy_eval_new".into()));
    let error = match migrated.define_machine_on(
        &mut FixedClock::new(2, 1),
        distinct_definition,
        false,
        false,
    ) {
        Ok(_) => panic!("a new oversized definition must use current admission"),
        Err(error) => error,
    };
    assert_eq!(error.code, "def/limit_eval");

    migrated
        .shutdown_snapshot_on(&mut FixedClock::new(3, 1))
        .expect("checkpoint and snapshot grandfathered definition");
    let snapshot_seq = migrated.state.last_seq;
    assert_eq!(snapshot_seq, 2);
    drop(migrated);

    let reopened = Store::open(directory.path()).expect("reopen from bound snapshot");
    assert!(reopened.opened_from_snapshot);
    assert_eq!(reopened.opened_snapshot_seq, Some(snapshot_seq));
    assert_eq!(reopened.replayed_records, 0);
    assert!(reopened.state.machines.contains_key(&machine_id));
}

#[test]
fn version_seven_history_shapes_survive_migration_snapshot_and_fast_reopen() {
    let directory = TestDirectory::create();
    let definition = malformed_legacy_history_definition();
    assert!(
        compile_accepted(&definition)
            .expect_err("current admission rejects the historical history shapes")
            .iter()
            .any(|finding| finding.code == "def/shape")
    );

    let genesis = genesis(true);
    let (defined, machine_id) = machine_record(&genesis, &definition);
    write_store(directory.path(), "7", &[genesis, defined]);

    let mut migrated = Store::open(directory.path()).expect("migrate VERSION7 history machine");
    assert_eq!(
        fs::read_to_string(directory.path().join("VERSION"))
            .expect("read migrated VERSION")
            .trim(),
        STORE_VERSION
    );
    assert!(migrated.state.machines.contains_key(&machine_id));

    let records_before_retry = migrated.records.len();
    let existing = migrated
        .define_machine_on(&mut FixedClock::new(2, 1), definition, false, false)
        .expect("an identical grandfathered definition remains idempotent");
    assert!(!existing.created);
    assert_eq!(migrated.records.len(), records_before_retry);

    migrated
        .shutdown_snapshot_on(&mut FixedClock::new(3, 1))
        .expect("checkpoint and snapshot grandfathered history definition");
    let snapshot_seq = migrated.state.last_seq;
    drop(migrated);

    let reopened = Store::open(directory.path()).expect("reopen history machine from snapshot");
    assert!(reopened.opened_from_snapshot);
    assert_eq!(reopened.opened_snapshot_seq, Some(snapshot_seq));
    assert_eq!(reopened.replayed_records, 0);
    assert!(reopened.state.machines.contains_key(&machine_id));
}

#[test]
fn historical_genesis_full_fold_accepts_a_current_parallel_deadline_tail() {
    let directory = TestDirectory::create();
    let genesis = genesis(true);
    write_store(directory.path(), "7", &[genesis]);

    let definition = current_parallel_deadline_definition();
    compile_accepted(&definition).expect("definition is valid under current admission");
    compile_accepted_historical_unchecked(&definition)
        .expect("historical full-fold mode retains current structural admission");

    let mut migrated = Store::open(directory.path()).expect("migrate legacy-genesis store");
    let defined = migrated
        .define_machine_on(&mut FixedClock::new(10, 0), definition, false, false)
        .expect("write current parallel deadline definition after migration");
    migrated
        .create_instance_ctx_on(
            &mut FixedClock::new(100, 0),
            &defined.machine_id,
            "parallel-instance",
            "create-parallel",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("create current parallel instance");
    migrated
        .poll_instance_deadline_on(
            &mut FixedClock::new(110, 0),
            "parallel-instance",
            "poll-parallel",
            None,
        )
        .expect("apply current deadline in migrated store");
    let final_state = migrated
        .state
        .instances
        .get("parallel-instance")
        .expect("instance exists")
        .clone();
    drop(migrated);
    fs::remove_dir_all(snap_dir(directory.path()))
        .expect("discard the drop-time cache to force a complete journal fold");

    let reopened = Store::open(directory.path())
        .expect("full-fold legacy genesis with current parallel deadline tail");
    assert!(!reopened.opened_from_snapshot);
    assert_eq!(
        reopened.state.instances.get("parallel-instance"),
        Some(&final_state)
    );
}

#[test]
fn current_genesis_cannot_bind_a_snapshot_that_bypasses_the_eval_limit() {
    let directory = TestDirectory::create();
    let definition = oversized_legacy_definition();
    assert_current_limit_rejects(&definition);

    let genesis = genesis(false);
    let (defined, machine_id) = machine_record(&genesis, &definition);
    let compiled = compile_accepted_historical_unchecked(&definition)
        .expect("test fixture is valid apart from the aggregate ceiling");
    let tree = Tree::for_machine(&compiled.spec);
    let mut state = StoreState {
        last_seq: defined.seq,
        last_hash: defined.hash.clone(),
        ..StoreState::default()
    };
    state.machines.insert(
        machine_id.clone(),
        StoredMachine {
            def: definition,
            compiled,
            tree,
        },
    );

    let checkpoint_seq = 2;
    let checkpoint = seal(
        checkpoint_seq,
        2,
        RecordKind::StateCheckpoint,
        Value::Obj(BTreeMap::from([
            (
                "state_root".into(),
                Value::Str(materialize_state_root_at(&state, checkpoint_seq)),
            ),
            (
                "state_root_format".into(),
                Value::Str(STATE_ROOT_FORMAT.into()),
            ),
        ])),
        &defined.hash,
    );
    state.last_seq = checkpoint.seq;
    state.last_hash = checkpoint.hash.clone();
    write_store(
        directory.path(),
        STORE_VERSION,
        &[genesis, defined, checkpoint],
    );

    fs::create_dir(snap_dir(directory.path())).expect("create snapshot directory");
    fs::write(
        snap_dir(directory.path()).join("snap-2.json"),
        canon_bytes(&state_to_snapshot(&state)),
    )
    .expect("write self-consistent bound snapshot");

    let error = match Store::open(directory.path()) {
        Ok(_) => panic!("current genesis must not grandfather an oversized definition"),
        Err(error) => error,
    };
    assert_eq!(error.code, "store/state_hash_mismatch");
}
