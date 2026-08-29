//! The executor against a sealed store.
//!
//! Plan 0017 task 8104. The executor keeps nothing in memory on purpose and
//! re-derives everything by scanning records, which makes it the component a
//! seal is most likely to break **quietly** — an effect resolved against the
//! wrong prefix runs a handler with stale arguments rather than failing.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::ctx_val_string;
use fsm_execute::dead;
use fsm_execute::effect::{PendingEffect, resolve};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

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
        let path = std::env::temp_dir().join(format!(
            "fsm-execute-sealed-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        for sub in ["store", "archive"] {
            fs::create_dir_all(path.join(sub)).expect("the directory is creatable");
        }
        Self(path)
    }

    fn store(&self) -> PathBuf {
        self.0.join("store")
    }

    fn archive(&self) -> PathBuf {
        self.0.join("archive")
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn definition(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("machine definition parses")
}

/// One emit on entering `review`, whose argument comes from a context field
/// the creation override sets — so an effect resolved against the wrong
/// prefix produces a *visibly* wrong argv rather than an error.
fn emitting_machine() -> Value {
    definition(
        r#"{
            "format":"fsm.machine/1",
            "name":"case_review_effects",
            "context":[{"name":"case_id","ty":"str","init":"case-0"}],
            "events":[{"name":"submit","fields":[]}],
            "effects":[{"name":"assign_reviewer","fields":[{"name":"case","ty":"str"}]}],
            "states":[
                {"name":"intake"},
                {"name":"review","entry":{"emit":[
                    {"effect":"assign_reviewer","args":{"case":"ctx.case_id"}}
                ]}}
            ],
            "initial":"intake",
            "transitions":[{"from":"intake","on":"submit","to":"review"}]
        }"#,
    )
}

fn overrides(pairs: &[(&str, Val)]) -> BTreeMap<String, Val> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

fn argument(effect: &PendingEffect, name: &str) -> String {
    ctx_val_string(effect.args.get(name).expect("the argument is present"))
}

/// A store holding one instance whose effect was emitted **above** the cut,
/// with everything below it settled so the seal is admissible. Returns the
/// effect id and the argv it resolved to before sealing.
fn store_with_an_effect_above_the_cut(directory: &TestDirectory) -> (String, String) {
    let mut clock = FixedClock::new(1_000, 1);
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine_on(&mut clock, emitting_machine(), false, false)
        .expect("the machine is definable");

    // An instance created and settled before the cut, so the prefix carries
    // real history a wrong fold would be missing.
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "settled",
            "create-settled",
            None,
            &overrides(&[("case_id", Val::Str("case-settled".into()))]),
            &[],
        )
        .expect("create succeeds");
    store
        .cancel_instance("settled", "cancel-settled")
        .expect("cancel succeeds");

    // The instance whose effect survives the cut.
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "live",
            "create-live",
            None,
            &overrides(&[("case_id", Val::Str("case-live".into()))]),
            &[],
        )
        .expect("create succeeds");
    // Close a segment so a boundary exists below the effect the send emits.
    store
        .journal
        .force_rotate()
        .expect("the journal rotates on demand");
    store
        .send_event(
            "live",
            "submit",
            Value::Obj(BTreeMap::new()),
            "send-live",
            None,
        )
        .expect("send succeeds");
    let effect_id = store.state.instances["live"].pending[0].clone();
    drop(store);

    let reader = Store::open_read_only(&directory.store()).expect("the store opens read-only");
    let before = argument(
        &resolve(&reader, &effect_id).expect("the effect resolves before sealing"),
        "case",
    );
    drop(reader);
    (effect_id, before)
}

fn seal(directory: &TestDirectory) -> u64 {
    let mut store = Store::open(&directory.store()).expect("the store opens");
    let report = store
        .seal_and_archive(&directory.archive(), None)
        .expect("the store seals below the pending effect");
    drop(store);
    report.sealed_through_seq
}

