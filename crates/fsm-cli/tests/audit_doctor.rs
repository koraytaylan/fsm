//! What is wrong with this store, and the command a person would run.
//!
//! The remedy field is this plan's answer to not exposing `repair`: the
//! model diagnoses exactly and hands over the exact command, and somebody
//! with the authority to destroy things decides.
//!
//! Plan 0014 task 6604.

// `ErrorObj` is 192 bytes and every tool returns one; boxing it in a test
// helper would differ from the surface under test for no benefit.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fs;

use fsm_cli::clock::FixedClock;
use fsm_cli::journal_io::{JournalHealth, classify};
use fsm_cli::mcp::tools::{MUTATING_TOOLS, annotations, dispatch};
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
        "fsm-doctor-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"doctor_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch, events: usize) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "doctor_case",
            "inst-d",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    for n in 0..events {
        store
            .send_event(
                "inst-d",
                "push",
                Value::Obj(BTreeMap::new()),
                &format!("push-{n}"),
                None,
            )
            .unwrap();
    }
    store
}

fn doctor(store: &mut Store) -> Value {
    dispatch(
        store,
        &mut FixedClock::new(2_000, 1),
        "store_doctor",
        &value("{}"),
    )
    .expect("a diagnosis always answers")
}

fn field(report: &Value, name: &str) -> Option<String> {
    report.get(name).and_then(|v| match v {
        Value::Str(s) => Some(s.clone()),
        Value::Num(n) => Some(n.clone()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn nested(report: &Value, group: &str, name: &str) -> Option<String> {
    report.get(group).and_then(|g| field(g, name))
}

fn segment(dir: &Scratch) -> std::path::PathBuf {
    dir.join("journal/seg-00000000000000000000.jsonl")
}

#[test]
fn a_healthy_store_is_reported_healthy_and_has_nothing_to_repair() {
    let dir = scratch("ok");
    let mut store = seeded(&dir, 3);
    let expected = store.records.len();
    let report = doctor(&mut store);
    assert_eq!(field(&report, "health").as_deref(), Some("Ok"));
    assert_eq!(field(&report, "readable").as_deref(), Some("true"));
    assert_eq!(
        field(&report, "records").as_deref(),
        Some(expected.to_string().as_str())
    );
    assert_eq!(
        field(&report, "version").as_deref(),
        Some(fsm_cli::journal_io::STORE_VERSION)
    );
    assert!(
        report.get("remedy").is_none(),
        "a healthy store has nothing to repair, and offering a command would be noise"
    );
    // One segment, holding those records.
    let segments = report.get("segments").and_then(Value::as_arr).unwrap();
    assert_eq!(segments.len(), 1);
    assert_eq!(field(&segments[0], "status").as_deref(), Some("ok"));
    assert!(
        report.get("orphans").is_some(),
        "a readable store was asked"
    );
}

#[test]
fn a_torn_tail_hands_over_the_exact_command() {
    let dir = scratch("torn");
    let store = seeded(&dir, 3);
    drop(store);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(segment(&dir), &bytes).unwrap();
    assert!(matches!(classify(&dir), JournalHealth::TornTail { .. }));

    let report = fsm_cli::mcp::tools::doctor_report(&dir);
    assert_eq!(field(&report, "health").as_deref(), Some("TornTail"));
    // Verbatim from SPEC's recovery table. A paraphrase relayed to a human
    // is worse than nothing: they would run it.
    let remedy = field(&report, "remedy").expect("a torn tail has a repair");
    assert_eq!(remedy, "fsm repair --truncate-torn-tail");
    let spec = include_str!("../../../docs/SPEC.md");
    assert!(
        spec.contains(&remedy),
        "the remedy is not the string SPEC prescribes: {remedy}"
    );
}

#[test]
fn interior_damage_offers_no_command_at_all() {
    let dir = scratch("chain");
    let store = seeded(&dir, 4);
    drop(store);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    let position = bytes.iter().position(|b| *b == b'{').unwrap();
    bytes.insert(position + 1, b' ');
    fs::write(segment(&dir), &bytes).unwrap();

    let report = fsm_cli::mcp::tools::doctor_report(&dir);
    assert_ne!(field(&report, "health").as_deref(), Some("Ok"));
    assert!(
        report.get("remedy").is_none(),
        "SPEC says refuse, no repair — so there is no command to hand over"
    );
    assert_eq!(
        field(&report, "readable").as_deref(),
        Some("false"),
        "and the store cannot be opened, which is the case this tool is for"
    );
    assert!(
        report.get("orphans").is_none(),
        "an empty orphan list would read as 'checked, and none found'"
    );
}

#[test]
fn the_tool_and_the_command_line_agree_about_health() {
    for (tag, damage) in [("clean", false), ("torn", true)] {
        let dir = scratch(tag);
        let store = seeded(&dir, 2);
        drop(store);
        if damage {
            let mut bytes = fs::read(segment(&dir)).unwrap();
            bytes.truncate(bytes.len() - 3);
            fs::write(segment(&dir), &bytes).unwrap();
        }
        let report = fsm_cli::mcp::tools::doctor_report(&dir);
        // `fsm doctor` prints `format!("{:?}", health)`; the tool reports the
        // recovery table's name for the same value. One computation behind
        // both, so they cannot disagree about which it is.
        let health = classify(&dir);
        let named = field(&report, "health").unwrap();
        assert!(
            format!("{health:?}").starts_with(&named),
            "{tag}: tool says {named}, classifier says {health:?}"
        );
    }
}

#[test]
fn the_writer_lock_is_reported_while_somebody_holds_it() {
    let dir = scratch("lock");
    let writer = seeded(&dir, 1);
    let report = fsm_cli::mcp::tools::doctor_report(&dir);
    assert_eq!(
        nested(&report, "writer_lock", "held").as_deref(),
        Some("true"),
        "something else has the writer is the commonest non-fatal surprise"
    );
    assert_eq!(
        nested(&report, "writer_lock", "holder"),
        None,
        "the holder is this very process, which tells an operator nothing \
         they did not know — and naming it would make two identical runs \
         report different things"
    );
    drop(writer);

    let report = fsm_cli::mcp::tools::doctor_report(&dir);
    assert_eq!(
        nested(&report, "writer_lock", "held").as_deref(),
        Some("false")
    );
}

#[test]
fn a_snapshot_is_reported_present_and_how_far_behind() {
    let dir = scratch("snapshot");
    let mut store = seeded(&dir, 2);
    let report = doctor(&mut store);
    assert_eq!(
        nested(&report, "snapshot", "present").as_deref(),
        Some("false"),
        "no snapshot has been taken yet"
    );
    assert_eq!(
        nested(&report, "snapshot", "stale").as_deref(),
        Some("false")
    );

    store.shutdown_snapshot().expect("a snapshot is written");
    let report = doctor(&mut store);
    assert_eq!(
        nested(&report, "snapshot", "present").as_deref(),
        Some("true")
    );
    assert_eq!(
        nested(&report, "snapshot", "records_behind").as_deref(),
        Some("0"),
        "a snapshot taken just now is not behind"
    );
    assert_eq!(
        nested(&report, "snapshot", "stale").as_deref(),
        Some("false"),
        "and presence alone is not staleness"
    );
}

#[test]
fn it_answers_for_a_store_that_will_not_open() {
    // The degraded case: the store most in need of a diagnosis is the one
    // nothing can open.
    let dir = scratch("unopenable");
    let store = seeded(&dir, 2);
    drop(store);
    fs::write(segment(&dir), b"not a journal at all\n").unwrap();
    assert!(Store::open_read_only(&dir).is_err());

    let report = fsm_cli::mcp::tools::doctor_report(&dir);
    assert_eq!(field(&report, "readable").as_deref(), Some("false"));
    assert_ne!(field(&report, "health").as_deref(), Some("Ok"));
    assert!(
        field(&report, "message").is_some_and(|m| !m.is_empty()),
        "and it says what it found"
    );
}

#[test]
fn it_writes_nothing() {
    let dir = scratch("readonly");
    let mut store = seeded(&dir, 2);
    store.shutdown_snapshot().unwrap();
    drop(store);
    let before: Vec<(std::path::PathBuf, Vec<u8>)> = walk(&dir);
    let _ = fsm_cli::mcp::tools::doctor_report(&dir);
    let after = walk(&dir);
    assert_eq!(
        before.len(),
        after.len(),
        "a diagnosis created or removed a file"
    );
    for ((path, before), (_, after)) in before.iter().zip(after.iter()) {
        assert_eq!(before, after, "a diagnosis rewrote {}", path.display());
    }
}

/// Every file under a directory, with its bytes, sorted by path.
fn walk(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = fs::read(&path) {
                // The lock file's bytes change when a writer takes it, which
                // is not this tool writing.
                if path.file_name().is_some_and(|name| name == "LOCK") {
                    continue;
                }
                out.push((path, bytes));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn it_reads_and_does_not_write() {
    assert!(!MUTATING_TOOLS.contains(&"store_doctor"));
    let derived = annotations("store_doctor");
    assert_eq!(derived.get("readOnlyHint"), Some(&Value::Bool(true)));
    assert_eq!(derived.get("openWorldHint"), Some(&Value::Bool(false)));
    let dir = scratch("annotations");
    let store = seeded(&dir, 1);
    drop(store);
    let mut store = Store::open_read_only(&dir).unwrap();
    doctor(&mut store);
}
