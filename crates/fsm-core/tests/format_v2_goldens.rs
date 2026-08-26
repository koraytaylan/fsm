//! Byte-exact goldens for the current genesis, state, root, and deadline-record formats.
//!
//! Expected material is checked in as literal canonical JSON. The tests never
//! ask the production encoders to manufacture the expected bytes.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::Val;
use fsm_core::hashes::{STATE_DOMAIN, domain_hash, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::{RecordKind, limits_value, seal, verify_line, zeros};
use fsm_core::replay::{RequestSlot, STATE_ROOT_DOMAIN, StoreState, StoredMachine, state_root_at};
use fsm_core::sha256::to_hex;
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

const MACHINE_ID: &str =
    "timed_parallel@sha256:d8921831c274663db25974be4c7bb4c9fcd6590a0357ed1bcd81d68384568811";
const STATE_HASH: &str = "sha256:8615dd42f12d300f9d35b94ea72ec53f75f63384be90845f7429e2447302b66b";
const STATE_ROOT: &str = "sha256:a2d9cfb45da21178d7845657a9cd71ca909f18f350930f2764ac97aa2ad78244";
const REQUEST_FINGERPRINT: &str =
    "sha256:7194902dadb0f474169b8e18095be3ac13526dc751c99d552ee14dfef56da6b1";
const GENESIS_HASH: &str = "9135442a38b0e05ca3ce5839cab077a16624be990ce1e6fa0d1856fe9abe24e6";

fn canonical_fixture(bytes: &[u8]) -> Value {
    let expected = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let value = parse(expected, &JsonLimits::DEFAULT).expect("golden fixture parses");
    assert_eq!(canon_bytes(&value), expected, "fixture itself is canonical");
    value
}

fn parallel_state() -> InstanceState {
    InstanceState {
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
        // `fsm.state/2` sorts these before hashing. The root and snapshot keep
        // the durable vector order, so the deliberately reversed input pins
        // both representations.
        pending: vec!["effect-z".into(), "effect-a".into()],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn store_state(root_material: &Value) -> StoreState {
    let machines = root_material
        .get("machines")
        .and_then(Value::as_obj)
        .expect("root fixture machines");
    let definition = machines.get(MACHINE_ID).expect("golden machine").clone();
    let compiled = compile_accepted(&definition).expect("golden machine compiles");
    assert_eq!(compiled.machine_id, MACHINE_ID);
    let tree = Tree::for_machine(&compiled.spec);

    StoreState {
        machines: BTreeMap::from([(
            MACHINE_ID.into(),
            StoredMachine {
                def: definition,
                compiled,
                tree,
            },
        )]),
        instances: BTreeMap::from([("case-1".into(), parallel_state())]),
        instance_machines: BTreeMap::from([("case-1".into(), MACHINE_ID.into())]),
        dedup: BTreeMap::from([(
            "poll-early".into(),
            RequestSlot {
                seq: 6,
                fp: Some(format!("sha256:{}", "b".repeat(64))),
            },
        )]),
        last_seq: 7,
        last_hash: "a".repeat(64),
    }
}

#[test]
fn genesis_limits_and_envelope_hash_match_literal_jsonl() {
    let body = Value::Obj(BTreeMap::from([
        ("created_ts".into(), Value::Num("0".into())),
        ("format".into(), Value::Str("fsm.journal/1".into())),
        ("limits".into(), limits_value()),
    ]));
    let genesis = seal(0, 0, RecordKind::Genesis, body, &zeros());

    assert_eq!(genesis.hash, GENESIS_HASH, "the envelope hash is fixed");
    assert_eq!(
        genesis.to_line(),
        include_bytes!("fixtures/records/genesis_current_limits.jsonl"),
        "the current definition ceilings and genesis envelope are byte-exact"
    );

    let verified = verify_line(
        include_bytes!("fixtures/records/genesis_current_limits.jsonl"),
        0,
        &zeros(),
    )
    .expect("golden genesis verifies");
    assert_eq!(verified.hash, GENESIS_HASH);
}

#[test]
fn state_v2_and_state_root_v3_match_literal_canonical_material() {
    let state_material =
        canonical_fixture(include_bytes!("fixtures/hashes/state_v2_parallel.json"));
    assert_eq!(
        format!(
            "sha256:{}",
            to_hex(&domain_hash(STATE_DOMAIN, &state_material))
        ),
        STATE_HASH,
        "the checked-in state material has the independently fixed digest"
    );
    assert_eq!(
        state_hash(MACHINE_ID, "case-1", 7, &parallel_state()),
        STATE_HASH,
        "the production state encoder commits the checked-in material"
    );

    let root_material = canonical_fixture(include_bytes!(
        "fixtures/hashes/state_root_v3_parallel.json"
    ));
    assert_eq!(
        format!(
            "sha256:{}",
            to_hex(&domain_hash(STATE_ROOT_DOMAIN, &root_material))
        ),
        STATE_ROOT,
        "the checked-in root material has the independently fixed digest"
    );
    assert_eq!(
        state_root_at(&store_state(&root_material), 7),
        STATE_ROOT,
        "the production root encoder commits the checked-in material"
    );
}

fn state_hash_value() -> Value {
    Value::Str(STATE_HASH.into())
}

fn request_fingerprint_value() -> Value {
    Value::Str(REQUEST_FINGERPRINT.into())
}

#[test]
fn deadline_record_bodies_and_hashes_match_literal_jsonl() {
    let applied_body = Value::Obj(BTreeMap::from([
        ("deadline".into(), Value::Str("expire".into())),
        ("deadline_idx".into(), Value::Num("0".into())),
        ("due_ms".into(), Value::Num("1200".into())),
        (
            "entered".into(),
            Value::Arr(vec![Value::Str("timed_out".into())]),
        ),
        (
            "exited".into(),
            Value::Arr(vec![Value::Str("waiting".into())]),
        ),
        ("instance_id".into(), Value::Str("case-1".into())),
        ("request_fp".into(), request_fingerprint_value()),
        ("request_id".into(), Value::Str("poll-applied".into())),
        ("source_state".into(), Value::Str("waiting".into())),
        ("state_format".into(), Value::Str("fsm.state/2".into())),
        ("state_hash".into(), state_hash_value()),
    ]));
    let applied = seal(
        8,
        1_200,
        RecordKind::DeadlineApplied,
        applied_body,
        &"1".repeat(64),
    );

    let rejected_body = Value::Obj(BTreeMap::from([
        ("code".into(), Value::Str("run/action_error".into())),
        ("deadline".into(), Value::Str("expire".into())),
        ("deadline_idx".into(), Value::Num("0".into())),
        (
            "details".into(),
            Value::Obj(BTreeMap::from([
                ("cause".into(), Value::Str("run/overflow".into())),
                ("source_state".into(), Value::Str("waiting".into())),
            ])),
        ),
        ("due_ms".into(), Value::Num("1200".into())),
        (
            "hint".into(),
            Value::Str("reduce the deadline duration or poll with a smaller base timestamp".into()),
        ),
        ("instance_id".into(), Value::Str("case-1".into())),
        (
            "message".into(),
            Value::Str("deadline expire failed: timestamp overflow".into()),
        ),
        ("request_fp".into(), request_fingerprint_value()),
        ("request_id".into(), Value::Str("poll-rejected".into())),
        ("state_format".into(), Value::Str("fsm.state/2".into())),
        ("state_hash".into(), state_hash_value()),
    ]));
    let rejected = seal(
        9,
        1_200,
        RecordKind::DeadlineRejected,
        rejected_body,
        &applied.hash,
    );

    let not_due_body = Value::Obj(BTreeMap::from([
        ("instance_id".into(), Value::Str("case-1".into())),
        ("next_deadline".into(), Value::Str("expire".into())),
        ("next_deadline_idx".into(), Value::Num("0".into())),
        ("next_due_ms".into(), Value::Num("1200".into())),
        ("request_fp".into(), request_fingerprint_value()),
        ("request_id".into(), Value::Str("poll-early".into())),
        ("state_format".into(), Value::Str("fsm.state/2".into())),
        ("state_hash".into(), state_hash_value()),
    ]));
    let not_due = seal(
        10,
        1_199,
        RecordKind::DeadlineNotDue,
        not_due_body,
        &rejected.hash,
    );

    let records = [applied, rejected, not_due];
    let actual: Vec<u8> = records.iter().flat_map(|record| record.to_line()).collect();
    assert_eq!(
        actual,
        include_bytes!("fixtures/records/deadlines_v8.jsonl"),
        "deadline envelopes, bodies, and record hashes are byte-exact"
    );

    let mut expected_prev = "1".repeat(64);
    for (expected_seq, line) in actual.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let record = verify_line(line, expected_seq as u64 + 8, &expected_prev)
            .expect("golden record verifies");
        expected_prev = record.hash;
    }
}
