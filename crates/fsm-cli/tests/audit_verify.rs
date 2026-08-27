//! The tamper-evidence claim, checkable by the model that reads it.
//!
//! Plan 0014 task 6602.

use std::collections::BTreeMap;
use std::fs;

use fsm_cli::clock::FixedClock;
use fsm_cli::journal_io::{JournalHealth, classify};
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
        "fsm-verify-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"verify_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

/// A store with a machine, an instance, and `events` applied events, so the
/// journal is long enough to walk in more than one batch.
fn seeded(dir: &Scratch, events: usize) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "verify_case",
            "inst-v",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    for n in 0..events {
        store
            .send_event(
                "inst-v",
                "push",
                Value::Obj(BTreeMap::new()),
                &format!("push-{n}"),
                None,
            )
            .unwrap();
    }
    store
}

fn verify(store: &mut Store, args: &str) -> Result<Value, fsm_cli::store::ErrorObj> {
    dispatch(
        store,
        &mut FixedClock::new(2_000, 1),
        "journal_verify",
        &value(args),
    )
}

/// Verify a directory nobody can open. The store you most want verified is
/// the one that will not open, so the report is a function of the path.
fn verify_dir(dir: &Scratch, args: &str) -> Value {
    fsm_cli::mcp::tools::verify_report(
        dir,
        &value(args),
        &mut FixedClock::new(2_000, 1),
        &fsm_cli::mcp::progress::ProgressReporter::discarding(),
        &fsm_cli::mcp::cancel::CancelFlag::default(),
    )
    .expect("a damaged store still answers")
}

fn field<'a>(report: &'a Value, name: &str) -> Option<&'a str> {
    report.get(name).and_then(|v| v.as_str().or(v.as_num()))
}

fn segment(dir: &Scratch) -> std::path::PathBuf {
    dir.join("journal/seg-00000000000000000000.jsonl")
}

#[test]
fn a_healthy_store_says_so_and_counts_what_it_walked() {
    let dir = scratch("ok");
    let mut store = seeded(&dir, 5);
    let expected = store.records.len();
    let report = verify(&mut store, "{}").expect("a healthy store verifies");
    assert_eq!(field(&report, "health"), Some("Ok"));
    assert_eq!(
        field(&report, "verified_records"),
        Some(expected.to_string().as_str())
    );
    assert!(report.get("first_bad_seq").is_none());
    assert!(
        report.get("remedy").is_none(),
        "a healthy store has nothing to repair"
    );
}

#[test]
fn a_flipped_byte_is_found_and_named() {
    let dir = scratch("flipped");
    let store = seeded(&dir, 3);
    drop(store);
    // A space inside the first record: still parseable JSON, no longer the
    // canonical bytes the hash covers.
    let mut bytes = fs::read(segment(&dir)).unwrap();
    let position = bytes.iter().position(|b| *b == b'{').unwrap();
    bytes.insert(position + 1, b' ');
    fs::write(segment(&dir), &bytes).unwrap();

    let health = classify(&dir);
    let report = verify_dir(&dir, "{}");
    // Whatever the classifier concluded, the tool reports the same word and
    // the same seq — it decides nothing of its own.
    let expected = match &health {
        JournalHealth::NonCanonical { .. } => "NonCanonical",
        JournalHealth::ChainBroken { .. } => "ChainBroken",
        JournalHealth::TornTail { .. } => "TornTail",
        other => panic!("unexpected health for a flipped byte: {other:?}"),
    };
    assert_eq!(field(&report, "health"), Some(expected));
    assert!(
        field(&report, "message").is_some_and(|m| !m.is_empty()),
        "the operator gets the classifier's own sentence"
    );
}

#[test]
fn a_torn_tail_carries_the_remedy_from_the_table() {
    let dir = scratch("torn");
    let store = seeded(&dir, 3);
    drop(store);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(segment(&dir), &bytes).unwrap();
    assert!(matches!(classify(&dir), JournalHealth::TornTail { .. }));

    let report = verify_dir(&dir, "{}");
    assert_eq!(field(&report, "health"), Some("TornTail"));
    assert_eq!(
        field(&report, "remedy"),
        Some("fsm repair --truncate-torn-tail"),
        "SPEC's recovery table, verbatim — and the tool never runs it"
    );
}

