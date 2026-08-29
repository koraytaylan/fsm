//! The `fsm.base/1` authoritative base state: its bytes, both its roots, and
//! the rule that every failure is a refusal rather than a fallback.
//!
//! Plan 0017 task 7902. Regenerate the golden with `REGEN_BASE=1`.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::replay::{RequestSlot, StoreState, StoredMachine, state_root_at};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;
use fsm_store::base::{
    BASE_FORMAT, BaseRoots, DefinitionLimits, base_roots, decode, dedup_fingerprint_root, encode,
    read,
};
use fsm_store::snapshot::store_states_eq;

const MACHINE_ID: &str =
    "timed_parallel@sha256:d8921831c274663db25974be4c7bb4c9fcd6590a0357ed1bcd81d68384568811";

/// A base state built from a committed machine definition, with one running
/// instance and two claimed keys — one fingerprinted, one not.
fn base_state() -> StoreState {
    let root_material = parse(
        include_bytes!("../../fsm-core/tests/fixtures/hashes/state_root_v3_parallel.json"),
        &JsonLimits::DEFAULT,
    )
    .expect("state-root golden parses");
    let definition = root_material
        .get("machines")
        .and_then(Value::as_obj)
        .and_then(|machines| machines.get(MACHINE_ID))
        .expect("golden machine")
        .clone();
    let compiled = compile_accepted(&definition).expect("golden machine compiles");
    let tree = Tree::for_machine(&compiled.spec);
    let instance = InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([
                ("audit".into(), "checking".into()),
                ("work".into(), "waiting".into()),
            ]),
        },
        ctx: BTreeMap::from([
            ("approved".into(), Val::Bool(true)),
            ("attempts".into(), Val::Int(-2)),
        ]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::from([("expire".into(), 1_200)]),
        pending: vec!["effect-z".into(), "effect-a".into()],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    StoreState {
        machines: BTreeMap::from([(
            MACHINE_ID.into(),
            StoredMachine {
                def: definition,
                compiled,
                tree,
            },
        )]),
        instances: BTreeMap::from([("case-1".into(), instance)]),
        instance_machines: BTreeMap::from([("case-1".into(), MACHINE_ID.into())]),
        dedup: BTreeMap::from([
            (
                "carried-with-fingerprint".into(),
                RequestSlot {
                    seq: 6,
                    fp: Some(format!("sha256:{}", "b".repeat(64))),
                },
            ),
            // A key claimed before fingerprints existed: it replays but cannot
            // be conflict-checked, and it is omitted from the fingerprint root.
            (
                "carried-without-fingerprint".into(),
                RequestSlot { seq: 4, fp: None },
            ),
        ]),
        last_seq: 40_000,
        last_hash: "a".repeat(64),
    }
}

fn encoded() -> Value {
    encode(&base_state(), DefinitionLimits::Current)
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/base_v1.json")
}

/// Replace one value inside the encoded base, leaving everything else alone.
fn with_field(mut object: BTreeMap<String, Value>, key: &str, value: Value) -> Value {
    object.insert(key.into(), value);
    Value::Obj(object)
}

fn object_of(value: &Value) -> BTreeMap<String, Value> {
    value.as_obj().expect("base is an object").clone()
}

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

#[test]
fn a_base_round_trips_to_an_equal_state() {
    let state = base_state();
    let restored = decode(&encoded(), &base_roots(&state)).expect("the base decodes");
    assert!(
        store_states_eq(&state, &restored),
        "a base did not round-trip to an equal state"
    );
}

#[test]
fn the_committed_golden_is_byte_exact() {
    let fixture = fixture_path();
    if std::env::var("REGEN_BASE").ok().as_deref() == Some("1") {
        let mut bytes = canon_bytes(&encoded());
        bytes.push(b'\n');
        std::fs::write(&fixture, bytes).expect("the golden is writable");
    }
    let committed = std::fs::read(&fixture).expect("the base golden is committed");
    let expected = committed
        .strip_suffix(b"\n")
        .expect("text fixture ends in one LF");
    let parsed = parse(expected, &JsonLimits::DEFAULT).expect("the golden parses");
    assert_eq!(canon_bytes(&parsed), expected, "the fixture is canonical");
    assert_eq!(
        canon_bytes(&encoded()),
        expected,
        "base bytes, both roots, and every embedded state hash are fixed"
    );
    assert_eq!(
        parsed.get("format").and_then(Value::as_str),
        Some(BASE_FORMAT)
    );
}

#[test]
fn encoding_is_deterministic() {
    assert_eq!(canon_bytes(&encoded()), canon_bytes(&encoded()));
}

