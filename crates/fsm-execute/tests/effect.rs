//! An effect id is opaque; a handler table is keyed by effect *name*. Every
//! row here is about turning the first into the second — across all three
//! record kinds that can emit, from a store nobody holds a lock on.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::ctx_val_string;
use fsm_execute::effect::{PendingEffect, resolve};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-execute-{test_name}-{}-{sequence}",
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

fn definition(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("machine definition parses")
}

/// Two emits on entering `review`, so one transition produces `k=0` and `k=1`.
fn transition_emit_machine() -> Value {
    definition(
        r#"{
            "format":"fsm.machine/1",
            "name":"case_review_effects",
            "context":[
                {"name":"case_id","ty":"str","init":"case-0"},
                {"name":"amount","ty":{"decimal":"2"},"init":"19.50"},
                {"name":"line_count","ty":"int","init":"3"}
            ],
            "events":[{"name":"submit","fields":[]}],
            "effects":[
                {"name":"assign_reviewer","fields":[
                    {"name":"case","ty":"str"},
                    {"name":"lines","ty":"int"},
                    {"name":"total","ty":{"decimal":"2"}}
                ]},
                {"name":"notify_reviewer","fields":[{"name":"case","ty":"str"}]}
            ],
            "states":[
                {"name":"intake"},
                {"name":"review","entry":{"emit":[
                    {"effect":"assign_reviewer","args":{
                        "case":"ctx.case_id","lines":"ctx.line_count","total":"ctx.amount"
                    }},
                    {"effect":"notify_reviewer","args":{"case":"ctx.case_id"}}
                ]}}
            ],
            "initial":"intake",
            "transitions":[{"from":"intake","on":"submit","to":"review"}]
        }"#,
    )
}

/// An entry block on the *initial* state, so creation itself emits.
fn creation_emit_machine() -> Value {
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
    )
}

/// A deadline transition that emits when it fires.
fn deadline_emit_machine() -> Value {
    definition(
        r#"{
            "format":"fsm.machine/1",
            "name":"case_review_deadline",
            "context":[{"name":"case_id","ty":"str","init":"case-0"}],
            "events":[{"name":"approve","fields":[]}],
            "effects":[{"name":"escalate_case","fields":[{"name":"case","ty":"str"}]}],
            "states":[
                {"name":"awaiting_review"},
                {"name":"approved","terminal":true},
                {"name":"expired","terminal":true}
            ],
            "initial":"awaiting_review",
            "transitions":[{"from":"awaiting_review","on":"approve","to":"approved"}],
            "deadlines":[{
                "name":"review_timeout",
                "from":"awaiting_review",
                "after":"dur(30, s)",
                "to":"expired",
                "emit":[{"effect":"escalate_case","args":{"case":"ctx.case_id"}}]
            }]
        }"#,
    )
}

fn overrides(pairs: &[(&str, Val)]) -> BTreeMap<String, Val> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

/// Drive a writer into the state each test needs, then hand back a read-only
/// handle — every resolution in this suite runs without the writer lock.
fn writer(directory: &TestDirectory) -> (Store, FixedClock) {
    (
        Store::open(directory.path()).expect("open writer"),
        FixedClock::new(1_000, 1),
    )
}

fn read_only(directory: &TestDirectory) -> Store {
    Store::open_read_only(directory.path()).expect("open read-only")
}

fn pending_ids(store: &Store, instance_id: &str) -> Vec<String> {
    store.state.instances[instance_id].pending.clone()
}

fn argument(effect: &PendingEffect, name: &str) -> String {
    ctx_val_string(
        effect
            .args
            .get(name)
            .unwrap_or_else(|| panic!("{name} is missing from {:?}", effect.args)),
    )
}

#[test]
fn a_transition_emit_resolves_to_its_name_and_evaluated_args() {
    let directory = TestDirectory::create("effect-transition");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, transition_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "case-1",
            "req-create",
            None,
            &overrides(&[("case_id", Val::Str("case-4711".into()))]),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "case-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    let ids = pending_ids(&store, "case-1");
    drop(store);

    let store = read_only(&directory);
    assert_eq!(ids.len(), 2, "two emits in one entry block");
    let assigned = resolve(&store, &ids[0]).unwrap();
    assert_eq!(assigned.effect_name, "assign_reviewer");
    assert_eq!(assigned.instance_id, "case-1");
    assert_eq!(assigned.k, 0);
    assert_eq!(argument(&assigned, "case"), "case-4711");
    assert_eq!(argument(&assigned, "lines"), "3");
    assert_eq!(argument(&assigned, "total"), "19.50");

    let notified = resolve(&store, &ids[1]).unwrap();
    assert_eq!(notified.effect_name, "notify_reviewer");
    assert_eq!(notified.k, 1);
    assert_eq!(argument(&notified, "case"), "case-4711");
    assert_eq!(notified.args.len(), 1);
}

#[test]
fn a_creation_time_emit_resolves_from_the_instance_created_record() {
    let directory = TestDirectory::create("effect-creation");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, creation_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_intake_effects",
            "case-2",
            "req-create",
            None,
            &overrides(&[("case_id", Val::Str("case-77".into()))]),
            &[],
        )
        .unwrap();
    let ids = pending_ids(&store, "case-2");
    drop(store);

    let store = read_only(&directory);
    assert_eq!(ids, ["case-2/0/0"], "a creation emit carries a zero seq");
    let opened = resolve(&store, &ids[0]).unwrap();
    assert_eq!(opened.effect_name, "open_case");
    assert_eq!(opened.emitted_seq, 0);
    assert_eq!(
        argument(&opened, "case"),
        "case-77",
        "the creation override is honoured by the replay"
    );
}