#[test]
fn an_effect_above_the_cut_resolves_to_the_same_argv_after_sealing() {
    // The assertion that catches a `fold_before` folding from the wrong
    // origin: byte for byte, the argv the handler would have been given.
    let directory = TestDirectory::create("argv");
    let (effect_id, before) = store_with_an_effect_above_the_cut(&directory);
    let cut = seal(&directory);
    assert!(
        !fsm_store::journal_io::chain_start(&directory.store()).is_origin(),
        "the store did not seal, so this case proves nothing"
    );

    let reader = Store::open_read_only(&directory.store()).expect("a sealed store opens read-only");
    let resolved = resolve(&reader, &effect_id).expect("the effect still resolves after sealing");
    assert_eq!(
        argument(&resolved, "case"),
        before,
        "the effect resolved to different arguments after sealing"
    );
    assert_eq!(before, "case-live");
    assert_eq!(resolved.effect_name, "assign_reviewer");
    assert!(
        resolved.emitted_seq > cut,
        "the emitting record was below the cut, which the pin should have forbidden"
    );
}

#[test]
fn a_creation_emitted_effect_above_the_cut_resolves_after_sealing() {
    let directory = TestDirectory::create("creation");
    let mut clock = FixedClock::new(1_000, 1);
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine_on(
            &mut clock,
            definition(
                r#"{
                    "format":"fsm.machine/1",
                    "name":"case_intake_effects",
                    "context":[{"name":"case_id","ty":"str","init":"case-0"}],
                    "events":[{"name":"close","fields":[]}],
                    "effects":[{"name":"open_case","fields":[{"name":"case","ty":"str"}]}],
                    "states":[
                        {"name":"intake","entry":{"emit":[
                            {"effect":"open_case","args":{"case":"ctx.case_id"}}
                        ]}},
                        {"name":"closed","terminal":true}
                    ],
                    "initial":"intake",
                    "transitions":[{"from":"intake","on":"close","to":"closed"}]
                }"#,
            ),
            false,
            false,
        )
        .expect("the machine is definable");
    // A boundary below the creation whose effect must survive.
    store
        .journal
        .force_rotate()
        .expect("the journal rotates on demand");
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_intake_effects",
            "live",
            "create-live",
            None,
            &overrides(&[("case_id", Val::Str("case-created".into()))]),
            &[],
        )
        .expect("create succeeds");
    let effect_id = store.state.instances["live"].pending[0].clone();
    drop(store);
    seal(&directory);

    let reader = Store::open_read_only(&directory.store()).expect("a sealed store opens read-only");
    let resolved = resolve(&reader, &effect_id).expect("a creation-time effect resolves");
    assert_eq!(argument(&resolved, "case"), "case-created");
    assert_eq!(resolved.emitted_seq, 0, "a creation-time id carries a zero");
}

#[test]
fn an_effect_whose_emitting_record_would_be_archived_cannot_be_created() {
    // The case `fold_before` cannot fix is the one the pin closes: a seal that
    // would archive a pending effect's emitting record is refused outright.
    let directory = TestDirectory::create("pinned");
    let mut clock = FixedClock::new(1_000, 1);
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine_on(&mut clock, emitting_machine(), false, false)
        .expect("the machine is definable");
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "live",
            "create-live",
            None,
            &overrides(&[("case_id", Val::Str("case-live".into()))]),
            &[],
        )
        .expect("create succeeds");
    store
        .send_event(
            "live",
            "submit",
            Value::Obj(BTreeMap::new()),
            "send-live",
            None,
        )
        .expect("send succeeds");
    // One segment, so there is no boundary below the pin to fall back to.
    let error = store
        .seal_and_archive(&directory.archive(), None)
        .expect_err("a seal that would archive a pending effect's record is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert_eq!(
        error.details.get("source").and_then(Value::as_str),
        Some("emitting_record")
    );
}

