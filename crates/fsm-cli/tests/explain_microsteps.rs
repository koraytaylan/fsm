//! The human `explain` shows a cascade as the sequence it was — one line per
//! reaction microstep — while `--json` keeps the structured sections.
//!
//! Plan 0009 task 4701.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;

const REACTIVE: &str = r#"{"format":"fsm.machine/1","name":"reactive","states":[{"name":"boot"},{"name":"idle"},{"name":"working","entry":{"raise":[{"event":"tick"}]}},{"name":"ticked"},{"name":"waiting"},{"name":"done","terminal":true}],"initial":"boot","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true},{"name":"settle","fields":[]}],"transitions":[{"from":"boot","to":"idle"},{"from":"idle","on":"go","to":"working"},{"from":"working","on":"tick","to":"ticked","do":[{"target":"n","value":"ctx.n + 1"}]},{"from":"ticked","to":"waiting"},{"from":"waiting","on":"settle","to":"done"}]}"#;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-cli-explain-microsteps-{}-{sequence}",
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

fn run(data_dir: &Path, arguments: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(arguments)
        .arg(format!("--data-dir={}", data_dir.display()))
        .env("FSM_CLOCK_MS", "1000")
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

/// A store holding one applied `go` whose macrostep cascaded twice; returns
/// the `event_applied` seq.
fn populate(data_dir: &Path) -> u64 {
    let mut store = Store::open(data_dir).unwrap();
    let definition = parse(REACTIVE.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    store.define_machine(definition, false, false).unwrap();
    store.create_instance("reactive", "i1", "c1", None).unwrap();
    store
        .send_event("i1", "go", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    store
        .records
        .iter()
        .find(|record| record.kind == RecordKind::EventApplied)
        .unwrap()
        .seq
}

#[test]
fn explain_renders_each_reaction_microstep_on_its_own_line() {
    let directory = TestDirectory::create();
    let seq = populate(directory.path());
    let human = run(
        directory.path(),
        &["explain".into(), "i1".into(), format!("--seq={seq}")],
    );
    let stdout = String::from_utf8_lossy(&human.stdout);
    assert!(
        human.status.success(),
        "{}",
        String::from_utf8_lossy(&human.stderr)
    );
    assert!(
        stdout.contains("→ microstep 1 (internal tick): working → ticked"),
        "{stdout}"
    );
    assert!(
        stdout.contains("→ microstep 2 (eventless): ticked → waiting"),
        "{stdout}"
    );

    let json = run(
        directory.path(),
        &[
            "explain".into(),
            "i1".into(),
            format!("--seq={seq}"),
            "--json".into(),
        ],
    );
    assert!(
        json.status.success(),
        "{}",
        String::from_utf8_lossy(&json.stderr)
    );
    let explained = parse(&json.stdout, &JsonLimits::DEFAULT).unwrap();
    let sections = explained
        .get("trace")
        .and_then(|trace| trace.get("microsteps"))
        .and_then(Value::as_arr)
        .unwrap();
    assert_eq!(sections.len(), 2);
    assert!(sections.iter().all(|s| s.get("candidates").is_some()));
}