#[test]
fn a_re_created_instance_resolves_against_its_latest_creation() {
    // Creation is not guarded against re-using an instance id, and both
    // creations compose the same `{instance}/0/{k}` effect id. Resolving to
    // the first creation's arguments would run the handler against values the
    // instance no longer holds.
    let directory = TestDirectory::create("effect-recreated");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, creation_emit_machine(), false, false)
        .unwrap();
    for (request, case) in [
        ("req-create-1", "case-first"),
        ("req-create-2", "case-second"),
    ] {
        store
            .create_instance_ctx_on(
                &mut clock,
                "case_intake_effects",
                "case-2",
                request,
                None,
                &overrides(&[("case_id", Val::Str(case.into()))]),
                &[],
            )
            .unwrap();
    }
    let ids = pending_ids(&store, "case-2");
    drop(store);

    let store = read_only(&directory);
    let opened = resolve(&store, &ids[0]).unwrap();
    assert_eq!(argument(&opened, "case"), "case-second");
}

#[test]
fn a_deadline_emit_resolves_from_the_deadline_applied_record() {
    let directory = TestDirectory::create("effect-deadline");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, deadline_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_deadline",
            "case-3",
            "req-create",
            None,
            &overrides(&[("case_id", Val::Str("case-9".into()))]),
            &[],
        )
        .unwrap();
    let mut later = FixedClock::new(100_000, 1);
    store
        .poll_instance_deadline_on(&mut later, "case-3", "req-poll", None)
        .unwrap();
    let ids = pending_ids(&store, "case-3");
    drop(store);

    let store = read_only(&directory);
    assert_eq!(ids.len(), 1);
    let escalated = resolve(&store, &ids[0]).unwrap();
    assert_eq!(escalated.effect_name, "escalate_case");
    assert_eq!(argument(&escalated, "case"), "case-9");
}

#[test]
fn an_emit_index_no_transition_produced_is_unresolved() {
    let directory = TestDirectory::create("effect-missing-k");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, transition_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "case-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "case-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    let emitted_seq = store.journal.last_seq;
    drop(store);

    let store = read_only(&directory);
    let error = resolve(&store, &format!("case-1/{emitted_seq}/9")).unwrap_err();
    assert_eq!(error.code, "exec/effect_unresolved");
    assert!(error.message.contains("k=9"), "{error:?}");
}

#[test]
fn a_malformed_or_unknown_id_is_an_error_never_a_panic() {
    let directory = TestDirectory::create("effect-rejects");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, transition_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "case-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "case-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    let emitted_seq = store.journal.last_seq;
    drop(store);

    let store = read_only(&directory);
    for id in [
        "",
        "no-slashes",
        "case-1/notanumber/0",
        "case-1/3/notanumber",
        "/3/0",
        // Genesis emits nothing, and a naive `seq - 1` prefix fold would
        // underflow here; a zero seq means "this instance's creation", and
        // this machine emits nothing at creation.
        "case-1/0/0",
        // Past the end of the journal.
        "case-1/9999/0",
    ] {
        let error = resolve(&store, id).unwrap_err();
        assert_eq!(error.code, "exec/effect_unresolved", "{id}");
    }

    // A real record, but named with an instance that did not write it.
    let error = resolve(&store, &format!("case-other/{emitted_seq}/0")).unwrap_err();
    assert_eq!(error.code, "exec/effect_unresolved");
    assert!(error.message.contains("another instance"), "{error:?}");
}

#[test]
fn a_record_that_emits_nothing_is_unresolved() {
    let directory = TestDirectory::create("effect-wrong-kind");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, creation_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_intake_effects",
            "case-2",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "case-2",
            "close",
            &mut Value::Obj(BTreeMap::new()),
            "req-close",
            None,
            &[],
        )
        .unwrap();
    let closed_seq = store.journal.last_seq;
    store
        .cancel_instance_reason_on(&mut clock, "case-2", "req-cancel", "reviewer withdrew")
        .unwrap();
    let cancelled_seq = store.journal.last_seq;
    drop(store);

    let store = read_only(&directory);
    // A transition that applied but emitted nothing.
    let no_emit = resolve(&store, &format!("case-2/{closed_seq}/0")).unwrap_err();
    assert_eq!(no_emit.code, "exec/effect_unresolved");
    assert!(no_emit.message.contains("k=0"), "{no_emit:?}");

    // A record kind that cannot emit at all.
    let wrong_kind = resolve(&store, &format!("case-2/{cancelled_seq}/0")).unwrap_err();
    assert_eq!(wrong_kind.code, "exec/effect_unresolved");
    assert!(
        wrong_kind.message.contains("instance_cancelled"),
        "{wrong_kind:?}"
    );
}

#[test]
fn resolution_is_deterministic_across_calls_and_opens() {
    let directory = TestDirectory::create("effect-determinism");
    let (mut store, mut clock) = writer(&directory);
    store
        .define_machine_on(&mut clock, transition_emit_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "case_review_effects",
            "case-1",
            "req-create",
            None,
            &overrides(&[("case_id", Val::Str("case-4711".into()))]),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "case-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    let ids = pending_ids(&store, "case-1");
    drop(store);

    let first_open = read_only(&directory);
    let once = resolve(&first_open, &ids[0]).unwrap();
    let twice = resolve(&first_open, &ids[0]).unwrap();
    drop(first_open);
    let second_open = read_only(&directory);
    let after_reopen = resolve(&second_open, &ids[0]).unwrap();
    assert_eq!(once, twice);
    assert_eq!(once, after_reopen);
}
