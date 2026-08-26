//! An operator learns their mapping is wrong when they write it, not when
//! they try to move a live workflow with it.
//!
//! Every check here needs both definitions in hand, so it runs at
//! `define_machine` — before a single instance is at risk. What these answer
//! is "is this mapping coherent"; whether a particular instance can move is a
//! run-time question with its own codes.
//!
//! Plan 0011 task 5302.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-migadm-{}-{n}", std::process::id()));
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

/// The definition an instance is on today: two leaves and a decimal.
const OLD: &str = r#"{"format":"fsm.machine/1","name":"review_v1","states":[{"name":"intake"},{"name":"triage"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"0"},{"name":"total","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"triage"}]}"#;

/// The corrected definition, with whatever mapping the case under test needs.
fn new_with(states: &str, context: &str, extra_states: &str, extra_ctx: &str) -> String {
    let old = digest(OLD);
    format!(
        r#"{{"format":"fsm.machine/1","name":"review_v2","states":[{{"name":"intake"}},{{"name":"triage"}}{extra_states}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}},{{"name":"total","ty":{{"decimal":"2"}},"init":"0.00"}}{extra_ctx}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}],"supersedes":{{"machine":"{old}","states":{states},"context":{context}}}}}"#
    )
}

/// Define `OLD`, then attempt the candidate; return the refusal's code.
fn refusal(candidate: &str) -> String {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(OLD), false, false)
        .unwrap();
    let before = store.records.len();
    let Err(error) = store.define_machine_on(&mut clock, value(candidate), false, false) else {
        panic!("expected a refusal for {candidate}");
    };
    assert_eq!(
        store.records.len(),
        before,
        "a refused definition writes no record"
    );
    error.code
}

#[test]
fn a_well_formed_pair_defines_cleanly() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(OLD), false, false)
        .unwrap();
    let candidate = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score + 1"}"#,
        "",
        "",
    );
    store
        .define_machine_on(&mut clock, value(&candidate), false, false)
        .unwrap();
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::MachineDefined)
            .count(),
        2
    );
}

#[test]
fn a_superseded_machine_the_store_does_not_hold_is_refused() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    let before = store.records.len();
    let candidate = new_with(r#"{}"#, r#"{}"#, "", "");
    let Err(error) = store.define_machine_on(&mut clock, value(&candidate), false, false) else {
        panic!("a mapping nobody can check is refused");
    };
    assert_eq!(error.code, "def/supersedes_unknown_machine");
    assert_eq!(store.records.len(), before, "and nothing is written");
}