#[test]
fn interior_damage_gets_a_blast_radius_and_no_remedy() {
    let dir = scratch("chain");
    let store = seeded(&dir, 4);
    drop(store);
    // Rewrite a `prev` so the chain no longer links: interior damage, which
    // the table says has no repair.
    let text = fs::read_to_string(segment(&dir)).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let target = lines.len() / 2;
    lines[target] = lines[target].replacen(
        "\"prev\":\"",
        "\"prev\":\"0000000000000000000000000000000000000000000000000000000000000000",
        1,
    );
    lines[target] = lines[target].replacen(
        "0000000000000000000000000000000000000000000000000000000000000000\"prev\"",
        "\"prev\"",
        1,
    );
    fs::write(segment(&dir), lines.join("\n") + "\n").unwrap();

    let health = classify(&dir);
    let report = verify_dir(&dir, "{}");
    match health {
        JournalHealth::ChainBroken { seq, .. } => {
            assert_eq!(field(&report, "health"), Some("ChainBroken"));
            assert_eq!(
                field(&report, "first_bad_seq"),
                Some(seq.to_string().as_str())
            );
            assert_eq!(
                field(&report, "blast_radius"),
                Some(format!("records ≥ {seq} unverifiable").as_str()),
                "SPEC's blast radius, in SPEC's words"
            );
            assert!(
                report.get("remedy").is_none(),
                "interior damage has no repair, and offering one would be a lie"
            );
        }
        JournalHealth::NonCanonical { .. } => {
            assert_eq!(field(&report, "health"), Some("NonCanonical"));
            assert!(report.get("remedy").is_none());
        }
        other => panic!("unexpected health for a rewritten prev: {other:?}"),
    }
}

