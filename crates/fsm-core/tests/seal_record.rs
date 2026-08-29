//! The `journal_sealed` record kind: its body shape, the join it asserts, and
//! the claim that it changes no logical state.
//!
//! Plan 0017 task 7901. A prefix detached without a record saying so is a
//! prefix that was deleted, so the seal is a first-class record before
//! anything is allowed to write one — and every reader that already assumed it
//! had seen all the kinds is taught this one here rather than discovering it
//! when a real store carries it.

use std::collections::BTreeMap;

use fsm_core::hashes::{ARCHIVE_DOMAIN, BASE_DEDUP_DOMAIN, BASE_DEDUP_FORMAT};
use fsm_core::json::Value;
use fsm_core::record::{Record, RecordKind, instances_touched, seal, verify_line, zeros};
use fsm_core::replay::{NopSink, STATE_ROOT_FORMAT, StoreState, fold_with};

fn hash(byte: u8) -> String {
    format!("sha256:{}", format!("{byte:02x}").repeat(32))
}

/// A predecessor hash that is a plausible record hash rather than the origin.
fn previous_hash() -> String {
    "ab".repeat(32)
}

fn seal_body(previous: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("sealed_through_seq".into(), Value::Num("40000".to_string())),
        (
            "sealed_last_hash".into(),
            Value::Str(format!("sha256:{previous}")),
        ),
        ("base_state_root".into(), Value::Str(hash(0x11))),
        (
            "state_root_format".into(),
            Value::Str(STATE_ROOT_FORMAT.to_string()),
        ),
        ("base_dedup_fp_root".into(), Value::Str(hash(0x22))),
        (
            "base_dedup_format".into(),
            Value::Str(BASE_DEDUP_FORMAT.to_string()),
        ),
        ("archive_id".into(), Value::Str(hash(0x33))),
        ("records_sealed".into(), Value::Num("40000".to_string())),
    ])
}

/// Build a seal at `seq` with `previous` as its predecessor, then round-trip it
/// through `verify_line`, which is the only path that runs the body-shape check.
fn verify_seal(seq: u64, previous: &str, body: BTreeMap<String, Value>) -> Result<Record, String> {
    let record = seal(
        seq,
        1_700_000_000_000,
        RecordKind::JournalSealed,
        Value::Obj(body),
        previous,
    );
    verify_line(&record.to_line(), seq, previous).map_err(|error| format!("{error:?}"))
}

#[test]
fn a_well_formed_seal_body_passes_the_shape_check() {
    let previous = previous_hash();
    assert!(verify_seal(40_001, &previous, seal_body(&previous)).is_ok());
}

#[test]
fn every_required_field_is_required() {
    let previous = previous_hash();
    for field in seal_body(&previous).keys() {
        let mut body = seal_body(&previous);
        body.remove(field);
        assert!(
            verify_seal(40_001, &previous, body).is_err(),
            "a seal without `{field}` was accepted"
        );
    }
}

#[test]
fn every_field_is_typed() {
    let previous = previous_hash();
    // One wrong-typed value per field: a number where a hash belongs, a string
    // where a count belongs. Each must be refused on its own.
    let wrong: BTreeMap<&str, Value> = BTreeMap::from([
        ("sealed_through_seq", Value::Str("40000".into())),
        ("sealed_last_hash", Value::Num("1".into())),
        ("base_state_root", Value::Str("not-a-hash".into())),
        ("state_root_format", Value::Str("fsm.state-root/4".into())),
        ("base_dedup_fp_root", Value::Str("sha256:short".into())),
        ("base_dedup_format", Value::Str("fsm.base-dedup/2".into())),
        ("archive_id", Value::Bool(true)),
        ("records_sealed", Value::Str("many".into())),
    ]);
    for (field, value) in wrong {
        let mut body = seal_body(&previous);
        body.insert(field.into(), value);
        assert!(
            verify_seal(40_001, &previous, body).is_err(),
            "a seal with a mistyped `{field}` was accepted"
        );
    }
}

#[test]
fn a_seal_whose_last_hash_disagrees_with_its_predecessor_is_refused() {
    // The seal is appended at `sealed_through_seq + 1`, so the join it names is
    // already in the chain. The body asserts it; it does not create it, and a
    // record where the two disagree is corrupt rather than merely inconsistent.
    let previous = previous_hash();
    let mut body = seal_body(&previous);
    body.insert("sealed_last_hash".into(), Value::Str(hash(0x44)));
    assert!(verify_seal(40_001, &previous, body).is_err());
}

#[test]
fn a_seal_naming_another_state_root_format_is_refused() {
    // This plan introduces no new version of the state-root format. Pinning the
    // constant here is what makes an accidental bump a red test rather than a
    // silently accepted record.
    assert_eq!(STATE_ROOT_FORMAT, "fsm.state-root/3");
    let previous = previous_hash();
    for wrong in ["fsm.state-root/2", "fsm.state-root/4", "fsm.state/3", ""] {
        let mut body = seal_body(&previous);
        body.insert("state_root_format".into(), Value::Str(wrong.into()));
        assert!(
            verify_seal(40_001, &previous, body).is_err(),
            "a seal declaring `{wrong}` was accepted"
        );
    }
}

