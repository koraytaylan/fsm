//! Replay: does today's engine still agree with what was written down?
//!
//! Plan 0014 task 6603.

// `ErrorObj` is 192 bytes and every tool returns one; boxing it in a test
// helper would differ from the surface under test for no benefit.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fs;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::cancel::Cancellations;
use fsm_cli::mcp::notify::{Notifier, SharedSink};
use fsm_cli::mcp::tools::{MUTATING_TOOLS, ToolCtx, annotations, dispatch, dispatch_with};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-replay-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"replay_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch, events: usize) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "replay_case",
            "inst-r",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    for n in 0..events {
        store
            .send_event(
                "inst-r",
                "push",
                Value::Obj(BTreeMap::new()),
                &format!("push-{n}"),
                None,
            )
            .unwrap();
    }
    store
}

fn replay(store: &mut Store, args: &str) -> Result<Value, fsm_cli::store::ErrorObj> {
    dispatch(
        store,
        &mut FixedClock::new(2_000, 1),
        "journal_replay",
        &value(args),
    )
}

fn replay_dir(dir: &Scratch, args: &str) -> Value {
    fsm_cli::mcp::tools::replay_report(
        dir,
        &value(args),
        &mut FixedClock::new(2_000, 1),
        &fsm_cli::mcp::progress::ProgressReporter::discarding(),
        &fsm_cli::mcp::cancel::CancelFlag::default(),
    )
    .expect("a store that loads answers")
}

