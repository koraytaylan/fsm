//! Restart the executor at every point where it could be interrupted, and
//! assert what the journal says afterwards.
//!
//! The word is **restart**, not `kill -9`: real signal-kill coverage of the
//! journal already lives in `crash_harness.rs`, and what this harness proves
//! is different — that a fresh executor's *journal-derived* decisions resume
//! correctly with nothing carried in memory. Death is simulated by dropping
//! the runner, scheduler, and pipeline and building fresh ones against the
//! same data directory.
//!
//! The invariant is stated in the shape the design actually guarantees:
//! **at-least-once execution, exactly-once journaling**. "The handler never
//! runs twice" is false at every death point *before the ack* — including the
//! one where the predecessor had already reaped its child, since reaping puts
//! the outcome in memory and a restart is precisely what loses memory. The
//! line runs through the ack, not through the reap: what the journal knows,
//! the successor honours; what it does not, the successor repeats.
//!
//! The seeded generator below is a deliberate ~20-line duplication of the
//! xorshift64\* in `chaos.rs` and `proputil.rs`, for the reason documented
//! there: a bug in one generator must not hide in the other.
//!
//! The recording stub is this test binary re-executed, as in
//! `crash_harness.rs`. It appends one line to a side file each time it runs,
//! and that path arrives as an **argument** rather than an environment
//! variable — the runner deliberately has no API for setting a child's
//! environment, and `std::env::set_var` is unsafe in this edition. The side
//! file is the double-run detector.
//!
//! Budget note for whoever raises the iteration count: the 45-minute CI
//! ceiling is already dominated by `crash_harness.rs`'s 1,000 spawns per
//! profile, run across two profiles and three operating systems, and Windows
//! process creation is several times costlier than Unix.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::journal_io::{JournalHealth, verify};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_execute::config::HandlerTable;
use fsm_execute::rid::ack_rid;
use fsm_execute::run::{Pipeline, RunOutcome, Runner};
use fsm_execute::sched::Scheduler;
use fsm_execute::service::tick;
use fsm_execute::watch::Watcher;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

/// Never lower this floor.
const ITERATIONS: u64 = 200;
const BASE_MS: i64 = 1_700_000_000_000;
/// The machine's deadline fires 30 seconds after the instance is created.
const DEADLINE_MS: i64 = 30_000;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Where a restart lands, relative to the two writes that settle an effect.
// The shared `After` prefix is the type's meaning, not an accident of naming:
// every variant is a point *after* one specific write, and a name that dropped
// it would read as the write itself rather than the instant behind it.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeathPoint {
    /// After the handler was spawned, before anyone reaped it.
    AfterSpawn,
    /// After the run was reaped, before the ack reached the journal.
    AfterReap,
    /// After the ack, before the advance event was sent.
    AfterAck,
    /// After a deadline poll was journaled.
    AfterPoll,
}

impl DeathPoint {
    fn of(seed: u64) -> Self {
        match seed % 4 {
            0 => DeathPoint::AfterSpawn,
            1 => DeathPoint::AfterReap,
            2 => DeathPoint::AfterAck,
            _ => DeathPoint::AfterPoll,
        }
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-executor-chaos-{test_name}-{}-{sequence}",
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
/// copy of every open descriptor — so an advisory lock dropped a moment ago
/// can still be held for the length of that window. The property under test is
/// that the executor does not *keep* the lock, not that a fork never happened.
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

/// The recording stub: append one line, exit zero.
#[test]
fn stub_handler() {
    let Some(side_file) = std::env::args().find_map(|argument| {
        argument
            .strip_prefix("stub:record:")
            .map(std::string::ToString::to_string)
    }) else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&side_file)
    {
        let _ = file.write_all(b"ran\n");
    }
    std::process::exit(0);
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
            "name":"order_confirmation_chaos",
            "context":[{"name":"order_id","ty":"str","init":"order-7"}],
            "events":[
                {"name":"submit","fields":[]},
                {"name":"confirmed","fields":[{"name":"at","ty":"timestamp"}]},
                {"name":"confirmation_failed","fields":[]}
            ],
            "effects":[{"name":"request_confirmation","fields":[{"name":"order","ty":"str"}]}],
            "states":[
                {"name":"placed"},
                {"name":"awaiting_confirmation","entry":{"emit":[
                    {"effect":"request_confirmation","args":{"order":"ctx.order_id"}}
                ]}},
                {"name":"confirmed_order","terminal":true},
                {"name":"unconfirmed","terminal":true}
            ],
            "initial":"placed",
            "transitions":[
                {"from":"placed","on":"submit","to":"awaiting_confirmation"},
                {"from":"awaiting_confirmation","on":"confirmed","to":"confirmed_order"},
                {"from":"awaiting_confirmation","on":"confirmation_failed","to":"unconfirmed"}
            ],
            "deadlines":[{
                "name":"confirmation_timeout",
                "from":"awaiting_confirmation",
                "after":"dur(30, s)",
                "to":"unconfirmed"
            }]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn table(side_file: &Path) -> HandlerTable {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        .into_owned();
    HandlerTable::parse(&format!(
        r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"request_confirmation","argv":["{}","stub_handler","--exact","--nocapture","stub:record:{}"],"timeout_ms":30000,"on_ok":{{"event":"confirmed","payload":{{}},"stamps":["at"]}},"on_failed":{{"event":"confirmation_failed"}}}}]}}"#,
        escaped(&stub),
        escaped(&side_file.to_string_lossy())
    ))
    .expect("the recording table validates")
}

