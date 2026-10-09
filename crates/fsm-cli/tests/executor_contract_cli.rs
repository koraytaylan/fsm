//! Real-binary contract checks: dry checks are not execution or store mutation.
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::spec::compile_accepted;
use fsm_execute::config::HandlerTable;
use fsm_execute::contract::{Limits, analyze_contract};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
const DRAFT: &str = include_str!("fixtures/contract/draft.json");
const TABLE: &str = r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","argv":["/SECRET_EXECUTABLE_SENTINEL","{value}"],"timeout_ms":1000}]}"#;
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fsm-contract-cli-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn json(bytes: &[u8]) -> Value {
    parse(bytes, &JsonLimits::DEFAULT).unwrap()
}
fn run(directory: &Directory, data: &Path, arguments: &[&str]) -> Output {
    fs::write(directory.0.join("handlers.json"), TABLE).unwrap();
    Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(data)
        .args(["execute", "--handlers"])
        .arg(directory.0.join("handlers.json"))
        .args(arguments)
        .output()
        .unwrap()
}
#[derive(Debug, PartialEq, Eq)]
enum InventoryEntry {
    Directory,
    Bytes(Vec<u8>),
    #[cfg(windows)]
    Locked {
        len: u64,
        created: u64,
        modified: u64,
        attributes: u32,
    },
}
fn open_writer(data: &Path) -> Store {
    // Pin the diagnostic record so its exact bytes can be independently
    // checked after release even on Windows, which denies locked reads.
    let _pin = fsm_store::clock::pin(4242);
    Store::open(data).unwrap()
}
fn verify_released_lock(data: &Path) {
    let expected = format!("{{\"pid\":{},\"started_ts\":4242}}\n", std::process::id());
    assert_eq!(
        fs::read(data.join("journal/LOCK")).unwrap(),
        expected.as_bytes()
    );
}
fn inventory(path: &Path) -> BTreeMap<PathBuf, InventoryEntry> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.insert(path.clone(), InventoryEntry::Directory);
            files.extend(inventory(&path));
        } else {
            let entry = match fs::read(&path) {
                Ok(bytes) => InventoryEntry::Bytes(bytes),
                Err(error) => {
                    #[cfg(windows)]
                    if error.raw_os_error() == Some(33)
                        && path.ends_with(Path::new("journal").join("LOCK"))
                    {
                        use std::os::windows::fs::MetadataExt;
                        let meta = fs::metadata(&path).unwrap();
                        files.insert(
                            path,
                            InventoryEntry::Locked {
                                len: meta.len(),
                                created: meta.creation_time(),
                                modified: meta.last_write_time(),
                                attributes: meta.file_attributes(),
                            },
                        );
                        continue;
                    }
                    panic!("cannot inventory {path:?}: {error}");
                }
            };
            files.insert(path, entry);
        }
    }
    files
}

#[test]
fn offline_report_matches_handwritten_golden_and_analyzer_without_accessing_data_dir() {
    let directory = Directory::new();
    let draft = directory.0.join("draft.json");
    fs::write(&draft, DRAFT).unwrap();
    for data in [
        directory.0.join("absent"),
        directory.0.join("not-a-directory"),
    ] {
        if data.ends_with("not-a-directory") {
            fs::write(&data, b"inaccessible store path").unwrap();
        }
        let output = run(
            &directory,
            &data,
            &["--check", "--machine-file", draft.to_str().unwrap()],
        );
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let machine = compile_accepted(&json(DRAFT.as_bytes())).unwrap();
        let expected = json(
            include_str!("fixtures/contract/compatible.report.json")
                .replace("$MACHINE_ID", &machine.machine_id)
                .as_bytes(),
        );
        assert_eq!(json(&output.stdout), expected);
        let table = HandlerTable::parse(TABLE).unwrap();
        let direct =
            analyze_contract(&machine, &BTreeMap::new(), &table, Limits::default()).unwrap();
        let mut bytes = canon_bytes(&direct.to_value());
        bytes.push(b'\n');
        assert_eq!(output.stdout, bytes);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("SECRET_EXECUTABLE_SENTINEL"));
        if data.ends_with("absent") {
            assert!(!data.exists());
        } else {
            assert_eq!(fs::read(&data).unwrap(), b"inaccessible store path");
        }
    }
}

