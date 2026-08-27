//! Restart the executor in the middle of a retry sequence, and assert what the
//! journal says afterwards.
//!
//! Retry exists to survive a restart, so the suite that proves it has to
//! interrupt one. The invariant that matters is that a killed process never
//! **costs or gains an attempt**: the count is derived from `effect_attempted`
//! records and from nothing a process remembers, so a successor reaches the
//! number its predecessor would have.
//!
//! The word is **restart**, not `kill -9` — signal-kill coverage of the journal
//! lives in `crash_harness.rs`. Death is simulated by dropping the runner,
//! scheduler, and pipeline and building fresh ones against the same data
//! directory, exactly as `executor_chaos.rs` does.
//!
//! The at-least-once boundary is asserted honestly, as plan 0008 did: a restart
//! *before* the attempt record may re-run the handler, because the record is
//! what makes an attempt remembered and a killed process loses what it had not
//! journaled. The claim is that the **journal** stays exact, not that the world
//! does.
//!
//! The seeded generator below is a deliberate ~20-line duplication of the
//! xorshift64\* in `chaos.rs`, `proputil.rs`, and `executor_chaos.rs`, for the
//! reason documented there: a bug in one generator must not hide in the other.
//!
//! The MCP fixture drives **this project's own server**: `fsm serve
//! --read-only` on the same data directory, called with a tool that fails on
//! purpose. A second stub would only prove the stub.
//!
//! Budget: see `iterations()` — the committed default is set by measurement,
//! not by preference, and the depth stays behind `FSM_POLICY_CHAOS_ITERS`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::journal_io::{JournalHealth, verify};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_execute::config::{HandlerTable, substitute};
use fsm_execute::rid::attempt_rid;
use fsm_execute::run::{McpCall, Pipeline, RunOutcome, Runner};
use fsm_execute::sched::Scheduler;
use fsm_execute::service::tick;
use fsm_execute::watch::Watcher;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

/// Never lower this floor. See `iterations()` for why it is what it is.
const ITERATIONS: u64 = 40;
const BASE_MS: i64 = 1_700_000_000_000;
/// Small enough to keep a run fast, large enough that a deferral is observable
/// against the resume loop's step.
const BACKOFF_MS: i64 = 500;
const RESUME_STEP_MS: i64 = 250;
const ATTEMPTS: u32 = 3;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Where a restart lands inside a retry sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Restart {
    /// The handler ran and failed; the process is gone before its
    /// `effect_attempted` record.
    BeforeRecord,
    /// The record landed, and the process is gone with the backoff already
    /// elapsed.
    AfterRecord,
    /// The record landed, and the successor comes up *inside* the wait.
    DuringBackoff,
    /// Every attempt is spent; the process is gone before the exhaustion ack.
    BeforeExhaustionAck,
    /// A conversation with an MCP server is in flight.
    MidConversation,
}

impl Restart {
    const ALL: [Restart; 5] = [
        Restart::BeforeRecord,
        Restart::AfterRecord,
        Restart::DuringBackoff,
        Restart::BeforeExhaustionAck,
        Restart::MidConversation,
    ];

    fn of(seed: u64) -> Self {
        Restart::ALL[(seed % Restart::ALL.len() as u64) as usize]
    }
}

/// Which table the executor is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fixture {
    /// A process handler that always fails, retried three times.
    Retrying,
    /// A process handler that always fails and declares no `retry`, so it
    /// means exactly what it meant before this plan.
    NoRetry,
    /// An `mcp` handler calling a tool that fails, retried three times.
    Mcp,
    /// Several instances against caps small enough to bind.
    Capped,
    /// An instance cancelled while its handler is in flight.
    Cancelled,
}

impl Fixture {
    const ALL: [Fixture; 5] = [
        Fixture::Retrying,
        Fixture::NoRetry,
        Fixture::Mcp,
        Fixture::Capped,
        Fixture::Cancelled,
    ];

    fn of(seed: u64) -> Self {
        Fixture::ALL[((seed / 5) % Fixture::ALL.len() as u64) as usize]
    }

