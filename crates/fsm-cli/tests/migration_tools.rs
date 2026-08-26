//! The dry run is the important half of this surface: an operator or a model
//! asks what a migration would do from a read-only server, and only then
//! decides to write.
//!
//! Plan 0011 task 5503.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{MUTATING_TOOLS, dispatch, registry, validate_args};
use fsm_cli::store::Store;
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-migtool-{}-{n}", std::process::id()));
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

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect(),
    )
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

const OLD: &str = r#"{"format":"fsm.machine/1","name":"case_v1","states":[{"name":"intake"},{"name":"stuck"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"1"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"stuck"}]}"#;

fn new_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"case_v2","states":[{{"name":"intake"}},{{"name":"stuck"}}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"stuck"}}],"supersedes":{{"machine":"{}","states":{{"intake":"intake"}},"context":{{"score":"ctx.score + 5"}}}}}}"#,
        digest(OLD)
    )
}

/// A store with both definitions and one instance on the old one.
fn ready(directory: &TestDirectory) -> (Store, FixedClock) {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for source in [OLD.to_string(), new_source()] {
        dispatch(
            &mut store,
            &mut clock,
            "machine_create",
            &obj(&[("spec", value(&source))]),
        )
        .unwrap();
    }
    dispatch(
        &mut store,
        &mut clock,
        "instance_create",
        &obj(&[("machine", text("case_v1")), ("request_id", text("case-1"))]),
    )
    .unwrap();
    (store, clock)
}

fn validated(tool: &str, output: &Value) {
    let spec = registry()
        .into_iter()
        .find(|candidate| candidate.name == tool)
        .unwrap();
    validate_args(&(spec.output_schema)(), output)
        .unwrap_or_else(|e| panic!("{tool} output does not validate: {e:?}"));
}

#[test]
fn a_dry_run_reports_what_would_change_and_writes_nothing() {
    let directory = TestDirectory::create();
    let (mut store, mut clock) = ready(&directory);
    let before = store.records.len();
    let preview = dispatch(
        &mut store,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("dry_run", Value::Bool(true)),
        ]),
    )
    .unwrap();
    assert_eq!(store.records.len(), before, "a question writes nothing");
    assert_eq!(preview.get("dry_run").and_then(Value::as_bool), Some(true));
    assert_eq!(
        preview.get("would_migrate").and_then(Value::as_bool),
        Some(true)
    );
    let context = preview
        .get("context_changes")
        .and_then(Value::as_arr)
        .unwrap();
    let score = context
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some("score"))
        .expect("score is in the preview");
    assert_eq!(score.get("before").and_then(Value::as_str), Some("1"));
    assert_eq!(score.get("after").and_then(Value::as_str), Some("6"));
    validated("instance_migrate", &preview);

    // And then the writing form, which does move it.
    let migrated = dispatch(
        &mut store,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("request_id", text("mig-1")),
        ]),
    )
    .unwrap();
    assert_eq!(
        migrated.get("dry_run").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        store.state.instances["inst-case-1"].ctx["score"].canonical_string(),
        "6"
    );
    validated("instance_migrate", &migrated);
}

#[test]
fn a_dry_run_works_on_a_read_only_server_and_the_writing_form_does_not() {
    let directory = TestDirectory::create();
    let (store, _) = ready(&directory);
    drop(store);
    let mut reader = Store::open_read_only(directory.path()).unwrap();
    let mut clock = FixedClock::new(2_000, 1);

    let preview = dispatch(
        &mut reader,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("dry_run", Value::Bool(true)),
        ]),
    )
    .expect("asking is reading");
    assert_eq!(preview.get("dry_run").and_then(Value::as_bool), Some(true));

    let error = dispatch(
        &mut reader,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("request_id", text("mig-1")),
        ]),
    )
    .expect_err("writing is not");
    assert_eq!(error.code, "io/write");
    assert!(error.message.contains("read-only"), "{}", error.message);
    assert!(MUTATING_TOOLS.contains(&"instance_migrate"));
}

