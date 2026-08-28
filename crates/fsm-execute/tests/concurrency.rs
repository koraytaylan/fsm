//! The concurrency caps: how many handler processes exist at once, and which
//! ones get the slots.
//!
//! Before these caps an outbox holding five hundred pending effects spawned
//! five hundred subprocesses. The fix has to be **deterministic** — the same
//! observation and the same `now_ms` must produce the same directives, or
//! restart equivalence stops meaning anything — so the caps are applied over a
//! stable total order rather than over whatever order the scan happened to
//! produce.
//!
//! The scheduler is pure, so every row here is a fabricated observation: no
//! store, no subprocess, no clock.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_execute::config::{
    DEFAULT_MAX_INFLIGHT, DEFAULT_MAX_INFLIGHT_PER_INSTANCE, HandlerTable, MAX_MAX_INFLIGHT,
    MAX_MAX_INFLIGHT_PER_INSTANCE,
};
use fsm_execute::effect::PendingEffect;
use fsm_execute::run::KillReason;
use fsm_execute::sched::{Directive, Scheduler};
use fsm_execute::watch::{DueDeadline, Observation};

const NOW: i64 = 1_700_000_000_000;

/// A table with both caps set, or with neither when `caps` is empty.
fn table(caps: &str) -> HandlerTable {
    HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",{caps}
            "handlers":[{{
                "effect":"notify",
                "argv":["/usr/local/bin/notify","--case","{{case}}"],
                "timeout_ms":30000,
                "on_ok":{{"event":"notified"}},
                "on_failed":{{"event":"notify_failed"}}
            }}]
        }}"#
    ))
    .expect("the table validates")
}

fn effect(instance: &str, k: u32) -> PendingEffect {
    PendingEffect {
        instance_id: instance.to_string(),
        effect_id: format!("{instance}/3/{k}"),
        effect_name: "notify".to_string(),
        args: BTreeMap::from([("case".to_string(), Val::Str("case-9".into()))]),
        emitted_seq: 3,
        k,
    }
}

/// `instances` instances, each with `per` pending effects.
fn spread(instances: u32, per: u32) -> Observation {
    let mut pending = Vec::new();
    for instance in 0..instances {
        for k in 0..per {
            pending.push(effect(&format!("case-{instance:02}"), k));
        }
    }
    Observation {
        pending,
        ..Observation::default()
    }
}

fn starts(directives: &[Directive]) -> Vec<String> {
    directives
        .iter()
        .filter_map(|directive| match directive {
            Directive::Start { effect, .. } => Some(effect.effect_id.clone()),
            _ => None,
        })
        .collect()
}

/// Settle every started effect, as the driver does when a run finishes.
fn complete_all(scheduler: &mut Scheduler, started: &[String]) {
    for effect_id in started {
        scheduler.complete(effect_id);
    }
}

#[test]
fn forty_candidates_under_a_global_cap_of_eight_start_exactly_eight() {
    let mut scheduler =
        Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":16,"#));
    let directives = scheduler.on_observation(&spread(10, 4), NOW);
    assert_eq!(starts(&directives).len(), 8, "{directives:?}");
    let capped = scheduler.capped().expect("a cap bound this tick");
    assert_eq!(capped.deferred, 32);
    assert_eq!(capped.inflight, 8);
}

#[test]
fn a_tick_at_the_cap_starts_nothing_and_one_completion_frees_exactly_one_slot() {
    let mut scheduler =
        Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":16,"#));
    let observation = spread(10, 4);
    let first = starts(&scheduler.on_observation(&observation, NOW));
    assert_eq!(first.len(), 8);

    // Nothing finished, so nothing may start: the eight are still running.
    let second = scheduler.on_observation(&observation, NOW + 1);
    assert!(starts(&second).is_empty(), "{second:?}");
    assert_eq!(scheduler.capped().expect("still capped").deferred, 32);

    scheduler.complete(&first[0]);
    let third = starts(&scheduler.on_observation(&observation, NOW + 2));
    assert_eq!(third.len(), 1, "one slot freed means exactly one start");
    // And it is the next one in the order, not the one that just finished:
    // that effect is still in `pending` here because nothing acked it, and a
    // scheduler that restarted it would run the same work twice.
    assert_eq!(third, [first[0].clone()]);
}