    /// How many attempts this fixture's handler is allowed.
    fn attempts(self) -> u32 {
        match self {
            Fixture::NoRetry => 1,
            _ => ATTEMPTS,
        }
    }

    /// The instances this fixture creates.
    fn instances(self) -> Vec<String> {
        match self {
            Fixture::Capped => (0..6).map(|index| format!("case-{index:02}")).collect(),
            _ => vec!["case-00".to_string()],
        }
    }

    /// How many notifications each instance emits.
    fn fanout(self) -> u32 {
        match self {
            Fixture::Capped => 3,
            _ => 1,
        }
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-policy-chaos-{test_name}-{}-{sequence}",
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

/// Open a writer, tolerating a lock this process itself just released.
///
/// Spawning a handler forks, and between `fork` and `exec` the child holds a
/// copy of every open descriptor, so an advisory lock dropped a moment ago can
/// still be held for the length of that window.
fn open_writer(path: &Path) -> Store {
    for _ in 0..50 {
        match Store::open(path) {
            Ok(store) => return store,
            Err(error) if error.code == "store/lock" => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("open writer {}: {error:?}", path.display()),
        }
    }
    panic!("the writer lock on {} never became free", path.display())
}

/// The recording stub: append one line, exit with the code it was given.
#[test]
fn stub_handler() {
    let arguments: Vec<String> = std::env::args().collect();
    let Some(side_file) = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("stub:record:"))
    else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(side_file)
    {
        let _ = file.write_all(b"ran\n");
    }
    // A separate argument rather than a suffix on the path: a Windows path
    // carries a colon of its own, and splitting on one would break there.
    let code = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("stub:exit:"))
        .and_then(|code| code.parse::<i32>().ok())
        .unwrap_or(0);
    std::process::exit(code);
}

/// xorshift64\*, kept local on purpose. See the module header.
fn next_seed(state: &mut u64) -> u64 {
    let mut seed = *state;
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    *state = seed;
    seed
}

fn machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"review_policy_chaos",
            "context":[{"name":"case_ref","ty":"str","init":"case-7"}],
            "events":[
                {"name":"open","fields":[]},
                {"name":"open_many","fields":[]},
                {"name":"notified","fields":[]},
                {"name":"notify_failed","fields":[]}
            ],
            "effects":[{"name":"notify","fields":[{"name":"case","ty":"str"}]}],
            "states":[
                {"name":"intake"},
                {"name":"notifying","entry":{"emit":[
                    {"effect":"notify","args":{"case":"ctx.case_ref"}}
                ]}},
                {"name":"fanning_out","entry":{"emit":[
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
                {"from":"intake","on":"open_many","to":"fanning_out"},
                {"from":"notifying","on":"notified","to":"reviewer_notified"},
                {"from":"notifying","on":"notify_failed","to":"reviewer_unreachable"},
                {"from":"fanning_out","on":"notified","to":"reviewer_notified"},
                {"from":"fanning_out","on":"notify_failed","to":"reviewer_unreachable"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("the chaos machine parses")
}

fn escaped(path: &str) -> String {
    path.replace('\\', "\\\\")
}

fn table(fixture: Fixture, directory: &TestDirectory, side_file: &Path) -> HandlerTable {
    // `retry.on` is kind-aware, so each kind lists the class it can actually
    // produce: `mcp_error` on a process handler is refused, and rightly.
    let retry = |class: &str| {
        format!(
            r#""retry":{{"attempts":{ATTEMPTS},"backoff_ms":{BACKOFF_MS},"max_backoff_ms":{},"on":["{class}"]}},"#,
            BACKOFF_MS * 4
        )
    };
    let source = match fixture {
        Fixture::Mcp => {
            // This project's own MCP server, read-only on the same directory —
            // no lock, no writes — asked for an instance that does not exist,
            // which is a tool error and therefore the `mcp_error` class.
            let fsm = escaped(env!("CARGO_BIN_EXE_fsm"));
            let dir = escaped(&directory.path().to_string_lossy());
            format!(
                r#"{{"format":"fsm.handlers/1","handlers":[{{
                    "effect":"notify",
                    "kind":"mcp",
                    "tool":"instance_get",
                    "arguments":{{"instance_id":"no-such-instance"}},
                    "argv":["{fsm}","serve","--read-only","--data-dir","{dir}"],
                    "timeout_ms":30000,
                    {}
                    "on_ok":{{"event":"notified"}},
                    "on_failed":{{"event":"notify_failed"}}
                }}]}}"#,
                retry("mcp_error")
            )
        }
        other => {
            let stub = escaped(
                &std::env::current_exe()
                    .expect("the test binary knows its own path")
                    .to_string_lossy(),
            );
            let side = escaped(&side_file.to_string_lossy());
            // `Capped` succeeds: its subject is how many run at once, not what
            // happens when they fail.
            let exit = if other == Fixture::Capped { 0 } else { 3 };
            let retry_block = if other == Fixture::NoRetry {
                String::new()
            } else {
                retry("nonzero_exit")
            };
            let caps = if other == Fixture::Capped {
                r#""max_inflight":4,"max_inflight_per_instance":2,"#
            } else {
                ""
            };
            format!(
                r#"{{"format":"fsm.handlers/1",{caps}"handlers":[{{
                    "effect":"notify",
                    "argv":["{stub}","stub_handler","--exact","--nocapture","stub:record:{side}","stub:exit:{exit}"],
                    "timeout_ms":30000,
                    {retry_block}
                    "on_ok":{{"event":"notified"}},
                    "on_failed":{{"event":"notify_failed"}}
                }}]}}"#
            )
        }
    };
    HandlerTable::parse(&source).expect("the chaos table validates")
}