#[test]
fn a_dry_run_needs_no_request_id_and_the_writing_form_does() {
    let directory = TestDirectory::create();
    let (mut store, mut clock) = ready(&directory);
    let spec = registry()
        .into_iter()
        .find(|candidate| candidate.name == "instance_migrate")
        .unwrap();
    let required: Vec<String> = (spec.input_schema)()
        .get("required")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    assert!(
        !required.contains(&"request_id".to_string()),
        "a preview claims no key, so the schema cannot demand one"
    );
    // The writing form without one still refuses, from the store.
    let error = dispatch(
        &mut store,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
        ]),
    )
    .expect_err("a write needs a key");
    assert_eq!(error.code, "args");
}

#[test]
fn a_refusal_is_data_a_model_can_act_on() {
    let directory = TestDirectory::create();
    let (mut store, mut clock) = ready(&directory);
    // Move the instance into the leaf the mapping does not cover.
    dispatch(
        &mut store,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("event", obj(&[("name", text("go"))])),
            ("request_id", text("go-1")),
        ]),
    )
    .unwrap();
    let preview = dispatch(
        &mut store,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("dry_run", Value::Bool(true)),
        ]),
    )
    .expect("a refusal is an answer, not a transport failure");
    assert_eq!(
        preview.get("would_migrate").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        preview
            .get("refusal")
            .and_then(|refusal| refusal.get("code"))
            .and_then(Value::as_str),
        Some("req/migrate_unmapped")
    );
    // The writing form reports the same code as a tool error.
    let error = dispatch(
        &mut store,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("request_id", text("mig-1")),
        ]),
    )
    .expect_err("the same refusal");
    assert_eq!(error.code, "req/migrate_unmapped");
}

#[test]
fn the_view_shows_which_definitions_an_instance_has_been_on() {
    let directory = TestDirectory::create();
    let (mut store, mut clock) = ready(&directory);
    let history = |store: &mut Store, clock: &mut FixedClock| -> Vec<String> {
        dispatch(
            store,
            clock,
            "instance_get",
            &obj(&[("instance_id", text("inst-case-1"))]),
        )
        .unwrap()
        .get("machine_history")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("machine_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
    };
    assert_eq!(history(&mut store, &mut clock).len(), 1);
    dispatch(
        &mut store,
        &mut clock,
        "instance_migrate",
        &obj(&[
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
            ("request_id", text("mig-1")),
        ]),
    )
    .unwrap();
    let after = history(&mut store, &mut clock);
    assert_eq!(after.len(), 2, "a reader sees the change without paging");
    assert_eq!(after[0], machine_id(&value(OLD)));
    assert_eq!(after[1], machine_id(&value(&new_source())));
}

#[test]
fn the_cli_and_the_tool_agree_byte_for_byte() {
    for dry in [true, false] {
        let cli_dir = TestDirectory::create();
        let tool_dir = TestDirectory::create();
        for directory in [&cli_dir, &tool_dir] {
            let (store, _) = ready(directory);
            drop(store);
        }
        let mut argv = vec![
            "instance".to_string(),
            "migrate".to_string(),
            "inst-case-1".to_string(),
            "--to=case_v2".to_string(),
            "--json".to_string(),
        ];
        if dry {
            argv.push("--dry-run".to_string());
        } else {
            argv.push("--request-id=mig-1".to_string());
        }
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(&argv)
            .arg(format!("--data-dir={}", cli_dir.path().display()))
            .env("FSM_CLOCK_MS", "5000")
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let mut store = Store::open(tool_dir.path()).unwrap();
        let mut clock = FixedClock::new(5_000, 1);
        let mut args = vec![
            ("instance_id", text("inst-case-1")),
            ("to_machine", text("case_v2")),
        ];
        if dry {
            args.push(("dry_run", Value::Bool(true)));
        } else {
            args.push(("request_id", text("mig-1")));
        }
        let structured = dispatch(&mut store, &mut clock, "instance_migrate", &obj(&args)).unwrap();
        let mut expected = fsm_core::canon::canon_bytes(&structured);
        expected.push(b'\n');
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(expected).unwrap(),
            "dry_run={dry}: one output, two surfaces"
        );
    }
}
