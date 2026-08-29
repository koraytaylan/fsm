//! The watcher is the executor's only read path. Every row here is about what
//! one scan reports from a journal nobody locked — and about what it keeps
//! reporting, since pending work is a state rather than an edge.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::ctx_val_string;
use fsm_execute::watch::{Observation, Watcher};
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

/// Emits on entering `review`, on entering `escalation`, and on entering the
/// terminal `closed` — the last is what proves the scan is not status-filtered.
fn case_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"case_review_outbox",
            "context":[{"name":"case_id","ty":"str","init":"case-0"}],
            "events":[
                {"name":"submit","fields":[]},
                {"name":"escalate","fields":[]},
                {"name":"ping","fields":[]},
                {"name":"close","fields":[]}
            ],
            "effects":[
                {"name":"assign_reviewer","fields":[{"name":"case","ty":"str"}]},
                {"name":"notify_manager","fields":[{"name":"case","ty":"str"}]},
                {"name":"archive_case","fields":[{"name":"case","ty":"str"}]}
            ],
            "states":[
                {"name":"intake"},
                {"name":"review","entry":{"emit":[
                    {"effect":"assign_reviewer","args":{"case":"ctx.case_id"}}
                ]}},
                {"name":"escalation","entry":{"emit":[
                    {"effect":"notify_manager","args":{"case":"ctx.case_id"}}
                ]}},
                {"name":"closed","terminal":true,"entry":{"emit":[
                    {"effect":"archive_case","args":{"case":"ctx.case_id"}}
                ]}}
            ],
            "initial":"intake",
            "transitions":[
                {"from":"intake","on":"submit","to":"review"},
                {"from":"review","on":"escalate","to":"escalation"},
                {"from":"review","on":"ping","emit":[
                    {"effect":"notify_manager","args":{"case":"ctx.case_id"}}
                ]},
                {"from":"review","on":"close","to":"closed"},
                {"from":"escalation","on":"close","to":"closed"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

/// Request ids are idempotency keys over content, so a second writer against
/// the same store must not restart the count — reusing `req-1` for a different
/// request is a conflict, not a fresh request.
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Writer {
    store: Store,
    clock: FixedClock,
}

impl Writer {
    fn open(directory: &TestDirectory) -> Self {
        Self {
            store: Store::open(directory.path()).expect("open writer"),
            clock: FixedClock::new(1_000, 1),
        }
    }

    fn request_id(&mut self) -> String {
        format!("req-{}", REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed))
    }

    fn define_and_create(&mut self, instance_id: &str) {
        let request = self.request_id();
        self.store
            .define_machine_on(&mut self.clock, case_machine(), false, false)
            .unwrap();
        self.store
            .create_instance_ctx_on(
                &mut self.clock,
                "case_review_outbox",
                instance_id,
                &request,
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }

    fn send(&mut self, instance_id: &str, event: &str) {
        let request = self.request_id();
        self.send_with_request(instance_id, event, &request);
    }

    fn send_with_request(&mut self, instance_id: &str, event: &str, request_id: &str) {
        self.store
            .send_event_stamp_on(
                &mut self.clock,
                instance_id,
                event,
                &mut Value::Obj(BTreeMap::new()),
                request_id,
                None,
                &[],
            )
            .unwrap();
    }

    fn ack(&mut self, instance_id: &str, effect_id: &str, request_id: &str) {
        self.store
            .ack_effect_outcome_on(
                &mut self.clock,
                instance_id,
                effect_id,
                request_id,
                "ok",
                None,
            )
            .unwrap();
    }

    fn cancel(&mut self, instance_id: &str) {
        let request = self.request_id();
        self.store
            .cancel_instance_reason_on(
                &mut self.clock,
                instance_id,
                &request,
                "operator stopped it",
            )
            .unwrap();
    }

    fn pending(&self, instance_id: &str) -> Vec<String> {
        self.store.state.instances[instance_id].pending.clone()
    }
}

/// Every effect this file's machines emit declares an advance in the tables
/// these tests stand in for, so an ack of any of them is one the executor
/// would still have work to do about.
fn advancing() -> BTreeSet<String> {
    [
        "assign_reviewer",
        "notify_manager",
        "archive_case",
        "open_case",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn scan(watcher: &mut Watcher) -> Observation {
    watcher.scan(10_000).expect("scan succeeds")
}

#[test]
fn an_empty_data_directory_observes_nothing_and_does_not_panic() {
    let directory = TestDirectory::create("watch-empty");
    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert_eq!(observation.to_seq, 0);
    assert_eq!(observation.from_seq, 0);
    assert!(observation.pending.is_empty());
    assert!(observation.settled.is_empty());
    assert!(observation.instance_states.is_empty());
    assert!(observation.unresolved.is_empty());
}

#[test]
fn a_data_directory_that_is_not_a_directory_is_a_store_error() {
    let directory = TestDirectory::create("watch-not-a-dir");
    let file = directory.path().join("journal");
    fs::write(&file, b"not a store").unwrap();
    let mut watcher = Watcher::new(file, advancing());
    let error = watcher.scan(0).unwrap_err();
    assert_eq!(error.code, "exec/store");
}

#[test]
fn one_scan_reports_the_pending_effect_with_its_name_and_args() {
    let directory = TestDirectory::create("watch-pending");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let expected_last_seq = writer.store.journal.last_seq;
    let expected_ids = writer.pending("case-1");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert_eq!(observation.to_seq, expected_last_seq);
    assert_eq!(observation.pending.len(), 1);
    let effect = &observation.pending[0];
    assert_eq!(effect.instance_id, "case-1");
    assert_eq!(effect.effect_id, expected_ids[0]);
    assert_eq!(effect.effect_name, "assign_reviewer");
    assert_eq!(ctx_val_string(&effect.args["case"]), "case-0");
    assert_eq!(observation.instance_states["case-1"].status, "running");
    assert_eq!(observation.instance_states["case-1"].pending, 1);
}

#[test]
fn a_still_pending_effect_is_reported_again_without_a_second_fold() {
    let directory = TestDirectory::create("watch-repeat");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let first = scan(&mut watcher);
    assert_eq!(watcher.resolved_count(), 1);
    let second = scan(&mut watcher);
    assert_eq!(
        first.pending, second.pending,
        "pending work is a state, not an edge"
    );
    assert_eq!(
        watcher.resolved_count(),
        1,
        "the memo answered the second scan"
    );
    assert_eq!(second.from_seq, first.to_seq);
}

#[test]
fn an_effect_emitted_on_entering_a_terminal_state_is_still_reported() {
    let directory = TestDirectory::create("watch-terminal");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-manual");
    writer.send("case-1", "close");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert_eq!(observation.instance_states["case-1"].status, "completed");
    assert_eq!(observation.pending.len(), 1);
    assert_eq!(observation.pending[0].effect_name, "archive_case");
}

#[test]
fn two_pending_effects_are_both_reported() {
    let directory = TestDirectory::create("watch-two");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    writer.send("case-1", "escalate");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    let names: Vec<&str> = observation
        .pending
        .iter()
        .map(|effect| effect.effect_name.as_str())
        .collect();
    assert_eq!(names, ["assign_reviewer", "notify_manager"]);
    assert_eq!(watcher.resolved_count(), 2);
}

#[test]
fn only_the_executors_own_keys_are_carried_from_the_dedup_map() {
    let directory = TestDirectory::create("watch-keys");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-case-1/2/0");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert!(
        observation
            .claimed_request_ids
            .contains("exec-ack-case-1/2/0")
    );
    assert!(
        observation
            .claimed_request_ids
            .iter()
            .all(|key| key.starts_with("exec-")),
        "a CLI-written req- key never enters the set: {:?}",
        observation.claimed_request_ids
    );
}

#[test]
fn an_acked_effect_leaves_pending_and_appears_as_settled() {
    let directory = TestDirectory::create("watch-settled");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-settled");
    let ack_seq = writer.store.journal.last_seq;
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert!(observation.pending.is_empty());
    assert_eq!(observation.settled.len(), 1);
    let settled = &observation.settled[0];
    assert_eq!(settled.instance_id, "case-1");
    assert_eq!(settled.effect_id, assigned);
    assert_eq!(settled.effect_name, "assign_reviewer");
    assert_eq!(settled.outcome, "ok");
    assert_eq!(settled.seq, ack_seq);
    assert!(observation.claimed_request_ids.contains("exec-ack-settled"));
}

#[test]
fn a_settled_effect_whose_advance_is_journaled_is_dropped() {
    let directory = TestDirectory::create("watch-bounded");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-advanced");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    assert_eq!(scan(&mut watcher).settled.len(), 1);

    // The advance the executor would send, under the key it derives.
    let mut writer = Writer::open(&directory);
    writer.send_with_request(
        "case-1",
        "escalate",
        &format!("exec-ev-{assigned}-escalate"),
    );
    drop(writer);
    let after_advance = scan(&mut watcher);
    assert!(
        after_advance.settled.is_empty(),
        "the claimed advance key is what says the pair completed"
    );
}

#[test]
fn an_ack_overtaken_by_an_unrelated_event_is_still_offered_for_recovery() {
    // Ack written, advance lost to a kill, and some other writer moved the
    // instance on before the executor came back. Ordering alone would abandon
    // the advance forever; the unclaimed advance key says it never happened.
    let directory = TestDirectory::create("watch-overtaken");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-overtaken");
    writer.send("case-1", "escalate");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert_eq!(observation.settled.len(), 1);
    assert_eq!(observation.settled[0].effect_id, assigned);
}

#[test]
fn a_creation_time_effect_is_re_resolved_rather_than_remembered() {
    // `{instance}/0/{k}` repeats if an instance id is re-used, so a memo keyed
    // on the id alone would hand back the first life's arguments.
    let directory = TestDirectory::create("watch-creation-memo");
    let mut writer = Writer::open(&directory);
    let definition = parse(
        br#"{
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
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    writer
        .store
        .define_machine_on(&mut writer.clock, definition, false, false)
        .unwrap();
    let first_request = writer.request_id();
    writer
        .store
        .create_instance_ctx_on(
            &mut writer.clock,
            "case_intake_effects",
            "case-1",
            &first_request,
            None,
            &BTreeMap::from([("case_id".to_string(), Val::Str("case-first".into()))]),
            &[],
        )
        .unwrap();
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let first = scan(&mut watcher);
    assert_eq!(ctx_val_string(&first.pending[0].args["case"]), "case-first");
    assert_eq!(
        watcher.resolved_count(),
        0,
        "a creation-time id is deliberately not remembered"
    );

    // An instance has exactly one creation since plan 0017 task 7903, so there
    // is no second one for the scan to follow. What the property was really
    // about survives: the id is re-resolved on every scan rather than cached,
    // which the unchanged `resolved_count` below is the evidence for.
    let mut writer = Writer::open(&directory);
    let second_request = writer.request_id();
    let refusal = writer
        .store
        .create_instance_ctx_on(
            &mut writer.clock,
            "case_intake_effects",
            "case-1",
            &second_request,
            None,
            &BTreeMap::from([("case_id".to_string(), Val::Str("case-second".into()))]),
            &[],
        )
        .expect_err("a second creation of one instance id is refused");
    assert_eq!(refusal.code, "req/instance_exists");
    drop(writer);

    let second = scan(&mut watcher);
    assert_eq!(
        ctx_val_string(&second.pending[0].args["case"]),
        "case-first",
        "the scan re-resolved the creation-time id against the one creation"
    );
    assert_eq!(
        watcher.resolved_count(),
        0,
        "a creation-time id is still not remembered between scans"
    );
}

#[test]
fn an_ack_whose_handler_declares_no_advance_is_not_outstanding() {
    // Nothing will ever retire such an ack — no advance means no key is ever
    // claimed — so listing it would fill the bounded window with acks nobody
    // will act on and crowd out a genuinely interrupted advance.
    let directory = TestDirectory::create("watch-no-advance");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-no-advance");
    drop(writer);

    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        BTreeSet::from(["notify_manager".to_string()]),
    );
    let observation = watcher.scan(10_000).expect("scan succeeds");
    assert!(
        observation.settled.is_empty(),
        "assign_reviewer declares no advance in this table: {:?}",
        observation.settled
    );
}

#[test]
fn one_instance_contributes_a_bounded_number_of_settled_acks() {
    // A handler with no declared advance never claims an advance key, so
    // nothing would ever retire these acks. The newest few are the only ones
    // recovery could act on — sending one advance transitions the instance —
    // so the list is capped rather than left to grow with the journal.
    let directory = TestDirectory::create("watch-settled-cap");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-0");
    for index in 0..12 {
        writer.send("case-1", "ping");
        let emitted = writer.pending("case-1")[0].clone();
        writer.ack("case-1", &emitted, &format!("exec-ack-ping-{index}"));
    }
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert_eq!(observation.settled.len(), 8);
    assert!(
        observation
            .settled
            .windows(2)
            .all(|pair| pair[0].seq < pair[1].seq),
        "settled stays in journal order"
    );
    assert_eq!(
        observation.settled.last().unwrap().seq,
        observation.settled.iter().map(|s| s.seq).max().unwrap(),
        "the newest acks are the ones kept"
    );
}

#[test]
fn a_cancelled_instances_pending_effects_are_not_offered_to_run() {
    // Cancel means stop. The scheduler kills what is in flight; starting a new
    // handler for the same instance on the next tick would undo that.
    let directory = TestDirectory::create("watch-cancel-pending");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    assert_eq!(writer.pending("case-1").len(), 1);
    writer.cancel("case-1");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert!(observation.pending.is_empty());
    assert_eq!(
        observation.instance_states["case-1"].pending, 1,
        "the effect is still pending in the journal, and says so"
    );
}

#[test]
fn an_ack_on_a_finished_instance_is_never_listed_as_settled() {
    let directory = TestDirectory::create("watch-finished");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    let assigned = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &assigned, "exec-ack-a");
    writer.send("case-1", "close");
    let archived = writer.pending("case-1")[0].clone();
    writer.ack("case-1", &archived, "exec-ack-b");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = scan(&mut watcher);
    assert_eq!(observation.instance_states["case-1"].status, "completed");
    assert!(
        observation.settled.is_empty(),
        "a completed instance has no enabled event to advance into"
    );
}

#[test]
fn a_writer_may_append_between_scans_without_locking_the_watcher_out() {
    let directory = TestDirectory::create("watch-concurrent");
    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());

    // The writer stays open across both scans: the watcher takes no lock, so
    // this is exactly the paired-mode arrangement.
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    let before = scan(&mut watcher);
    assert!(before.pending.is_empty());

    writer.send("case-1", "submit");
    let after = scan(&mut watcher);
    assert_eq!(after.pending.len(), 1);
    assert!(
        after.to_seq > before.to_seq,
        "the next open sees the append"
    );
}

#[test]
fn a_cancellation_is_reported_once_and_not_on_every_later_scan() {
    let directory = TestDirectory::create("watch-cancel");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.send("case-1", "submit");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let running = scan(&mut watcher);
    assert!(running.cancellations.is_empty());

    let mut writer = Writer::open(&directory);
    writer.cancel("case-1");
    drop(writer);

    let cancelled = scan(&mut watcher);
    assert_eq!(cancelled.cancellations, ["case-1"]);
    let later = scan(&mut watcher);
    assert!(
        later.cancellations.is_empty(),
        "the cancel is an edge, reported on the scan that observes it"
    );
    assert_eq!(later.instance_states["case-1"].status, "cancelled");
}

#[test]
fn a_deadline_at_or_past_the_observed_time_is_due() {
    let directory = TestDirectory::create("watch-deadline");
    let mut writer = Writer::open(&directory);
    let definition = parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"case_review_timeout",
            "context":[],
            "events":[{"name":"approve","fields":[]}],
            "effects":[],
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
                "to":"expired"
            }]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let request = writer.request_id();
    writer
        .store
        .define_machine_on(&mut writer.clock, definition, false, false)
        .unwrap();
    writer
        .store
        .create_instance_ctx_on(
            &mut writer.clock,
            "case_review_timeout",
            "case-1",
            &request,
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let due_ms = writer.store.state.instances["case-1"].deadlines["review_timeout"];
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let early = watcher.scan(due_ms - 1).unwrap();
    assert!(early.due_deadlines.is_empty());
    let due = watcher.scan(due_ms).unwrap();
    assert_eq!(due.due_deadlines.len(), 1);
    assert_eq!(due.due_deadlines[0].deadline_name, "review_timeout");
    assert_eq!(due.due_deadlines[0].due_ms, due_ms);
}

#[test]
fn a_cancelled_instances_deadlines_are_never_due() {
    let directory = TestDirectory::create("watch-cancel-deadline");
    let mut writer = Writer::open(&directory);
    writer.define_and_create("case-1");
    writer.cancel("case-1");
    drop(writer);

    let mut watcher = Watcher::new(directory.path().to_path_buf(), advancing());
    let observation = watcher.scan(i64::MAX).unwrap();
    assert!(observation.due_deadlines.is_empty());
}