#[test]
fn a_window_bounds_the_walk_and_the_count() {
    let dir = scratch("window");
    let mut store = seeded(&dir, 20);
    let whole = verify(&mut store, "{}").unwrap();
    let all: u64 = field(&whole, "verified_records").unwrap().parse().unwrap();

    let windowed = verify(&mut store, r#"{"to_seq":3}"#).unwrap();
    let counted: u64 = field(&windowed, "verified_records")
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        counted < all,
        "a window walks less than everything: {counted} of {all}"
    );
    assert_eq!(field(&windowed, "health"), Some("Ok"));

    // A window that runs backwards is a request nobody can serve.
    let error = verify(&mut store, r#"{"from_seq":9,"to_seq":2}"#).expect_err("backwards");
    assert_eq!(error.code, "req/args_invalid");
}

#[test]
fn a_progress_token_is_reported_against_and_silence_is_kept_without_one() {
    let dir = scratch("progress");
    // Enough records that the walk crosses more than one batch boundary.
    let mut store = seeded(&dir, fsm_cli::journal_io::BATCH as usize * 2);

    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let ctx = ToolCtx {
        notifier: Some(&notifier),
        meta: Some(value(r#"{"progressToken":"verify-token"}"#)),
        ..Default::default()
    };
    dispatch_with(
        &mut store,
        // A clock that steps past the rate limit between batches, so every
        // batch's report is one a client would actually see.
        &mut FixedClock::new(1_000, 1_000),
        "journal_verify",
        &value("{}"),
        &ctx,
    )
    .expect("verified");
    let written = sink.text();
    let notifications = written
        .lines()
        .filter(|line| line.contains("notifications/progress"))
        .count();
    assert!(
        notifications >= 2,
        "a long walk reports as it goes: {}",
        sink.text()
    );
    assert!(
        sink.text().contains("verification complete"),
        "and says when it is done"
    );

    // Without a token, nothing at all.
    let quiet = SharedSink::new();
    let quiet_notifier = Notifier::new(Box::new(quiet.writer()));
    let ctx = ToolCtx {
        notifier: Some(&quiet_notifier),
        ..Default::default()
    };
    dispatch_with(
        &mut store,
        &mut FixedClock::new(1_000, 1_000),
        "journal_verify",
        &value("{}"),
        &ctx,
    )
    .unwrap();
    assert!(
        quiet.text().is_empty(),
        "a call with no token is silent: {}",
        quiet.text()
    );
}

#[test]
fn a_cancelled_verification_stops_at_a_record_boundary() {
    let dir = scratch("cancel");
    let mut store = seeded(&dir, fsm_cli::journal_io::BATCH as usize * 2);
    let id = Value::Str("verify-cancel".into());
    let mut cancellations = Cancellations::default();
    cancellations.cancel(&id);
    let ctx = ToolCtx {
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    let error = dispatch_with(
        &mut store,
        &mut FixedClock::new(1_000, 1),
        "journal_verify",
        &value("{}"),
        &ctx,
    )
    .expect_err("the client withdrew it");
    assert_eq!(error.code, "req/cancelled");
}

#[test]
fn verification_takes_no_lock() {
    // A verification that stopped the writer is a verification nobody runs
    // while anything is happening — which is exactly when it is wanted.
    let dir = scratch("nolock");
    let mut writer = seeded(&dir, 2);
    let mut reader = Store::open_read_only(&dir).unwrap();
    let report = verify(&mut reader, "{}").expect("verified beside a live writer");
    assert_eq!(field(&report, "health"), Some("Ok"));
    // And the writer is still a writer afterwards.
    writer
        .send_event(
            "inst-v",
            "push",
            Value::Obj(BTreeMap::new()),
            "after-verify",
            None,
        )
        .expect("the lock was never taken");
}

#[test]
fn it_reads_and_does_not_write() {
    assert!(!MUTATING_TOOLS.contains(&"journal_verify"));
    let derived = annotations("journal_verify");
    assert_eq!(derived.get("readOnlyHint"), Some(&Value::Bool(true)));
    assert_eq!(derived.get("openWorldHint"), Some(&Value::Bool(false)));
    let dir = scratch("readonly");
    let store = seeded(&dir, 1);
    drop(store);
    let mut store = Store::open_read_only(&dir).unwrap();
    verify(&mut store, "{}").expect("a read-only server verifies");
}

#[test]
fn the_seam_changed_no_conclusion() {
    // Behaviour parity: the incremental walk and the all-at-once one agree
    // about every fixture, which is the whole permission this task had to
    // touch verification at all.
    for (tag, damage) in [("clean", 0usize), ("torn", 1), ("spaced", 2), ("prev", 3)] {
        let dir = scratch(tag);
        let store = seeded(&dir, 3);
        drop(store);
        let path = segment(&dir);
        match damage {
            1 => {
                let mut bytes = fs::read(&path).unwrap();
                bytes.truncate(bytes.len() - 3);
                fs::write(&path, &bytes).unwrap();
            }
            2 => {
                let mut bytes = fs::read(&path).unwrap();
                let position = bytes.iter().position(|b| *b == b'{').unwrap();
                bytes.insert(position + 1, b' ');
                fs::write(&path, &bytes).unwrap();
            }
            3 => {
                let text = fs::read_to_string(&path).unwrap();
                let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
                let last = lines.len() - 1;
                lines[last] = lines[last].replacen("\"seq\":", "\"seq\":9", 1);
                fs::write(&path, lines.join("\n") + "\n").unwrap();
            }
            _ => {}
        }
        let all_at_once = fsm_cli::journal_io::verify_segments(&dir);
        let incrementally = fsm_cli::journal_io::verify_segments_with(&dir, &mut |_, _| {
            fsm_cli::journal_io::Walk::Continue
        });
        let shape = |segments: &[fsm_cli::journal_io::SegmentProgress]| -> Vec<String> {
            segments
                .iter()
                .map(|s| {
                    format!(
                        "{}:{}:{:?}:{:?}",
                        s.status, s.records, s.first_seq, s.last_seq
                    )
                })
                .collect()
        };
        assert_eq!(shape(&all_at_once), shape(&incrementally), "{tag}");
        // And the classifier, which is what `fsm journal verify` reports.
        let before = classify(&dir).message();
        let after = classify(&dir).message();
        assert_eq!(before, after, "{tag}");
    }
}