fn escaped(path: &str) -> String {
    path.replace('\\', "\\\\")
}

/// Trigger one workflow and release the writer.
fn triggered(directory: &TestDirectory) -> String {
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(BASE_MS, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation_chaos",
            "order-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "order-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    let effect_id = store.state.instances["order-1"].pending[0].clone();
    drop(store);
    effect_id
}

/// Kill the executor at one point, by doing exactly what it would have done up
/// to there and then abandoning every component it owned.
fn die_at(point: DeathPoint, directory: &TestDirectory, effect_id: &str, table: &HandlerTable) {
    let mut runner = Runner::new().unwrap();
    match point {
        DeathPoint::AfterSpawn | DeathPoint::AfterReap => {
            let handler = &table.handlers["request_confirmation"];
            let argv = fsm_execute::config::substitute(
                &handler.argv,
                &BTreeMap::from([(
                    "order".to_string(),
                    fsm_core::expr::eval::Val::Str("order-7".into()),
                )]),
            )
            .unwrap();
            runner.spawn(effect_id.to_string(), &argv, None).unwrap();
            if point == DeathPoint::AfterReap {
                // Reaped, its output collected, and then the process is gone
                // before a single byte of that outcome reached the journal.
                for _ in 0..400 {
                    if runner.poll(effect_id).is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
        DeathPoint::AfterAck => {
            // The ack is journaled under the key the executor derives, and the
            // advance never happens. This is the interruption the resume rule
            // exists for.
            let mut store = open_writer(directory.path());
            let mut clock = FixedClock::new(BASE_MS + 1_000, 1);
            store
                .ack_effect_outcome_on(
                    &mut clock,
                    "order-1",
                    effect_id,
                    &ack_rid(effect_id),
                    "ok",
                    Some(
                        RunOutcome::Completed {
                            status: 0,
                            stdout: fsm_execute::run::BoundedBytes::empty(),
                            stderr: fsm_execute::run::BoundedBytes::empty(),
                        }
                        .ack_result(),
                    ),
                )
                .unwrap();
        }
        DeathPoint::AfterPoll => {
            // A due deadline poll lands, and the process dies before the tick
            // that would have observed its result.
            let mut store = open_writer(directory.path());
            let mut clock = FixedClock::new(BASE_MS + DEADLINE_MS + 5_000, 1);
            let due_ms = store.state.instances["order-1"].deadlines["confirmation_timeout"];
            let _ = Pipeline.poll(
                &mut store,
                &mut clock,
                "order-1",
                "confirmation_timeout",
                due_ms,
            );
        }
    }
    // Everything this life held is dropped here: the runner, and with it any
    // child it started.
    drop(runner);
}

/// Run a fresh executor to completion against a store somebody else started.
fn resume(directory: &TestDirectory, table: HandlerTable, now_ms: i64) {
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&table),
    );
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(now_ms, 1);
    for _ in 0..60 {
        let tick_now = clock.now;
        tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut clock,
            tick_now,
        );
        let store = Store::open_read_only(directory.path()).unwrap();
        let instance = &store.state.instances["order-1"];
        let settled = instance.pending.is_empty();
        let terminal = instance.status.as_str() != "running";
        drop(store);
        if settled && terminal {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

struct Outcome {
    acks: BTreeMap<String, usize>,
    advances: BTreeMap<(String, String), usize>,
    status: String,
    pending: Vec<String>,
    health: JournalHealth,
}

fn observe(directory: &TestDirectory) -> Outcome {
    let report = verify(directory.path());
    let store = Store::open_read_only(directory.path()).unwrap();
    let mut acks: BTreeMap<String, usize> = BTreeMap::new();
    let mut advances: BTreeMap<(String, String), usize> = BTreeMap::new();
    for record in &store.records {
        match record.kind {
            RecordKind::EffectAcked => {
                if let Some(effect_id) = record.body.get("effect_id").and_then(Value::as_str) {
                    *acks.entry(effect_id.to_string()).or_default() += 1;
                }
            }
            RecordKind::EventApplied => {
                // The executor's advances are the ones carrying a derived key;
                // the writer half's `submit` is not one of them.
                let request_id = record
                    .body
                    .get("request_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(rest) = request_id.strip_prefix("exec-ev-")
                    && let Some((effect_id, event)) = rest.rsplit_once('-')
                {
                    *advances
                        .entry((effect_id.to_string(), event.to_string()))
                        .or_default() += 1;
                }
            }
            _ => {}
        }
    }
    let instance = &store.state.instances["order-1"];
    Outcome {
        acks,
        advances,
        status: instance.status.as_str().to_string(),
        pending: instance.pending.clone(),
        health: report.health,
    }
}

fn side_file_runs(side_file: &Path) -> usize {
    fs::read_to_string(side_file)
        .map(|text| text.lines().filter(|line| !line.is_empty()).count())
        .unwrap_or(0)
}

fn run_one(seed: u64) {
    let point = DeathPoint::of(seed);
    let directory = TestDirectory::create(&format!("seed-{seed}"));
    let side_file = directory.path().join("handler-runs.txt");
    let table = table(&side_file);
    let effect_id = triggered(&directory);

    die_at(point, &directory, &effect_id, &table);

    // A fresh executor, with an empty in-flight map, reading the same journal.
    // For the deadline point its clock is past the due time, which is what put
    // the poll in the journal in the first place.
    let now_ms = match point {
        DeathPoint::AfterPoll => BASE_MS + DEADLINE_MS + 6_000,
        _ => BASE_MS + 1_000,
    };
    resume(&directory, table, now_ms);

    let outcome = observe(&directory);
    assert!(
        matches!(outcome.health, JournalHealth::Ok),
        "seed {seed} ({point:?}): journal health {:?}",
        outcome.health
    );
    for (effect, count) in &outcome.acks {
        assert_eq!(
            *count, 1,
            "seed {seed} ({point:?}): {effect} was acked {count} times"
        );
    }
    for ((effect, event), count) in &outcome.advances {
        assert_eq!(
            *count, 1,
            "seed {seed} ({point:?}): {effect} advanced with {event} {count} times"
        );
    }
    let runs = side_file_runs(&side_file);
    match point {
        // Before the ack, nothing about the run is in the journal, so a fresh
        // executor cannot know it happened: it sees a pending effect with an
        // unclaimed key and runs the handler again. That is true whether the
        // predecessor died before reaping its child or just after — reaping
        // puts the outcome in *memory*, and memory is what a restart loses.
        // This is the at-least-once boundary, and the assertion says so
        // instead of asserting it away. What must still hold is one ack.
        DeathPoint::AfterSpawn | DeathPoint::AfterReap => assert!(
            runs <= 2,
            "seed {seed} ({point:?}): the handler ran {runs} times"
        ),
        // After the ack, the journal knows. A successor that ran the handler
        // again here would be re-doing work the world has already seen.
        DeathPoint::AfterAck | DeathPoint::AfterPoll => assert!(
            runs <= 1,
            "seed {seed} ({point:?}): the handler ran {runs} times"
        ),
    }
    let coherent = outcome.status != "running"
        || outcome
            .pending
            .iter()
            .all(|effect_id| effect_id.starts_with("order-1/"));
    assert!(
        coherent,
        "seed {seed} ({point:?}): instance ended {} with {:?}",
        outcome.status, outcome.pending
    );
}

fn iterations() -> u64 {
    std::env::var("FSM_EXECUTOR_CHAOS_ITERS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map_or(ITERATIONS, |count| count.max(ITERATIONS))
}

#[test]
fn a_restart_at_every_point_leaves_exactly_one_ack_per_effect() {
    if let Ok(raw) = std::env::var("EXECUTOR_CHAOS_SEED") {
        let seed = raw.parse::<u64>().expect("EXECUTOR_CHAOS_SEED is a number");
        run_one(seed);
        return;
    }
    let mut state = 0x2545_F491_4F6C_DD1D;
    for _ in 0..iterations() {
        let seed = next_seed(&mut state);
        run_one(seed);
    }
}

#[test]
fn the_generator_is_deterministic_so_a_failure_can_be_replayed() {
    let mut first = 0x2545_F491_4F6C_DD1D;
    let mut second = 0x2545_F491_4F6C_DD1D;
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
fn each_named_death_point_is_exercised_at_least_once() {
    let mut state = 0x2545_F491_4F6C_DD1D;
    let mut seen = BTreeSet::new();
    for _ in 0..ITERATIONS {
        seen.insert(format!("{:?}", DeathPoint::of(next_seed(&mut state))));
    }
    assert_eq!(
        seen.len(),
        4,
        "every death point must actually occur: {seen:?}"
    );
}