#[test]
fn one_altered_context_byte_fails_on_the_state_root() {
    // The altered base is *internally* consistent — its own per-instance state
    // hash and its own declared roots agree with its contents — so the only
    // thing left to catch it is the root the seal committed. That is the real
    // threat: a base from the same store at a different instant, which every
    // layer below the root would happily accept.
    let expected = base_roots(&base_state());
    let mut altered = base_state();
    altered
        .instances
        .get_mut("case-1")
        .expect("the instance")
        .ctx
        .insert("attempts".into(), Val::Int(-3));
    let error = decode(&encode(&altered, DefinitionLimits::Current), &expected)
        .expect_err("a base one context byte away from the seal is refused");
    assert_eq!(error.code, "store/base_mismatch");
    assert!(
        error.message.contains("base_state_root"),
        "the message does not name the root that failed: {}",
        error.message
    );
}

#[test]
fn an_instance_whose_state_hash_disagrees_with_its_own_state_is_refused() {
    // The layer below the root: a base edited by hand, where the context moved
    // and the per-instance hash did not.
    let expected = base_roots(&base_state());
    let mut object = object_of(&encoded());
    let mut instances = object
        .get("instances")
        .and_then(Value::as_obj)
        .expect("instances")
        .clone();
    let mut instance = instances
        .get("case-1")
        .and_then(Value::as_obj)
        .expect("the instance")
        .clone();
    let mut context = instance
        .get("context")
        .and_then(Value::as_obj)
        .expect("the context")
        .clone();
    context.insert("attempts".into(), Value::Str("-3".into()));
    instance.insert("context".into(), Value::Obj(context));
    instances.insert("case-1".into(), Value::Obj(instance));
    object.insert("instances".into(), Value::Obj(instances));

    let error = decode(&Value::Obj(object), &expected).expect_err("an edited base is refused");
    assert_eq!(error.code, "io/read");
    assert!(
        error.message.contains("state_hash"),
        "the message does not name what failed: {}",
        error.message
    );
}

#[test]
fn one_altered_fingerprint_fails_on_the_dedup_root() {
    // This is the entire reason the second root exists. `state_root_at` covers
    // the claiming sequence and not the fingerprint, so a suite that only
    // altered an instance would pass an implementation that omitted this root.
    let state = base_state();
    let expected = base_roots(&state);
    let mut object = object_of(&encoded());
    let mut dedup = object
        .get("dedup")
        .and_then(Value::as_obj)
        .expect("dedup")
        .clone();
    let mut entry = dedup
        .get("carried-with-fingerprint")
        .and_then(Value::as_obj)
        .expect("the fingerprinted entry")
        .clone();
    entry.insert(
        "fp".into(),
        Value::Str(format!("sha256:{}", "c".repeat(64))),
    );
    dedup.insert("carried-with-fingerprint".into(), Value::Obj(entry));
    object.insert("dedup".into(), Value::Obj(dedup));

    let error = decode(&Value::Obj(object), &expected)
        .expect_err("a base with an altered fingerprint is refused");
    assert_eq!(error.code, "store/base_mismatch");
    assert!(
        error.message.contains("base_dedup_fp_root"),
        "the message does not name the root that failed: {}",
        error.message
    );
}

#[test]
fn an_entry_without_a_fingerprint_is_omitted_rather_than_hashed_as_null() {
    let with_both = base_state();
    let mut only_fingerprinted = with_both.clone();
    only_fingerprinted
        .dedup
        .remove("carried-without-fingerprint");
    assert_eq!(
        dedup_fingerprint_root(&with_both),
        dedup_fingerprint_root(&only_fingerprinted),
        "an entry with no fp changed the fingerprint root"
    );

    let mut none_fingerprinted = with_both.clone();
    for slot in none_fingerprinted.dedup.values_mut() {
        slot.fp = None;
    }
    let empty = dedup_fingerprint_root(&none_fingerprinted);
    let mut no_keys = with_both.clone();
    no_keys.dedup.clear();
    assert_eq!(
        empty,
        dedup_fingerprint_root(&no_keys),
        "a base whose entries all lack fingerprints must still produce a stable root"
    );
    assert!(empty.starts_with("sha256:"));
}

#[test]
fn the_state_root_is_the_core_function_and_not_a_reimplementation() {
    let state = base_state();
    assert_eq!(
        base_roots(&state).state_root,
        state_root_at(&state, state.last_seq),
        "the base computed a state root a divergent private implementation produced"
    );
}

#[test]
fn the_base_root_differs_from_the_checkpoint_root_when_a_key_was_dropped() {
    // Same function, same sequence, different state. `state_root_at` covers the
    // dedup table; the base's table has the dropped entries removed while the
    // checkpoint record's covers the table as it stood. These two are NOT equal
    // and a later reader must not "fix" them into agreement — this assertion is
    // what stops that.
    let at_the_cut = base_state();
    let mut carried_only = at_the_cut.clone();
    carried_only.dedup.remove("carried-without-fingerprint");
    assert_ne!(
        state_root_at(&at_the_cut, at_the_cut.last_seq),
        state_root_at(&carried_only, carried_only.last_seq),
        "dropping a dedup entry left the state root unchanged"
    );
}

