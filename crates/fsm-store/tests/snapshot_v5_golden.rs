//! Byte-exact golden for a parallel `fsm.snapshot/5` with an active deadline.
//!
//! Regenerate with `REGEN_SNAPSHOT=1`. A snapshot is a disposable cache, so a
//! version bump replaces this golden rather than migrating it — an older
//! snapshot beside a current journal is skipped and the journal folded.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::replay::{RequestSlot, StoreState, StoredMachine};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;
use fsm_store::snapshot::{snapshot_to_state, state_to_snapshot};

const MACHINE_ID: &str =
    "timed_parallel@sha256:d8921831c274663db25974be4c7bb4c9fcd6590a0357ed1bcd81d68384568811";

fn state_from_literal_machine() -> StoreState {
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
    assert_eq!(compiled.machine_id, MACHINE_ID);
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
fn snapshot_v5_matches_literal_canonical_bytes_and_round_trips() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/snapshot_v5_parallel.json");
    if std::env::var("REGEN_SNAPSHOT").ok().as_deref() == Some("1") {
        let mut bytes = canon_bytes(&state_to_snapshot(&state_from_literal_machine()));
        bytes.push(b'\n');
        std::fs::write(&fixture, bytes).unwrap();
    }
    let expected_with_lf = std::fs::read(&fixture).expect("the snapshot golden is committed");
    let expected_with_lf = expected_with_lf.as_slice();
    let expected = expected_with_lf
        .strip_suffix(b"\n")
        .expect("text fixture ends in one LF");
    let expected_value = parse(expected, &JsonLimits::DEFAULT).expect("snapshot golden parses");
    assert_eq!(
        canon_bytes(&expected_value),
        expected,
        "fixture itself is canonical"
    );

    let state = state_from_literal_machine();
    let actual = state_to_snapshot(&state);
    assert_eq!(
        canon_bytes(&actual),
        expected,
        "snapshot bytes and embedded state/root/snapshot hashes are fixed"
    );

    let restored = snapshot_to_state(&expected_value).expect("golden snapshot verifies");
    assert_eq!(restored.last_seq, state.last_seq);
    assert_eq!(restored.last_hash, state.last_hash);
    assert_eq!(restored.instances, state.instances);
    assert_eq!(restored.instance_machines, state.instance_machines);
    assert_eq!(restored.dedup, state.dedup);
}