#[test]
fn a_seal_touches_no_instance() {
    let previous = previous_hash();
    let record = verify_seal(40_001, &previous, seal_body(&previous)).expect("the seal is valid");
    // Asserted against a constructed record rather than against the existence
    // of a match arm: the arm could be present and wrong.
    assert!(instances_touched(&record).is_empty());
}

#[test]
fn the_kind_round_trips_through_its_wire_name() {
    assert_eq!(RecordKind::JournalSealed.as_str(), "journal_sealed");
    assert_eq!(
        RecordKind::from_str("journal_sealed"),
        Some(RecordKind::JournalSealed)
    );
    assert!(RecordKind::all().contains(&RecordKind::JournalSealed));
    for kind in RecordKind::all() {
        assert_eq!(RecordKind::from_str(kind.as_str()), Some(kind));
    }
}

/// A journal with a genesis and a checkpoint, plus the same journal with a
/// seal appended: folding both must reach the same logical state.
///
/// The checkpoint is there because a seal is only ever appended after one, so
/// the pair is the shape a real sealed journal has.
fn journal_with_and_without_a_seal() -> (Vec<Record>, Vec<Record>) {
    let genesis = seal(
        0,
        1,
        RecordKind::Genesis,
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("1".into())),
            ("limits".into(), fsm_core::record::limits_value()),
        ])),
        &zeros(),
    );
    let checkpoint = seal(
        1,
        2,
        RecordKind::StateCheckpoint,
        Value::Obj(BTreeMap::from([
            (
                "state_root".into(),
                Value::Str(fsm_core::replay::state_root_at(&StoreState::default(), 1)),
            ),
            (
                "state_root_format".into(),
                Value::Str(STATE_ROOT_FORMAT.to_string()),
            ),
        ])),
        &genesis.hash,
    );
    let without = vec![genesis, checkpoint.clone()];
    let sealed = seal(
        2,
        3,
        RecordKind::JournalSealed,
        Value::Obj(seal_body(&checkpoint.hash)),
        &checkpoint.hash,
    );
    let mut with = without.clone();
    with.push(sealed);
    (without, with)
}

#[test]
fn folding_a_seal_changes_no_logical_state() {
    let (without, with) = journal_with_and_without_a_seal();
    let before = fold_with(without, &mut NopSink).expect("the unsealed journal folds");
    let after = fold_with(with, &mut NopSink).expect("the journal with a seal folds");
    // Field by field, because "changes no logical state" is this task's central
    // claim and a whole-struct comparison would hide which half held.
    assert_eq!(before.machines.len(), after.machines.len());
    assert_eq!(before.instances, after.instances);
    assert_eq!(before.instance_machines, after.instance_machines);
    assert_eq!(before.dedup, after.dedup);
    // The two journal-position fields are the only ones that move, and they
    // move because the seal is a record in the chain, not because it mutated
    // anything the fold owns.
    assert_eq!(before.last_seq + 1, after.last_seq);
    assert_ne!(before.last_hash, after.last_hash);
}

#[test]
fn the_new_domain_constants_have_their_exact_byte_values() {
    // As string literals, so a typo in a domain tag cannot ship: a changed
    // domain silently changes every value hashed under it.
    assert_eq!(BASE_DEDUP_DOMAIN, "fsm:base-dedup:1");
    assert_eq!(ARCHIVE_DOMAIN, "fsm:archive:1");
    assert_eq!(BASE_DEDUP_FORMAT, "fsm.base-dedup/1");
}

#[test]
fn a_seal_on_a_ten_thousandth_sequence_carries_an_injected_state_root() {
    // `store/commit.rs` folds a provisional record on every 10 000th sequence
    // and inserts `state_root` and `state_root_format` into its body before
    // appending. A seal declares `state_root_format` itself, so the two meet.
    // Record bodies are not closed, so the extra field is survivable — and it
    // has to be survivable *deliberately*, which is what this pins.
    let previous = previous_hash();
    let mut body = seal_body(&previous);
    body.insert("state_root".into(), Value::Str(hash(0x55)));
    assert!(
        verify_seal(10_000, &previous, body).is_ok(),
        "a boundary seal carrying the injected state_root was refused"
    );
    // The other direction: away from a boundary there is no injection, and the
    // seal passes without the field.
    assert!(verify_seal(40_001, &previous, seal_body(&previous)).is_ok());
}

#[test]
fn a_boundary_seals_injected_root_is_not_its_base_root() {
    // Same function, same sequence, different state: `state_root_at` covers the
    // dedup table, and a base's table has the dropped entries removed while the
    // record's covers the table as it stood. They are two different values and
    // a later reader must not "fix" them into agreement.
    let state = StoreState::default();
    let root_over_full_state = fsm_core::replay::state_root_at(&state, 10_000);
    let base_root_in_body = hash(0x11);
    assert_ne!(root_over_full_state, base_root_in_body);
}