#[test]
fn the_per_instance_cap_binds_even_when_the_global_one_has_room() {
    let mut scheduler =
        Scheduler::new(table(r#""max_inflight":64,"max_inflight_per_instance":2,"#));
    // One instance with ten pending effects and sixty-four slots free.
    let directives = scheduler.on_observation(&spread(1, 10), NOW);
    let started = starts(&directives);
    assert_eq!(started.len(), 2, "{directives:?}");
    assert_eq!(started, ["case-00/3/0", "case-00/3/1"]);
    assert_eq!(scheduler.capped().expect("capped per instance").deferred, 8);
}

#[test]
fn with_both_caps_the_binding_one_wins_in_each_direction() {
    // Global binds: five instances, two each, ten candidates, four slots.
    let mut global = Scheduler::new(table(r#""max_inflight":4,"max_inflight_per_instance":2,"#));
    assert_eq!(starts(&global.on_observation(&spread(5, 2), NOW)).len(), 4);

    // Per-instance binds: two instances of ten, thirty-two global slots, so
    // only the per-instance cap can stop anything.
    let mut per_instance =
        Scheduler::new(table(r#""max_inflight":32,"max_inflight_per_instance":3,"#));
    let started = starts(&per_instance.on_observation(&spread(2, 10), NOW));
    assert_eq!(started.len(), 6, "three per instance, two instances");
    // Round-robin, per `7602`: every instance's first effect before any
    // instance's second.
    assert_eq!(
        started,
        [
            "case-00/3/0",
            "case-01/3/0",
            "case-00/3/1",
            "case-01/3/1",
            "case-00/3/2",
            "case-01/3/2",
        ]
    );
}

#[test]
fn a_hundred_runs_under_the_caps_produce_the_same_directives() {
    // A cap that takes an arbitrary prefix of an unordered set would make the
    // same observation produce different directives on different runs, and
    // restart equivalence is what everything else rests on.
    let handlers = table(r#""max_inflight":8,"max_inflight_per_instance":2,"#);
    let observation = spread(10, 4);
    let rendered = |directives: &[Directive]| format!("{directives:?}");

    let mut first = Scheduler::new(handlers.clone());
    let expected = rendered(&first.on_observation(&observation, NOW));
    for run in 0..100 {
        let mut scheduler = Scheduler::new(handlers.clone());
        let directives = scheduler.on_observation(&observation, NOW);
        assert_eq!(rendered(&directives), expected, "run {run} differed");
    }
}

#[test]
fn a_full_executor_still_kills_polls_and_advances() {
    // A `Kill`, a `SendEvent`, and a `PollDeadline` are bookkeeping against
    // the journal. None costs a subprocess, so a concurrency bound has no
    // business deferring one — and a timed-out handler that could not be
    // killed because the host was busy is the worst version of this bug.
    let mut scheduler = Scheduler::new(table(r#""max_inflight":2,"max_inflight_per_instance":2,"#));
    let observation = spread(4, 2);
    let started = starts(&scheduler.on_observation(&observation, NOW));
    assert_eq!(started.len(), 2);

    let mut with_deadline = spread(4, 2);
    with_deadline.due_deadlines = vec![DueDeadline {
        instance_id: "case-03".to_string(),
        deadline_name: "escalate".to_string(),
        due_ms: NOW,
    }];
    // Past the first handler's timeout, so the running pair must be killed.
    let directives = scheduler.on_observation(&with_deadline, NOW + 30_001);
    assert!(starts(&directives).is_empty(), "still at the cap");
    let kills: Vec<&Directive> = directives
        .iter()
        .filter(|directive| matches!(directive, Directive::Kill { .. }))
        .collect();
    assert_eq!(kills.len(), 2, "{directives:?}");
    assert!(
        kills.iter().all(|directive| matches!(
            directive,
            Directive::Kill {
                reason: KillReason::Timeout,
                ..
            }
        )),
        "{directives:?}"
    );
    assert!(
        directives
            .iter()
            .any(|directive| matches!(directive, Directive::PollDeadline { .. })),
        "a due deadline is journal bookkeeping, not a subprocess: {directives:?}"
    );
}

#[test]
fn the_deferral_is_reported_once_per_tick_with_counts_only() {
    let mut scheduler =
        Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":16,"#));
    let observation = spread(10, 4);
    scheduler.on_observation(&observation, NOW);
    let capped = scheduler.capped().expect("a cap bound this tick");
    // Counts, not a line per effect: an outbox of five hundred would drown
    // the trace this is meant to explain.
    assert_eq!(capped.deferred, 32);
    assert_eq!(capped.inflight, 8);

    // And a tick with room to spare says nothing at all.
    let mut roomy = Scheduler::new(table(
        r#""max_inflight":64,"max_inflight_per_instance":16,"#,
    ));
    roomy.on_observation(&spread(2, 2), NOW);
    assert_eq!(roomy.capped(), None);
}

#[test]
fn a_restarted_scheduler_starts_up_to_the_cap_again() {
    // The cap is about *this process's* concurrency, not about journal state.
    // A predecessor's children are gone, and their effects are still pending
    // precisely because nothing acked them.
    let handlers = table(r#""max_inflight":8,"max_inflight_per_instance":16,"#);
    let observation = spread(10, 4);
    let mut before = Scheduler::new(handlers.clone());
    let first = starts(&before.on_observation(&observation, NOW));
    assert_eq!(first.len(), 8);

    let mut after = Scheduler::new(handlers);
    let second = starts(&after.on_observation(&observation, NOW));
    assert_eq!(second, first);
}

#[test]
fn completing_everything_lets_the_next_batch_through() {
    let handlers = table(r#""max_inflight":8,"max_inflight_per_instance":16,"#);
    let mut scheduler = Scheduler::new(handlers);
    let observation = spread(10, 4);
    let first = starts(&scheduler.on_observation(&observation, NOW));
    complete_all(&mut scheduler, &first);
    let second = starts(&scheduler.on_observation(&observation, NOW + 1));
    assert_eq!(second.len(), 8, "a drained executor fills up again");
}

#[test]
fn a_table_with_neither_key_gets_the_documented_defaults() {
    let handlers = table("");
    assert_eq!(handlers.max_inflight, DEFAULT_MAX_INFLIGHT);
    assert_eq!(
        handlers.max_inflight_per_instance,
        DEFAULT_MAX_INFLIGHT_PER_INSTANCE
    );
    assert_eq!(DEFAULT_MAX_INFLIGHT, 8);
    assert_eq!(DEFAULT_MAX_INFLIGHT_PER_INSTANCE, 2);

    // And the defaults are the ones actually applied. Ten instances of four
    // under the round-robin: the eight slots go to eight *different*
    // instances, because every instance's first effect is considered before
    // any instance's second. An `effect_id` ordering would have spent all
    // eight on `case-00` through `case-03`.
    let mut scheduler = Scheduler::new(table(""));
    let started = starts(&scheduler.on_observation(&spread(10, 4), NOW));
    assert_eq!(
        started,
        [
            "case-00/3/0",
            "case-01/3/0",
            "case-02/3/0",
            "case-03/3/0",
            "case-04/3/0",
            "case-05/3/0",
            "case-06/3/0",
            "case-07/3/0",
        ],
        "eight globally, one apiece"
    );

    // The per-instance default binds when there is only one instance to
    // spend the slots on.
    let mut alone = Scheduler::new(table(""));
    assert_eq!(
        starts(&alone.on_observation(&spread(1, 4), NOW)),
        ["case-00/3/0", "case-00/3/1"],
        "two per instance"
    );
}

fn refused(caps: &str) -> String {
    let table = HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",{caps}
            "handlers":[{{
                "effect":"notify",
                "argv":["/usr/local/bin/notify"],
                "timeout_ms":1000
            }}]
        }}"#
    ));
    match table {
        Ok(_) => panic!("{caps} should not validate"),
        Err(error) => {
            assert_eq!(error.code, "exec/config");
            error.message
        }
    }
}

#[test]
fn a_cap_outside_its_range_is_a_config_error() {
    // Zero is refused rather than read as "unbounded": a table saying
    // `max_inflight: 0` almost certainly means "do not limit me", and
    // honouring that reading would start nothing at all.
    assert!(refused(r#""max_inflight":0,"#).contains("max_inflight"));
    assert!(
        refused(&format!(r#""max_inflight":{},"#, MAX_MAX_INFLIGHT + 1)).contains("64"),
        "the ceiling is stated in the message"
    );
    assert!(refused(r#""max_inflight_per_instance":0,"#).contains("max_inflight_per_instance"));
    assert!(
        refused(&format!(
            r#""max_inflight_per_instance":{},"#,
            MAX_MAX_INFLIGHT_PER_INSTANCE + 1
        ))
        .contains("16")
    );
    // Not a number at all.
    assert!(refused(r#""max_inflight":"eight","#).contains("max_inflight"));
}

#[test]
fn a_misspelled_top_level_key_is_refused_rather_than_ignored() {
    // The same rule the handler keys follow: a table that validated while
    // silently ignoring `max_in_flight` would spawn without a bound and look
    // like it had one.
    let message = refused(r#""max_in_flight":8,"#);
    assert!(message.contains("max_in_flight"), "{message}");
}

#[test]
fn the_boundaries_of_each_range_are_accepted() {
    let smallest = table(r#""max_inflight":1,"max_inflight_per_instance":1,"#);
    assert_eq!(smallest.max_inflight, 1);
    assert_eq!(smallest.max_inflight_per_instance, 1);
    let largest = table(&format!(
        r#""max_inflight":{MAX_MAX_INFLIGHT},"max_inflight_per_instance":{MAX_MAX_INFLIGHT_PER_INSTANCE},"#
    ));
    assert_eq!(largest.max_inflight, MAX_MAX_INFLIGHT);
    assert_eq!(
        largest.max_inflight_per_instance,
        MAX_MAX_INFLIGHT_PER_INSTANCE
    );

    // One slot at a time really is one at a time.
    let mut scheduler = Scheduler::new(table(r#""max_inflight":1,"max_inflight_per_instance":1,"#));
    assert_eq!(
        starts(&scheduler.on_observation(&spread(4, 4), NOW)).len(),
        1
    );
}

// The rows above are about the decision. This one is about the *line*: the
// scheduler can report a deferral all it likes, but if the driver does not put
// it in the trace an operator watching a busy executor sees a tick that
// started two things and said nothing about the other thirty-eight.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::run::{Pipeline, Runner};
use fsm_execute::service::tick;
use fsm_execute::watch::Watcher;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-execute-conc-{test_name}-{}-{sequence}",
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

/// The stub handler: this test binary re-executed with a marker argument.
#[test]
fn stub_handler() {
    if std::env::args().any(|argument| argument == "stub:ok") {
        std::process::exit(0);
    }
}

/// A machine whose one state emits four notifications on entry.
fn fanout_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"review_fanout",
            "context":[{"name":"case_ref","ty":"str","init":"case-7"}],
            "events":[
                {"name":"open","fields":[]},
                {"name":"notified","fields":[]},
                {"name":"notify_failed","fields":[]}
            ],
            "effects":[{"name":"notify","fields":[{"name":"case","ty":"str"}]}],
            "states":[
                {"name":"intake"},
                {"name":"notifying","entry":{"emit":[
                    {"effect":"notify","args":{"case":"ctx.case_ref"}},
                    {"effect":"notify","args":{"case":"ctx.case_ref"}},
                    {"effect":"notify","args":{"case":"ctx.case_ref"}},
                    {"effect":"notify","args":{"case":"ctx.case_ref"}}
                ]}},
                {"name":"reviewer_notified","terminal":true},
                {"name":"reviewer_unreachable","terminal":true}
            ],
            "initial":"intake",
            "transitions":[
                {"from":"intake","on":"open","to":"notifying"},
                {"from":"notifying","on":"notified","to":"reviewer_notified"},
                {"from":"notifying","on":"notify_failed","to":"reviewer_unreachable"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("the fan-out machine parses")
}

fn stub_table_json(caps: &str) -> String {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        // A Windows path is backslash-separated, and a backslash begins an
        // escape in a JSON string: interpolating one raw makes the table
        // unparseable on that platform and nowhere else.
        .replace('\\', "\\\\");
    format!(
        r#"{{
            "format":"fsm.handlers/1",{caps}
            "handlers":[{{
                "effect":"notify",
                "argv":["{stub}","stub_handler","--exact","--nocapture","stub:ok"],
                "timeout_ms":30000,
                "on_ok":{{"event":"notified"}}
            }}]
        }}"#
    )
}

#[test]
fn a_tick_that_defers_says_so_in_the_trace_once_with_counts_only() {
    let directory = TestDirectory::create("deferral-line");
    let mut store = Store::open(directory.path()).expect("a fresh directory opens");
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, fanout_machine(), false, false)
        .expect("the machine defines");
    for instance in 0..10 {
        let instance_id = format!("case-{instance:02}");
        store
            .create_instance_ctx_on(
                &mut clock,
                "review_fanout",
                &instance_id,
                &format!("req-create-{instance_id}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .expect("the instance is created");
        store
            .send_event_stamp_on(
                &mut clock,
                &instance_id,
                "open",
                &mut Value::Obj(BTreeMap::new()),
                &format!("req-open-{instance_id}"),
                None,
                &[],
            )
            .expect("opening the case emits four notifications");
    }
    assert_eq!(store.state.instances["case-00"].pending.len(), 4);
    drop(store);

    let handlers = HandlerTable::parse(&stub_table_json(
        r#""max_inflight":2,"max_inflight_per_instance":2,"#,
    ))
    .expect("the stub table validates");
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&handlers),
    );
    let mut scheduler = Scheduler::new(handlers);
    let mut runner = Runner::new().expect("the runner makes its scratch directory");
    let mut pipeline = Pipeline;
    let mut executor_clock = FixedClock::new(5_000, 1);
    let lines = tick(
        &mut watcher,
        &mut scheduler,
        &mut runner,
        &mut pipeline,
        directory.path(),
        &mut executor_clock,
        5_000,
    );

    let deferrals: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("error exec/inflight_deferred"))
        .collect();
    assert_eq!(
        deferrals.len(),
        1,
        "once per tick, not once per effect: {lines:?}"
    );
    assert_eq!(
        deferrals[0],
        "error exec/inflight_deferred deferred=38 inflight=2"
    );
    // Counts and a code, nothing else: this stream is byte-compared, so a
    // path, a pid, or a duration in it would differ per host and per run.
    let scratch = directory.path().to_string_lossy().into_owned();
    for line in &lines {
        assert!(!line.contains(&scratch), "{line}");
    }
}