/// Create this fixture's instances and drive each to "notification pending".
fn triggered(fixture: Fixture, directory: &TestDirectory) -> Vec<String> {
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(BASE_MS, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .expect("the machine defines");
    let mut pending = Vec::new();
    for instance_id in fixture.instances() {
        store
            .create_instance_ctx_on(
                &mut clock,
                "review_policy_chaos",
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
                if fixture == Fixture::Capped {
                    "open_many"
                } else {
                    "open"
                },
                &mut Value::Obj(BTreeMap::new()),
                &format!("req-open-{instance_id}"),
                None,
                &[],
            )
            .expect("opening the case emits the notification");
        pending.extend(store.state.instances[&instance_id].pending.clone());
    }
    if fixture == Fixture::Cancelled {
        // Cancelled *before* the executor ever sees it, which is the state a
        // successor comes up into: the effect is still pending and
        // unacknowledged, and no table may resurrect it.
        store
            .cancel_instance_reason_on(&mut clock, "case-00", "req-cancel", "")
            .expect("the instance cancels");
    }
    drop(store);
    assert_eq!(
        pending.len() as u32,
        fixture.instances().len() as u32 * fixture.fanout()
    );
    pending
}

/// Run one attempt to completion without journaling anything about it.
fn run_and_forget(directory: &TestDirectory, table: &HandlerTable, effect_id: &str) {
    let mut runner = Runner::new().expect("a scratch directory");
    let handler = &table.handlers["notify"];
    let args = BTreeMap::from([(
        "case".to_string(),
        fsm_core::expr::eval::Val::Str("case-7".into()),
    )]);
    let argv = substitute(&handler.argv, &args).expect("the argv substitutes");
    let call = match &handler.kind {
        fsm_execute::config::HandlerKind::Process => None,
        fsm_execute::config::HandlerKind::Mcp { tool, arguments } => Some(McpCall {
            tool: tool.clone(),
            arguments: arguments.clone(),
        }),
    };
    if runner
        .spawn(effect_id.to_string(), &argv, call.as_ref())
        .is_err()
    {
        return;
    }
    for _ in 0..400 {
        if runner.finished_effects().iter().any(|id| id == effect_id) {
            let _ = runner.poll(effect_id);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Everything this life held is dropped here, outcome included.
    drop(runner);
    let _ = directory;
}

/// Journal `count` failed attempts directly, as a predecessor would have.
fn journal_attempts(directory: &TestDirectory, effect_id: &str, instance_id: &str, count: u32) {
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(BASE_MS + 1_000, 1);
    for attempt in 1..=count {
        store
            .attempt_effect_on(
                &mut clock,
                instance_id,
                effect_id,
                &attempt_rid(effect_id, attempt),
                u64::from(attempt),
                Some(
                    RunOutcome::Completed {
                        status: 3,
                        stdout: fsm_execute::run::BoundedBytes::empty(),
                        stderr: fsm_execute::run::BoundedBytes::empty(),
                    }
                    .ack_result(),
                ),
            )
            .expect("an attempt records");
    }
    drop(store);
}

/// Kill the executor at one point, and return the `now_ms` its successor
/// should come up at.
fn die_at(
    restart: Restart,
    fixture: Fixture,
    directory: &TestDirectory,
    table: &HandlerTable,
    pending: &[String],
) -> i64 {
    let effect_id = &pending[0];
    let instance_id = effect_id
        .split('/')
        .next()
        .expect("an effect id names its instance");
    if fixture == Fixture::Cancelled {
        // A predecessor could not have run this handler or journaled an
        // attempt for it: the scheduler never starts an effect of a cancelled
        // instance, and a run killed by cancellation is settled rather than
        // retried. Fabricating either would be testing a state the system
        // cannot produce. What a restart *can* land on is the cancellation
        // itself, which is what the successor now comes up into.
        return BASE_MS + 1_000;
    }
    match restart {
        Restart::BeforeRecord => {
            run_and_forget(directory, table, effect_id);
            BASE_MS + 1_000
        }
        Restart::AfterRecord | Restart::DuringBackoff => {
            if fixture.attempts() > 1 {
                journal_attempts(directory, effect_id, instance_id, 1);
            }
            match restart {
                // Past the wait, so the successor's first tick may act.
                Restart::AfterRecord => BASE_MS + 1_000 + BACKOFF_MS * 4,
                // Inside it, so the successor's first ticks must defer.
                _ => BASE_MS + 1_000,
            }
        }
        Restart::BeforeExhaustionAck => {
            if fixture.attempts() > 1 {
                journal_attempts(directory, effect_id, instance_id, fixture.attempts() - 1);
            }
            run_and_forget(directory, table, effect_id);
            BASE_MS + 1_000 + BACKOFF_MS * 8
        }
        Restart::MidConversation => {
            // Spawned and abandoned without waiting: for the MCP fixture the
            // conversation is genuinely in flight, and for the others the
            // handler is.
            let mut runner = Runner::new().expect("a scratch directory");
            let handler = &table.handlers["notify"];
            let args = BTreeMap::from([(
                "case".to_string(),
                fsm_core::expr::eval::Val::Str("case-7".into()),
            )]);
            if let Ok(argv) = substitute(&handler.argv, &args) {
                let call = match &handler.kind {
                    fsm_execute::config::HandlerKind::Process => None,
                    fsm_execute::config::HandlerKind::Mcp { tool, arguments } => Some(McpCall {
                        tool: tool.clone(),
                        arguments: arguments.clone(),
                    }),
                };
                let _ = runner.spawn(effect_id.to_string(), &argv, call.as_ref());
            }
            drop(runner);
            BASE_MS + 1_000
        }
    }
}

/// What one tick saw, for the cap assertions.
#[derive(Default)]
struct Watermarks {
    inflight: usize,
    per_instance: usize,
    served: BTreeSet<String>,
}

/// Run a fresh executor to completion against a store somebody else started.
fn resume(directory: &TestDirectory, table: HandlerTable, mut now_ms: i64) -> Watermarks {
    let instances: Vec<String> = {
        let store = Store::open_read_only(directory.path()).expect("the store opens");
        store.state.instances.keys().cloned().collect()
    };
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&table),
    );
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().expect("a scratch directory");
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(now_ms, 1);
    let mut marks = Watermarks::default();
    for _ in 0..200 {
        let lines = tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut clock,
            now_ms,
        );
        // Counted from the trace rather than from the scheduler's internals,
        // because a cap that only holds when you ask it nicely is not a cap.
        let started: Vec<String> = lines
            .iter()
            .filter_map(|line| line.strip_prefix("spawned handler notify "))
            .map(std::string::ToString::to_string)
            .collect();
        marks.inflight = marks.inflight.max(started.len());
        let mut per_instance: BTreeMap<&str, usize> = BTreeMap::new();
        for effect_id in &started {
            let instance = effect_id.split('/').next().unwrap_or_default();
            *per_instance.entry(instance).or_default() += 1;
            marks.served.insert(instance.to_string());
        }
        marks.per_instance = marks
            .per_instance
            .max(per_instance.values().copied().max().unwrap_or(0));
        now_ms += RESUME_STEP_MS;
        clock.now = now_ms;

        let store = Store::open_read_only(directory.path()).expect("the store opens");
        // Empty outboxes, not terminal instances: a terminal instance's
        // remaining effects are still acked — plan 0008's rule — and stopping
        // at the first terminal one would leave them unsettled and hide
        // whatever the successor did with them.
        let done = instances.iter().all(|instance_id| {
            store
                .state
                .instances
                .get(instance_id)
                .is_none_or(|instance| {
                    instance.pending.is_empty()
                        || instance.status == fsm_core::machine::Status::Cancelled
                })
        });
        drop(store);
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    marks
}

/// Every attempt record and ack this journal holds, per effect.
struct Journal {
    attempts: BTreeMap<String, Vec<u32>>,
    acks: BTreeMap<String, Vec<Value>>,
    health: JournalHealth,
}

fn observe(directory: &TestDirectory) -> Journal {
    let report = verify(directory.path());
    let store = Store::open_read_only(directory.path()).expect("the store opens");
    let mut attempts: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut acks: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for record in &store.records {
        let Some(effect_id) = record.body.get("effect_id").and_then(Value::as_str) else {
            continue;
        };
        match record.kind {
            RecordKind::EffectAttempted => {
                let attempt = record
                    .body
                    .get("attempt")
                    .and_then(Value::as_num)
                    .and_then(|attempt| attempt.parse::<u32>().ok())
                    .unwrap_or(0);
                attempts
                    .entry(effect_id.to_string())
                    .or_default()
                    .push(attempt);
            }
            RecordKind::EffectAcked => acks
                .entry(effect_id.to_string())
                .or_default()
                .push(record.body.clone()),
            _ => {}
        }
    }
    Journal {
        attempts,
        acks,
        health: report.health,
    }
}

/// The most times the handler may run, counting the at-least-once boundary.
///
/// This is the assertion that catches a count derived from *memory* rather
/// than from the journal. Every journal invariant can still hold while the
/// handler runs an extra time per restart — the store refuses an out-of-order
/// attempt, the effect stays pending, and the next tick quietly fixes the
/// number — so a suite that only reads records would pass a policy that turns
/// "three tries" into four runs. Verified by mutation during development:
/// swapping the derivation for a process-local counter is red here and green
/// everywhere else.
fn max_runs(fixture: Fixture, restart: Restart) -> Option<usize> {
    let attempts = fixture.attempts() as usize;
    match fixture {
        // Runs `fsm serve`, not the recording stub, so there is no side file.
        Fixture::Mcp => None,
        // Never, at any point, whatever the table says.
        Fixture::Cancelled => Some(0),
        // Its subject is concurrency and its handler succeeds first time, so
        // the count is one per effect and says nothing about retry.
        Fixture::Capped => None,
        _ => Some(match restart {
            // The predecessor ran once and journaled nothing, so the successor
            // starts from zero and spends the whole budget. This is the
            // at-least-once boundary stated as a number: the record is what
            // makes an attempt remembered, and a killed process loses what it
            // had not written.
            Restart::BeforeRecord | Restart::MidConversation => attempts + 1,
            // One attempt is already journaled, so only the rest may run.
            Restart::AfterRecord | Restart::DuringBackoff => attempts.saturating_sub(1).max(1),
            // Every attempt but the last is journaled, and the predecessor ran
            // the last one and lost it — so the successor runs it once more.
            Restart::BeforeExhaustionAck => 2,
        }),
    }
}

fn side_file_runs(side_file: &Path) -> usize {
    fs::read_to_string(side_file)
        .map(|text| text.lines().filter(|line| !line.is_empty()).count())
        .unwrap_or(0)
}

fn run_one(seed: u64) {
    let restart = Restart::of(seed);
    let fixture = Fixture::of(seed);
    let directory = TestDirectory::create(&format!("seed-{seed}"));
    let side_file = directory.path().join("handler-runs.txt");
    let handlers = table(fixture, &directory, &side_file);
    let pending = triggered(fixture, &directory);

    let now_ms = die_at(restart, fixture, &directory, &handlers, &pending);
    let marks = resume(&directory, handlers, now_ms);
    let journal = observe(&directory);
    let where_ = format!("seed {seed} ({fixture:?} at {restart:?})");

    assert!(
        matches!(journal.health, JournalHealth::Ok),
        "{where_}: journal health {:?}",
        journal.health
    );

    for effect_id in &pending {
        let attempts = journal.attempts.get(effect_id).cloned().unwrap_or_default();
        // Gapless and strictly increasing from one. A gap would make the
        // derived count unreliable, and an unreliable count makes "three
        // tries" mean something different after a crash.
        let expected: Vec<u32> = (1..=attempts.len() as u32).collect();
        assert_eq!(attempts, expected, "{where_}: {effect_id} attempts");
        // The last attempt is acked rather than journaled, so the bound is one
        // below the budget — tighter than "at most `attempts`", and true.
        assert!(
            attempts.len() as u32 <= fixture.attempts().saturating_sub(1),
            "{where_}: {effect_id} has {} records for {} attempts",
            attempts.len(),
            fixture.attempts()
        );

        let acks = journal.acks.get(effect_id).cloned().unwrap_or_default();
        if fixture == Fixture::Cancelled {
            // Somebody decided this instance was over. No table resurrects it,
            // at any restart point, whatever `retry.on` says.
            assert!(acks.is_empty(), "{where_}: a cancelled effect was acked");
            assert!(
                attempts.is_empty(),
                "{where_}: a cancelled effect was retried"
            );
            continue;
        }
        // Exactly-once journaling, everywhere and always: two acks over one
        // derived key with different content is the collision the whole design
        // refuses, and a restart is where it would happen.
        assert_eq!(acks.len(), 1, "{where_}: {effect_id} acks");
        let result = acks[0].get("result").expect("an executor ack has a result");
        let exhausted =
            result.get("error").and_then(Value::as_str) == Some("exec/retries_exhausted");
        // If and only if: the cause is present exactly when the budget ran
        // out, so a reader can trust it in both directions.
        let spent = fixture.attempts() > 1
            && attempts.len() as u32 == fixture.attempts() - 1
            && acks[0].get("outcome").and_then(Value::as_str) == Some("failed");
        assert_eq!(exhausted, spent, "{where_}: {effect_id} result {result:?}");
        if exhausted {
            assert_eq!(
                result.get("attempts").and_then(Value::as_num),
                Some(fixture.attempts().to_string().as_str())
            );
        }
    }

    if let Some(bound) = max_runs(fixture, restart) {
        let runs = side_file_runs(&side_file);
        assert!(
            runs <= bound,
            "{where_}: the handler ran {runs} times, at most {bound} allowed"
        );
    }

    if fixture == Fixture::Capped {
        // A fresh scheduler respects the caps: they are about this process's
        // concurrency, and a restart does not exempt it from them.
        assert!(
            marks.inflight <= 4,
            "{where_}: {} started at once",
            marks.inflight
        );
        assert!(
            marks.per_instance <= 2,
            "{where_}: {} on one instance at once",
            marks.per_instance
        );
        assert_eq!(marks.served.len(), 6, "{where_}: {:?} served", marks.served);
    }
}

/// The committed iteration count.
///
/// **Measured, not chosen.** Each iteration spawns up to three handler
/// processes — and for the MCP fixture a full `fsm serve` — so this suite is
/// process-bound in the way `crash_harness.rs` is. Measured on this
/// workspace's Linux host:
///
/// | iterations | debug | release |
/// |---|---|---|
/// | 40 (committed) | 9.1 s | 2.3 s |
/// | 200 | 45.0 s | — |
///
/// `ci.yml` allows forty-five minutes per job across three operating systems
/// and two toolchains, and both profiles run. Two hundred iterations would
/// cost about a minute per Linux job and several times that on Windows, where
/// process creation is far costlier — against a budget `crash_harness.rs`
/// (1,000 spawns per profile) and `executor_chaos.rs` (200 iterations) already
/// dominate. Forty costs about twelve seconds per Linux job and still reaches
/// every restart point and every fixture, which a row below asserts.
///
/// So the depth lives behind the override, the pattern `FSM_CRASH_ITERS` and
/// `FSM_EXECUTOR_CHAOS_ITERS` already establish. Raise it locally, or in a
/// nightly job, rather than in the committed default.
fn iterations() -> u64 {
    std::env::var("FSM_POLICY_CHAOS_ITERS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map_or(ITERATIONS, |count| count.max(ITERATIONS))
}

#[test]
fn a_restart_inside_a_retry_sequence_leaves_the_journal_exact() {
    if let Ok(raw) = std::env::var("POLICY_CHAOS_SEED") {
        let seed = raw.parse::<u64>().expect("POLICY_CHAOS_SEED is a number");
        run_one(seed);
        return;
    }
    let mut state = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..iterations() {
        let seed = next_seed(&mut state);
        run_one(seed);
    }
}

#[test]
fn the_generator_is_deterministic_so_a_failure_can_be_replayed() {
    let mut first = 0x9E37_79B9_7F4A_7C15;
    let mut second = 0x9E37_79B9_7F4A_7C15;
    let a: Vec<u64> = (0..8).map(|_| next_seed(&mut first)).collect();
    let b: Vec<u64> = (0..8).map(|_| next_seed(&mut second)).collect();
    assert_eq!(a, b);
    assert_eq!(
        a.iter().collect::<BTreeSet<_>>().len(),
        a.len(),
        "a seed sequence that repeats would silently shrink the run"
    );
}

#[test]
fn every_restart_point_and_fixture_is_exercised_by_the_committed_run() {
    // A default too low to reach every combination would be a suite that
    // passes without covering what it claims.
    let mut state = 0x9E37_79B9_7F4A_7C15;
    let mut points = BTreeSet::new();
    let mut fixtures = BTreeSet::new();
    for _ in 0..ITERATIONS {
        let seed = next_seed(&mut state);
        points.insert(Restart::of(seed));
        fixtures.insert(Fixture::of(seed));
    }
    assert_eq!(points.len(), Restart::ALL.len(), "{points:?}");
    assert_eq!(fixtures.len(), Fixture::ALL.len(), "{fixtures:?}");
}

#[test]
fn an_instance_cancelled_mid_retry_is_never_tried_again() {
    // The reachable version of the rule the seeded loop covers at its own
    // points: attempt one failed and was journaled, and *then* somebody
    // cancelled the instance. A successor reads a journal that says "one
    // failed attempt, budget remaining" — and must still not run it, because
    // the cancellation is a decision about the whole instance and a retry
    // would spend the operator's budget undoing it.
    let directory = TestDirectory::create("cancelled-mid-retry");
    let side_file = directory.path().join("handler-runs.txt");
    let handlers = table(Fixture::Retrying, &directory, &side_file);
    let pending = triggered(Fixture::Retrying, &directory);
    let effect_id = &pending[0];

    journal_attempts(&directory, effect_id, "case-00", 1);
    {
        let mut store = open_writer(directory.path());
        let mut clock = FixedClock::new(BASE_MS + 2_000, 1);
        store
            .cancel_instance_reason_on(&mut clock, "case-00", "req-cancel-late", "")
            .expect("the instance cancels");
    }

    resume(&directory, handlers, BASE_MS + 2_000 + BACKOFF_MS * 8);

    let journal = observe(&directory);
    assert!(matches!(journal.health, JournalHealth::Ok));
    assert_eq!(
        journal.attempts.get(effect_id).map(Vec::len),
        Some(1),
        "the successor added an attempt to a cancelled instance"
    );
    assert!(
        journal.acks.get(effect_id).is_none(),
        "a cancelled instance's effect was acked by the successor"
    );
    // And the handler itself never ran again.
    let runs = side_file_runs(&side_file);
    assert_eq!(
        runs, 0,
        "the handler ran {runs} times after the cancellation"
    );
}