#[test]
fn every_state_mapping_rule_reports_its_own_code() {
    // An old state that does not exist, and a new one that does not.
    assert_eq!(
        refusal(&new_with(r#"{"ghost":"triage"}"#, "{}", "", "")),
        "def/supersedes_unknown_state"
    );
    assert_eq!(
        refusal(&new_with(r#"{"intake":"ghost"}"#, "{}", "", "")),
        "def/supersedes_unknown_state"
    );
    // A compound and a history pseudostate are not leaves.
    let compound = r#",{"name":"box","initial":"inner","states":[{"name":"inner"},{"name":"h","history":"shallow"}]}"#;
    assert_eq!(
        refusal(&new_with(r#"{"intake":"box"}"#, "{}", compound, "")),
        "def/supersedes_target_not_leaf"
    );
    assert_eq!(
        refusal(&new_with(r#"{"intake":"h"}"#, "{}", compound, "")),
        "def/supersedes_target_not_leaf"
    );
    // A terminal state and a final state each end something.
    assert_eq!(
        refusal(&new_with(
            r#"{"intake":"done"}"#,
            "{}",
            r#",{"name":"done","terminal":true}"#,
            ""
        )),
        "def/supersedes_target_terminal"
    );
    let with_final = r#",{"name":"box","initial":"inner","states":[{"name":"inner"},{"name":"finished","final":true}]}"#;
    assert_eq!(
        refusal(&new_with(r#"{"intake":"finished"}"#, "{}", with_final, "")),
        "def/supersedes_target_terminal"
    );
}

#[test]
fn region_topology_is_not_mappable() {
    let parallel = format!(
        r#"{{"format":"fsm.machine/1","name":"review_v2","regions":[{{"name":"left","states":[{{"name":"a"}}],"initial":"a"}},{{"name":"right","states":[{{"name":"b"}}],"initial":"b"}}],"context":[],"events":[],"transitions":[],"supersedes":{{"machine":"{}","states":{{}},"context":{{}}}}}}"#,
        digest(OLD)
    );
    assert_eq!(refusal(&parallel), "def/supersedes_region");

    // And two parallel machines whose region names differ.
    let old_parallel = r#"{"format":"fsm.machine/1","name":"par_v1","regions":[{"name":"left","states":[{"name":"a"}],"initial":"a"},{"name":"right","states":[{"name":"b"}],"initial":"b"}],"context":[],"events":[],"transitions":[]}"#;
    let renamed = format!(
        r#"{{"format":"fsm.machine/1","name":"par_v2","regions":[{{"name":"left","states":[{{"name":"a"}}],"initial":"a"}},{{"name":"other","states":[{{"name":"b"}}],"initial":"b"}}],"context":[],"events":[],"transitions":[],"supersedes":{{"machine":"{}","states":{{}},"context":{{}}}}}}"#,
        digest(old_parallel)
    );
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(old_parallel), false, false)
        .unwrap();
    let Err(error) = store.define_machine_on(&mut clock, value(&renamed), false, false) else {
        panic!("a renamed region is a different shape");
    };
    assert_eq!(error.code, "def/supersedes_region");
}

#[test]
fn a_context_mapping_is_typed_against_both_definitions() {
    // A variable the new machine does not declare.
    assert_eq!(
        refusal(&new_with("{}", r#"{"ghost":"ctx.score"}"#, "", "")),
        "def/supersedes_ctx_unknown"
    );
    // An expression reading a variable the old machine does not declare.
    assert_eq!(
        refusal(&new_with("{}", r#"{"score":"ctx.ghost"}"#, "", "")),
        "def/supersedes_ctx_unknown"
    );
    // A decimal scale mismatch is a type mismatch: scale is part of the type.
    assert_eq!(
        refusal(&new_with(
            "{}",
            r#"{"precise":"ctx.total"}"#,
            "",
            r#",{"name":"precise","ty":{"decimal":"4"},"init":"0.0000"}"#
        )),
        "def/supersedes_ctx_type"
    );
    // And an int where a decimal is declared.
    assert_eq!(
        refusal(&new_with("{}", r#"{"total":"ctx.score"}"#, "", "")),
        "def/supersedes_ctx_type"
    );
}

#[test]
fn a_slot_the_new_machine_lacks_is_refused() {
    let child = r#"{"format":"fsm.machine/1","name":"child","states":[{"name":"w"},{"name":"d","terminal":true}],"initial":"w","context":[],"events":[{"name":"f","fields":[]}],"transitions":[{"from":"w","on":"f","to":"d"}]}"#;
    let old_with_slot = format!(
        r#"{{"format":"fsm.machine/1","name":"slot_v1","states":[{{"name":"intake","invoke":[{{"id":"check","machine":"{}"}}]}},{{"name":"out"}}],"initial":"intake","context":[],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"out"}}]}}"#,
        digest(child)
    );
    let without = format!(
        r#"{{"format":"fsm.machine/1","name":"slot_v2","states":[{{"name":"intake"}},{{"name":"out"}}],"initial":"intake","context":[],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"out"}}],"supersedes":{{"machine":"{}","states":{{}},"context":{{}}}}}}"#,
        digest(&old_with_slot)
    );
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for spec in [child.to_string(), old_with_slot.clone()] {
        store
            .define_machine_on(&mut clock, value(&spec), false, false)
            .unwrap();
    }
    let Err(error) = store.define_machine_on(&mut clock, value(&without), false, false) else {
        panic!("a dropped slot is work with nowhere to go");
    };
    assert_eq!(error.code, "def/supersedes_slot");

    // Declaring it again is accepted.
    let with_slot = without.replace(
        r#"{"name":"intake"}"#,
        &format!(
            r#"{{"name":"intake","invoke":[{{"id":"check","machine":"{}"}}]}}"#,
            digest(child)
        ),
    );
    store
        .define_machine_on(&mut clock, value(&with_slot), false, false)
        .unwrap();
}

#[test]
fn findings_are_stable_and_a_machine_without_the_block_is_unaffected() {
    let candidate = new_with(
        r#"{"ghost":"ghost2"}"#,
        r#"{"ghost3":"ctx.ghost4"}"#,
        "",
        "",
    );
    let first = refusal(&candidate);
    let second = refusal(&candidate);
    assert_eq!(
        first, second,
        "the same mapping reports the same first code"
    );

    // A definition with no `supersedes` takes exactly the path it always did.
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(OLD), false, false)
        .unwrap();
    assert_eq!(store.records.len(), 2, "genesis and the definition");
    assert!(
        fsm_core::migrate::validate::validate_supersedes(
            &fsm_core::spec::compile_accepted(&value(OLD)).unwrap(),
            &fsm_core::tree::Tree::for_machine(
                &fsm_core::spec::compile_accepted(&value(OLD)).unwrap().spec
            ),
            &fsm_core::spec::compile_accepted(&value(OLD)).unwrap(),
            &fsm_core::tree::Tree::for_machine(
                &fsm_core::spec::compile_accepted(&value(OLD)).unwrap().spec
            ),
        )
        .is_empty(),
        "no block, no findings"
    );
    let _: BTreeMap<String, Value> = BTreeMap::new();
}