#[test]
fn invalid_unknown_and_input_failures_have_distinct_exit_codes() {
    let directory = Directory::new();
    let draft = directory.0.join("draft.json");
    let data = directory.0.join("absent");
    let invalid_source = DRAFT.replace("\"value\":\"1\",", "");
    fs::write(&draft, &invalid_source).unwrap();
    let invalid = run(
        &directory,
        &data,
        &["--check", "--machine-file", draft.to_str().unwrap()],
    );
    assert_eq!(invalid.status.code(), Some(1));
    assert_eq!(
        json(&invalid.stdout).as_obj().unwrap()["status"].as_str(),
        Some("invalid")
    );
    assert_golden(
        &invalid,
        &invalid_source,
        include_str!("fixtures/contract/invalid.report.json"),
    );
    let unknown = DRAFT.replace(
        "\"name\":\"idle\",",
        &format!(
            "\"name\":\"idle\",\"invoke\":[{{\"id\":\"child\",\"machine\":\"{}\"}}],",
            "a".repeat(64)
        ),
    );
    fs::write(&draft, &unknown).unwrap();
    let unknown_source = unknown;
    let unknown = run(
        &directory,
        &data,
        &["--check", "--machine-file", draft.to_str().unwrap()],
    );
    assert_eq!(unknown.status.code(), Some(3));
    assert_golden(
        &unknown,
        &unknown_source,
        include_str!("fixtures/contract/unknown.report.json"),
    );
    for args in [
        vec!["--machine-file", draft.to_str().unwrap()],
        vec![
            "--check",
            "--machine-file",
            draft.to_str().unwrap(),
            "--machine",
            "simple",
        ],
        vec!["--check", "--machine", "simple"],
        vec!["--check", "--machine-file", "missing-file"],
    ] {
        let output = run(&directory, &data, &args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
    assert!(!data.exists());
}

#[test]
fn stored_checks_resolve_name_and_hash_under_a_held_writer_without_changing_files() {
    let directory = Directory::new();
    let data = directory.0.join("store");
    let mut store = open_writer(&data);
    let mut clock = FixedClock::new(1000, 1);
    store
        .define_machine_on(&mut clock, json(DRAFT.as_bytes()), false, false)
        .unwrap();
    let id = store.state.machines.keys().next().unwrap().clone();
    let before = inventory(&data);
    for reference in ["simple", id.as_str()] {
        let output = run(&directory, &data, &["--check", "--machine", reference]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(inventory(&data), before);
    }
    drop(store); // The writer itself writes its normal snapshot on drop.
    verify_released_lock(&data);
    let before = inventory(&data);
    let output = run(&directory, &data, &["--check", "--machine", "simple"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(inventory(&data), before);
}

#[test]
fn offline_missing_child_is_unknown_and_stored_closure_checks_the_child_sites() {
    for valid_child in [true, false] {
        let directory = Directory::new();
        let data = directory.0.join("store");
        let mut child = DRAFT.replace("\"name\":\"simple\"", "\"name\":\"child\"");
        if !valid_child {
            child = child.replace("\"value\":\"1\",", "");
        }
        let child_machine = compile_accepted(&json(child.as_bytes())).unwrap();
        let digest = child_machine.machine_id.rsplit_once("@sha256:").unwrap().1;
        let root = DRAFT.replace(
            "\"name\":\"idle\",",
            &format!(
                "\"name\":\"idle\",\"invoke\":[{{\"id\":\"child\",\"machine\":\"{digest}\"}}],"
            ),
        );
        let path = directory.0.join("root.json");
        fs::write(&path, &root).unwrap();
        let offline = run(
            &directory,
            &data,
            &["--check", "--machine-file", path.to_str().unwrap()],
        );
        assert_eq!(offline.status.code(), Some(3));
        assert!(!data.exists());
        let mut store = open_writer(&data);
        let mut clock = FixedClock::new(1000, 1);
        store
            .define_machine_on(&mut clock, json(child.as_bytes()), false, false)
            .unwrap();
        store
            .define_machine_on(&mut clock, json(root.as_bytes()), false, false)
            .unwrap();
        let before = inventory(&data);
        let stored = run(&directory, &data, &["--check", "--machine", "simple"]);
        assert_eq!(
            stored.status.code(),
            Some(if valid_child { 0 } else { 1 }),
            "{stored:?}"
        );
        assert_eq!(inventory(&data), before);
        if !valid_child {
            let value = json(&stored.stdout);
            let finding = &value.as_obj().unwrap()["findings"].as_arr().unwrap()[0];
            assert_eq!(
                finding.as_obj().unwrap()["machine_id"].as_str(),
                Some(child_machine.machine_id.as_str())
            );
            assert_eq!(
                finding.as_obj().unwrap()["path"].as_str(),
                Some("/states/0/entry/emit/0/args/value")
            );
        }
        drop(store);
        verify_released_lock(&data);
    }
}

#[test]
fn dual_stdin_and_private_table_errors_are_refused_without_disclosure() {
    let directory = Directory::new();
    let data = directory.0.join("absent");
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&data)
        .args([
            "execute",
            "--check",
            "--handlers",
            "-",
            "--machine-file",
            "-",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        json(&output.stderr),
        json(include_bytes!("fixtures/contract/usage.error.json"))
    );
    let handlers = directory.0.join("bad-table.json");
    fs::write(
        &handlers,
        TABLE.replace("/SECRET_EXECUTABLE_SENTINEL", "SECRET_EXECUTABLE_SENTINEL"),
    )
    .unwrap();
    let draft = directory.0.join("draft.json");
    fs::write(&draft, DRAFT).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&data)
        .args(["execute", "--check", "--handlers"])
        .arg(&handlers)
        .arg("--machine-file")
        .arg(&draft)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SECRET_EXECUTABLE_SENTINEL"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SECRET_EXECUTABLE_SENTINEL"));
    assert!(!data.exists());
}

fn assert_golden(output: &Output, source: &str, golden: &str) {
    let machine = compile_accepted(&json(source.as_bytes())).unwrap();
    assert_eq!(
        json(&output.stdout),
        json(
            golden
                .replace("$MACHINE_ID", &machine.machine_id)
                .as_bytes()
        )
    );
}

#[test]
fn single_stdin_draft_works_and_execution_options_are_refused() {
    use std::io::Write;
    let directory = Directory::new();
    let data = directory.0.join("absent");
    let table = directory.0.join("handlers.json");
    fs::write(&table, TABLE).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&data)
        .args(["execute", "--check", "--handlers"])
        .arg(&table)
        .args(["--machine-file", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(DRAFT.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_golden(
        &output,
        DRAFT,
        include_str!("fixtures/contract/compatible.report.json"),
    );
    let draft = directory.0.join("draft.json");
    fs::write(&draft, DRAFT).unwrap();
    for extra in [
        vec!["--exclusive"],
        vec!["--list-dead"],
        vec!["--poll-interval-ms", "100"],
        vec!["--since", "1"],
        vec!["--control-dir", "unused-control-directory"],
    ] {
        let mut arguments = vec!["--check", "--machine-file", draft.to_str().unwrap()];
        arguments.extend(extra);
        let output = run(&directory, &data, &arguments);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
    }
    assert!(!data.exists());
}

#[test]
fn explicit_manual_policy_is_compatible_and_legacy_scope_remains_table_only() {
    let directory = Directory::new();
    let data = directory.0.join("absent");
    let handlers = directory.0.join("manual.json");
    let draft = directory.0.join("draft.json");
    fs::write(
        &handlers,
        r#"{"format":"fsm.handlers/1","handlers":[],"manual_effects":["work"]}"#,
    )
    .unwrap();
    fs::write(&draft, DRAFT).unwrap();
    for check_machine in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fsm"));
        command
            .args(["--json", "--data-dir"])
            .arg(&data)
            .args(["execute", "--check", "--handlers"])
            .arg(&handlers);
        if check_machine {
            command.arg("--machine-file").arg(&draft);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        let value = json(&output.stdout);
        let fields = value.as_obj().unwrap();
        if check_machine {
            assert_eq!(fields["status"].as_str(), Some("compatible"));
            assert_eq!(fields["progress"], json(br#"["manual"]"#));
            let site = fields["effects"].as_arr().unwrap()[0].as_obj().unwrap();
            assert_eq!(site["disposition"].as_str(), Some("manual"));
            assert_eq!(site["outcomes"], json(b"{}"));
        } else {
            assert_eq!(fields["scope"].as_str(), Some("handler-table-only"));
            assert_eq!(fields["format"].as_str(), Some("fsm.handlers/1"));
        }
        assert!(!data.exists());
    }
}

#[test]
fn machine_reports_and_input_errors_never_disclose_nested_mcp_literals() {
    let directory = Directory::new();
    let data = directory.0.join("absent");
    let draft = directory.0.join("draft.json");
    let handlers = directory.0.join("handlers.json");
    fs::write(&draft, DRAFT).unwrap();
    let table = r#"{"format":"fsm.handlers/1","handlers":[{
      "effect":"work","kind":"mcp","argv":["/SECRET_EXECUTABLE_SENTINEL","SECRET_ARGV_SENTINEL","{value}"],
      "tool":"SECRET_TOOL_SENTINEL","arguments":{"nested":{"literal":"SECRET_ARGUMENT_SENTINEL","value":"{value}"}},
      "timeout_ms":1000
    }]}"#;
    let invalid = table.replace("\"timeout_ms\":1000", "\"timeout_ms\":1000,\"on_ok\":{\"event\":\"undeclared\",\"payload\":{\"private\":\"SECRET_OUTCOME_SENTINEL\"}}");
    let malformed = table.replace("/SECRET_EXECUTABLE_SENTINEL", "SECRET_EXECUTABLE_SENTINEL");
    for (source, expected_exit) in [(table, 0), (invalid.as_str(), 1), (malformed.as_str(), 2)] {
        fs::write(&handlers, source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
            .args(["--json", "--data-dir"])
            .arg(&data)
            .args(["execute", "--check", "--handlers"])
            .arg(&handlers)
            .arg("--machine-file")
            .arg(&draft)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected_exit), "{output:?}");
        for encoded in [&output.stdout, &output.stderr] {
            assert!(
                !String::from_utf8_lossy(encoded).contains("SENTINEL"),
                "private table data leaked: {output:?}"
            );
        }
        if expected_exit != 2 {
            let machine = compile_accepted(&json(DRAFT.as_bytes())).unwrap();
            let table = HandlerTable::parse(source).unwrap();
            let report =
                analyze_contract(&machine, &BTreeMap::new(), &table, Limits::default()).unwrap();
            let mut expected = canon_bytes(&report.to_value());
            expected.push(b'\n');
            assert_eq!(output.stdout, expected);
        }
        assert!(!data.exists());
    }
}

#[test]
fn handler_stdin_and_file_draft_use_separate_inputs_without_store_access() {
    use std::io::Write;
    let directory = Directory::new();
    let data = directory.0.join("absent");
    let draft = directory.0.join("draft.json");
    fs::write(&draft, DRAFT).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&data)
        .args(["execute", "--check", "--handlers", "-"])
        .arg("--machine-file")
        .arg(&draft)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(TABLE.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_golden(
        &output,
        DRAFT,
        include_str!("fixtures/contract/compatible.report.json"),
    );
    assert!(!data.exists());
}

#[test]
fn legacy_table_inspection_and_dead_letter_listing_remain_read_only_under_writer() {
    let directory = Directory::new();
    let data = directory.0.join("store");
    let store = open_writer(&data);
    let records = store.records.clone();
    let before = inventory(&data);
    let check = run(&directory, &data, &["--check"]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let table = json(&check.stdout);
    assert_eq!(
        table.get("scope").and_then(Value::as_str),
        Some("handler-table-only")
    );
    assert!(String::from_utf8_lossy(&check.stdout).contains("SECRET_EXECUTABLE_SENTINEL"));
    assert_eq!(table.get("dead_letters"), Some(&json(b"[]")));
    let dead = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--json", "--data-dir"])
        .arg(&data)
        .args(["execute", "--list-dead", "--since", "0"])
        .output()
        .unwrap();
    assert_eq!(dead.status.code(), Some(0), "{dead:?}");
    let report = json(&dead.stdout);
    assert_eq!(report.get("count"), Some(&json(b"0")));
    assert_eq!(report.get("dead_letters"), Some(&json(b"[]")));
    assert_eq!(inventory(&data), before);
    assert_eq!(Store::open_read_only(&data).unwrap().records, records);
    drop(store);
    verify_released_lock(&data);
}