#[test]
fn an_attempt_record_for_a_pending_effect_cannot_be_archived() {
    // `watch.rs::attempt_state` scans every `effect_attempted` record and
    // needs all of them: losing the earliest lowers the count, an exhausted
    // effect retries again, and `exec/retries_exhausted` never fires. The pin
    // makes an archived attempt record for a pending effect impossible, and
    // this proves that rather than assuming it.
    let directory = TestDirectory::create("attempts");
    let mut clock = FixedClock::new(1_000, 1);
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine_on(&mut clock, emitting_machine(), false, false)
        .expect("the machine is definable");
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "live",
            "create-live",
            None,
            &overrides(&[("case_id", Val::Str("case-live".into()))]),
            &[],
        )
        .expect("create succeeds");
    store
        .send_event(
            "live",
            "submit",
            Value::Obj(BTreeMap::new()),
            "send-live",
            None,
        )
        .expect("send succeeds");
    let effect_id = store.state.instances["live"].pending[0].clone();
    store
        .attempt_effect_on(&mut clock, "live", &effect_id, "attempt-1", 1, None)
        .expect("the first attempt is journalable");
    // A boundary above the attempt, so a cut there would archive it.
    store
        .journal
        .force_rotate()
        .expect("the journal rotates on demand");
    store
        .annotate("live", "after-attempt", "a note above the attempt")
        .expect("the annotation succeeds");

    let error = store
        .seal_and_archive(&directory.archive(), None)
        .expect_err("a cut above an attempt record for a pending effect is refused");
    assert_eq!(error.code, "store/archive_refused");
    let pinned: u64 = error
        .details
        .get("pinned_seq")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .expect("the refusal names the record it protects");
    assert!(
        pinned <= store.state.last_seq,
        "the pin names a record the journal does not hold"
    );
}

#[test]
fn a_dead_letter_report_on_a_sealed_store_says_what_it_could_not_see() {
    let directory = TestDirectory::create("dead");
    store_with_an_effect_above_the_cut(&directory);
    let cut = seal(&directory);
    let (letters, horizon) =
        dead::report_with_horizon(&directory.store(), 0).expect("the report reads a sealed store");
    let horizon = horizon.expect("a sealed store's report names its horizon");
    assert_eq!(horizon.sealed_through_seq, cut);
    assert!(horizon.archive_id.starts_with("sha256:"));
    let rendered = format!("{:?}", horizon.to_value());
    assert!(
        rendered.contains("archive"),
        "the horizon does not say where the entries went: {rendered}"
    );
    // No effect exhausted here, so the report is empty — and visibly so.
    assert!(letters.is_empty());
}

#[test]
fn a_dead_letter_report_on_an_unsealed_store_names_no_horizon() {
    let directory = TestDirectory::create("dead-unsealed");
    store_with_an_effect_above_the_cut(&directory);
    let (_letters, horizon) = dead::report_with_horizon(&directory.store(), 0)
        .expect("the report reads an unsealed store");
    assert!(
        horizon.is_none(),
        "an unsealed report claimed a horizon it does not have"
    );
}

#[test]
fn no_sealing_concept_appears_in_the_scheduler() {
    // The scheduler is a pure function of one observation and must stay one:
    // sealing is a fact about where the observation came from, not an input to
    // the decision.
    let source = include_str!("../src/sched.rs");
    for forbidden in ["seal", "archive", "BASE", "chain_start"] {
        assert!(
            !source.contains(forbidden),
            "the scheduler mentions {forbidden}, so it now knows about the store's shape"
        );
    }
}

#[test]
fn resolution_on_an_unsealed_store_is_unchanged() {
    let directory = TestDirectory::create("unsealed");
    let (effect_id, before) = store_with_an_effect_above_the_cut(&directory);
    let reader = Store::open_read_only(&directory.store()).expect("the store opens read-only");
    assert_eq!(
        argument(
            &resolve(&reader, &effect_id).expect("the effect resolves"),
            "case"
        ),
        before
    );
}