/// One reported field as text, whatever JSON type it arrived as.
fn field(report: &Value, name: &str) -> Option<String> {
    report.get(name).and_then(|v| match v {
        Value::Str(s) => Some(s.clone()),
        Value::Num(n) => Some(n.clone()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn segment(dir: &Scratch) -> std::path::PathBuf {
    dir.join("journal/seg-00000000000000000000.jsonl")
}

/// Rewrite the `state_hash` of the record at `nth` applied event, leaving
/// the chain and the canonical bytes intact — a store that verifies clean
/// and replays wrong.
fn tamper_state_hash(dir: &Scratch, wanted: usize) -> u64 {
    let text = fs::read_to_string(segment(dir)).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut seen = 0;
    let mut seq = 0;
    for line in lines.iter_mut() {
        let Some(at) = line.find("\"state_hash\":\"sha256:") else {
            continue;
        };
        if seen != wanted {
            seen += 1;
            continue;
        }
        // `sha256:` then 64 hex characters — replacing the hex keeps the
        // shape the record validator requires, so the store still loads and
        // only the *claim* is wrong.
        let start = at + "\"state_hash\":\"sha256:".len();
        let end = start + 64;
        let record = parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        seq = record
            .get("seq")
            .and_then(Value::as_num)
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap();
        line.replace_range(start..end, &"a".repeat(64));
        break;
    }
    fs::write(segment(dir), lines.join("\n") + "\n").unwrap();
    seq
}

#[test]
fn a_healthy_store_replays_to_a_root_an_independent_fold_agrees_with() {
    let dir = scratch("ok");
    let mut store = seeded(&dir, 4);
    let expected = store.records.len();
    let report = replay(&mut store, "{}").expect("a healthy store replays");
    assert_eq!(field(&report, "matches").as_deref(), Some("true"));
    assert_eq!(
        field(&report, "replayed_records").as_deref(),
        Some(expected.to_string().as_str())
    );

    // The same root, computed by folding the records here rather than
    // trusting the tool's own arithmetic.
    let records = fsm_cli::journal_io::load_records(&dir).unwrap();
    let folded = fsm_core::replay::fold_with(records, &mut fsm_core::replay::NopSink).unwrap();
    assert_eq!(
        field(&report, "state_root").as_deref(),
        Some(fsm_core::replay::state_root_at(&folded, folded.last_seq).as_str())
    );
}

#[test]
fn a_tampered_state_hash_is_named_by_its_seq() {
    let dir = scratch("tampered");
    let store = seeded(&dir, 4);
    drop(store);
    let seq = rehash_after_tamper(&dir, 1);
    let report = replay_dir(&dir, "{}");
    assert_eq!(field(&report, "matches").as_deref(), Some("false"));
    assert_eq!(
        field(&report, "first_divergence_seq").as_deref(),
        Some(seq.to_string().as_str())
    );
    assert!(
        field(&report, "message").is_some_and(|m| m.contains("replays as")),
        "the message says what was recorded and what the engine produces"
    );
}

#[test]
fn two_divergences_report_the_earlier_one() {
    // A difference propagates, so every later divergence is a consequence
    // and only the first is a clue.
    let dir = scratch("two");
    let store = seeded(&dir, 6);
    drop(store);
    let later = tamper_state_hash(&dir, 3);
    let earlier = rehash_after_tamper(&dir, 1);
    assert!(earlier < later);
    let report = replay_dir(&dir, "{}");
    assert_eq!(
        field(&report, "first_divergence_seq").as_deref(),
        Some(earlier.to_string().as_str())
    );
}

#[test]
fn a_prefix_replays_to_the_root_at_that_seq() {
    let dir = scratch("prefix");
    let mut store = seeded(&dir, 6);
    let whole = replay(&mut store, "{}").unwrap();
    let cut = store.records[3].seq;
    let prefix = replay(&mut store, &format!(r#"{{"to_seq":{cut}}}"#)).unwrap();
    assert_eq!(field(&prefix, "matches").as_deref(), Some("true"));
    assert_eq!(field(&prefix, "replayed_records").as_deref(), Some("4"));
    assert_ne!(
        field(&prefix, "state_root"),
        field(&whole, "state_root"),
        "a prefix is a different state, and its root says so"
    );

    // And that root is the root an independent fold of the same prefix gives
    // — which is what makes a bisection over a divergence possible.
    let records: Vec<_> = fsm_cli::journal_io::load_records(&dir)
        .unwrap()
        .into_iter()
        .filter(|r| r.seq <= cut)
        .collect();
    let folded = fsm_core::replay::fold_with(records, &mut fsm_core::replay::NopSink).unwrap();
    assert_eq!(
        field(&prefix, "state_root").as_deref(),
        Some(fsm_core::replay::state_root_at(&folded, folded.last_seq).as_str())
    );
}

#[test]
fn verify_and_replay_disagree_correctly() {
    // The case that justifies having both tools: the bytes are untouched and
    // the chain links, so verification is happy; the recorded outcome is not
    // the outcome the engine produces, so replay is not.
    let dir = scratch("disagree");
    let store = seeded(&dir, 4);
    drop(store);
    // Rewrite a `state_hash` *and* re-chain the journal, so the bytes verify.
    let seq = rehash_after_tamper(&dir, 1);

    let verified = fsm_cli::mcp::tools::verify_report(
        &dir,
        &value("{}"),
        &mut FixedClock::new(2_000, 1),
        &fsm_cli::mcp::progress::ProgressReporter::discarding(),
        &fsm_cli::mcp::cancel::CancelFlag::default(),
    )
    .unwrap();
    assert_eq!(
        field(&verified, "health").as_deref(),
        Some("Ok"),
        "the bytes and the chain are exactly as they should be"
    );

    let replayed = replay_dir(&dir, "{}");
    assert_eq!(
        field(&replayed, "matches").as_deref(),
        Some("false"),
        "and the engine still disagrees with what was recorded"
    );
    assert_eq!(
        field(&replayed, "first_divergence_seq").as_deref(),
        Some(seq.to_string().as_str())
    );
}

/// Tamper with a `state_hash` and rebuild the chain over it, so the store is
/// byte-clean and semantically wrong.
fn rehash_after_tamper(dir: &Scratch, wanted: usize) -> u64 {
    let seq = tamper_state_hash(dir, wanted);
    let text = fs::read_to_string(segment(dir)).unwrap();
    let mut out: Vec<String> = Vec::new();
    let mut prev = fsm_core::record::zeros();
    for line in text.lines() {
        let parsed = parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        let field = |name: &str| parsed.get(name).cloned().unwrap_or(Value::Null);
        // Sealed the way the store seals: same envelope, same domain tag, so
        // the chain is genuinely a chain rather than something that merely
        // looks like one.
        let sealed = fsm_core::record::seal(
            field("seq").as_num().unwrap().parse().unwrap(),
            field("ts").as_num().unwrap().parse().unwrap(),
            fsm_core::record::RecordKind::from_str(field("kind").as_str().unwrap()).unwrap(),
            field("body"),
            &prev,
        );
        prev = sealed.hash.clone();
        out.push(String::from_utf8(fsm_core::canon::canon_bytes(&sealed.to_value())).unwrap());
    }
    fs::write(segment(dir), out.join("\n") + "\n").unwrap();
    seq
}

#[test]
fn a_progress_token_is_reported_against_and_silence_is_kept_without_one() {
    let dir = scratch("progress");
    let mut store = seeded(&dir, fsm_cli::journal_io::BATCH as usize * 2);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let ctx = ToolCtx {
        notifier: Some(&notifier),
        meta: Some(value(r#"{"progressToken":"replay-token"}"#)),
        ..Default::default()
    };
    dispatch_with(
        &mut store,
        &mut FixedClock::new(1_000, 1_000),
        "journal_replay",
        &value("{}"),
        &ctx,
    )
    .expect("replayed");
    let written = sink.text();
    assert!(
        written
            .lines()
            .filter(|l| l.contains("notifications/progress"))
            .count()
            >= 2,
        "a long replay reports as it goes: {written}"
    );
    assert!(written.contains("replay complete"));

    let quiet = SharedSink::new();
    let quiet_notifier = Notifier::new(Box::new(quiet.writer()));
    let ctx = ToolCtx {
        notifier: Some(&quiet_notifier),
        ..Default::default()
    };
    dispatch_with(
        &mut store,
        &mut FixedClock::new(1_000, 1_000),
        "journal_replay",
        &value("{}"),
        &ctx,
    )
    .unwrap();
    assert!(quiet.text().is_empty(), "no token, no notifications");
}

#[test]
fn a_cancelled_replay_stops_at_a_record_boundary() {
    let dir = scratch("cancel");
    let mut store = seeded(&dir, fsm_cli::journal_io::BATCH as usize * 2);
    let id = Value::Str("replay-cancel".into());
    let mut cancellations = Cancellations::default();
    cancellations.cancel(&id);
    let ctx = ToolCtx {
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    let error = dispatch_with(
        &mut store,
        &mut FixedClock::new(1_000, 1),
        "journal_replay",
        &value("{}"),
        &ctx,
    )
    .expect_err("withdrawn");
    assert_eq!(error.code, "req/cancelled");
}

#[test]
fn it_writes_nothing_and_takes_no_lock() {
    let dir = scratch("nolock");
    let mut writer = seeded(&dir, 2);
    let before = fs::read(segment(&dir)).unwrap();
    let mut reader = Store::open_read_only(&dir).unwrap();
    let report = replay(&mut reader, "{}").expect("replayed beside a live writer");
    assert_eq!(field(&report, "matches").as_deref(), Some("true"));
    assert_eq!(
        fs::read(segment(&dir)).unwrap(),
        before,
        "replay wrote to the journal"
    );
    writer
        .send_event(
            "inst-r",
            "push",
            Value::Obj(BTreeMap::new()),
            "after-replay",
            None,
        )
        .expect("the lock was never taken");
}

#[test]
fn it_reads_and_does_not_write() {
    assert!(!MUTATING_TOOLS.contains(&"journal_replay"));
    let derived = annotations("journal_replay");
    assert_eq!(derived.get("readOnlyHint"), Some(&Value::Bool(true)));
    assert_eq!(derived.get("openWorldHint"), Some(&Value::Bool(false)));
    let dir = scratch("readonly");
    let store = seeded(&dir, 1);
    drop(store);
    let mut store = Store::open_read_only(&dir).unwrap();
    replay(&mut store, "{}").expect("a read-only server replays");
}