#[test]
fn a_base_declaring_the_wrong_format_is_refused() {
    let expected = base_roots(&base_state());
    for (key, wrong) in [
        ("format", "fsm.base/2"),
        ("state_root_format", "fsm.state-root/2"),
        ("base_dedup_format", "fsm.base-dedup/2"),
        ("definition_limits", "whatever"),
    ] {
        let altered = with_field(object_of(&encoded()), key, Value::Str(wrong.into()));
        let error = decode(&altered, &expected)
            .err()
            .unwrap_or_else(|| panic!("a base declaring `{key}: {wrong}` was accepted"));
        assert_eq!(error.code, "io/read", "wrong code for {key}");
    }
}

#[test]
fn a_base_whose_roots_disagree_with_the_seal_is_refused() {
    // The base's own roots are consistent; it is the seal that names different
    // ones. That is a base from another store, and it must never be served.
    let another_store = BaseRoots {
        state_root: format!("sha256:{}", "d".repeat(64)),
        dedup_fp_root: format!("sha256:{}", "e".repeat(64)),
    };
    let error = decode(&encoded(), &another_store).expect_err("a foreign base is refused");
    assert_eq!(error.code, "store/base_mismatch");
    assert!(
        error.hint.contains("no repair"),
        "the hint must say plainly that nothing reconstructs a base: {}",
        error.hint
    );
}

#[test]
fn a_base_whose_instance_is_invalid_for_its_machine_is_refused() {
    let expected = base_roots(&base_state());
    let mut object = object_of(&encoded());
    let mut instances = object
        .get("instances")
        .and_then(Value::as_obj)
        .expect("instances")
        .clone();
    let mut instance = instances
        .get("case-1")
        .and_then(Value::as_obj)
        .expect("the instance")
        .clone();
    // A leaf the machine does not have: the tree's own validation must catch it
    // rather than a root comparison, so the failure names the real fault.
    instance.insert(
        "configuration".into(),
        Value::Obj(BTreeMap::from([
            ("kind".into(), Value::Str("sequential".into())),
            ("leaf".into(), Value::Str("no_such_state".into())),
        ])),
    );
    instances.insert("case-1".into(), Value::Obj(instance));
    object.insert("instances".into(), Value::Obj(instances));

    let error = decode(&Value::Obj(object), &expected).expect_err("an invalid instance is refused");
    assert_eq!(error.code, "io/read");
}

#[test]
fn a_base_naming_a_machine_it_does_not_carry_is_refused() {
    let expected = base_roots(&base_state());
    let mut object = object_of(&encoded());
    object.insert("machines".into(), Value::Obj(BTreeMap::new()));
    let error = decode(&Value::Obj(object), &expected).expect_err("a dangling instance is refused");
    assert_eq!(error.code, "io/read");
}

#[test]
fn a_base_over_the_persistence_cap_is_refused_without_being_read_whole() {
    let directory = std::env::temp_dir().join(format!("fsm-base-cap-{}", invocation_tag()));
    std::fs::create_dir_all(&directory).expect("the temporary directory is creatable");
    let path = directory.join("BASE");
    // One byte past the cap every persistence unit obeys.
    let oversized = vec![b'x'; JsonLimits::DEFAULT.max_bytes + 1];
    std::fs::write(&path, &oversized).expect("the oversized base is writable");
    let error = read(&path, &base_roots(&base_state())).expect_err("an oversized base is refused");
    assert_eq!(error.code, "io/read");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_missing_base_is_an_io_read_refusal_rather_than_a_panic() {
    let missing = std::env::temp_dir().join(format!("fsm-base-absent-{}/BASE", invocation_tag()));
    let error = read(&missing, &base_roots(&base_state())).expect_err("an absent base is refused");
    assert_eq!(error.code, "io/read");
}

#[test]
fn a_well_formed_base_reads_back_from_disk() {
    let directory = std::env::temp_dir().join(format!("fsm-base-read-{}", invocation_tag()));
    std::fs::create_dir_all(&directory).expect("the temporary directory is creatable");
    let path = directory.join("BASE");
    std::fs::write(&path, canon_bytes(&encoded())).expect("the base is writable");
    let state = base_state();
    let restored = read(&path, &base_roots(&state)).expect("a well-formed base reads back");
    assert!(store_states_eq(&state, &restored));
    let _ = std::fs::remove_dir_all(&directory);
}
