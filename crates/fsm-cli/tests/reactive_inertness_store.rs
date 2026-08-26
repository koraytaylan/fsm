//! The store-level leg of the reactive plan's compatibility contract: a
//! journal the pre-plan build wrote folds to the hashes that build computed,
//! and records appended today to that non-reactive machine are the bytes that
//! build would have written. The fixture was written by the pre-plan build
//! (commit c5b5620) and is not what gets edited when this suite fails.
//!
//! Plan 0009 task 4603.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::journal_io::STORE_VERSION;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, limits_value, verify_line, zeros};
use fsm_core::replay::state_root_at;

const JOURNAL: &[u8] = include_bytes!("fixtures/inertness/preplan_session.journal");
const META: &[u8] = include_bytes!("fixtures/inertness/preplan_session.meta.json");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-cli-reactive-inertness-{}-{sequence}",
            std::process::id()
        ));
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

fn meta(key: &str) -> String {
    parse(META, &JsonLimits::DEFAULT)
        .unwrap()
        .get(key)
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            Value::Num(n) => n.clone(),
            other => panic!("{other:?}"),
        })
        .unwrap()
}

/// The journal's lines, each with its newline.
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split_inclusive(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .collect()
}

/// A data directory holding the first `count` records of the fixture, as
/// the pre-plan build left them on disk.
fn store_with_prefix(directory: &Path, count: usize) -> Store {
    let journal = directory.join("journal");
    fs::create_dir(&journal).unwrap();
    let bytes: Vec<u8> = lines(JOURNAL)[..count].concat();
    fs::write(journal.join("seg-00000000000000000000.jsonl"), bytes).unwrap();
    fs::write(directory.join("VERSION"), format!("{STORE_VERSION}\n")).unwrap();
    Store::open(directory).unwrap()
}

#[test]
fn the_pre_plan_journal_folds_to_its_hashes_and_continues_byte_for_byte() {
    let directory = TestDirectory::create();
    let prefix: usize = meta("prefix_records").parse().unwrap();
    let mut store = store_with_prefix(directory.path(), prefix);
    assert_eq!(store.state.last_seq.to_string(), meta("last_seq"));
    assert_eq!(store.state.last_hash, meta("last_hash"));
    assert_eq!(
        state_root_at(&store.state, store.state.last_seq),
        meta("state_root")
    );

    // The same continuation the pre-plan build wrote, appended today.
    let mut clock = FixedClock::new(20_000, 1_000);
    store
        .send_event_stamp_on(
            &mut clock,
            "inst-1",
            "docs_ok",
            &mut Value::Obj(BTreeMap::new()),
            "send-2",
            None,
            &[],
        )
        .unwrap();
    let mut payload = Value::Obj(BTreeMap::from([(
        "score".to_string(),
        Value::Str("800".into()),
    )]));
    store
        .send_event_stamp_on(
            &mut clock,
            "inst-1",
            "scored",
            &mut payload,
            "send-3",
            None,
            &[],
        )
        .unwrap();
    let _ = store.poll_instance_deadline_on(&mut clock, "inst-1", "poll-2", None);
    let written: Vec<u8> = store.records.iter().flat_map(Record::to_line).collect();
    assert!(
        written == JOURNAL,
        "records appended to a non-reactive machine are not the bytes the pre-plan build wrote"
    );
    assert!(
        !String::from_utf8_lossy(&written).contains("microsteps"),
        "no record of a non-reactive machine carries the key"
    );
    let mut prev = zeros();
    for (seq, line) in lines(&written).iter().enumerate() {
        prev = verify_line(line, seq as u64, &prev).unwrap().hash;
    }
}

#[test]
fn the_genesis_limits_block_is_the_pre_plan_block() {
    let genesis = parse(lines(JOURNAL)[0], &JsonLimits::DEFAULT).unwrap();
    let committed = genesis.get("body").and_then(|b| b.get("limits")).unwrap();
    assert_eq!(committed, &limits_value());
    let keys: Vec<&String> = committed.as_obj().unwrap().keys().collect();
    assert!(
        keys.iter()
            .all(|key| !key.contains("microstep") && !key.contains("raise")),
        "{keys:?}"
    );
}
