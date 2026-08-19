//! CLI inspection commands must not acquire or mutate the persistent store.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::record::RecordKind;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-cli-read-only-{}-{sequence}",
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

fn inspection_commands(instance_seq: u64) -> Vec<Vec<String>> {
    let strings = |arguments: &[&str]| {
        arguments
            .iter()
            .map(|argument| (*argument).to_string())
            .collect()
    };
    vec![
        strings(&["machine", "ls"]),
        strings(&["machine", "show", "case_review"]),
        strings(&["machine", "analyze", "case_review"]),
        strings(&["machine", "diagram", "case_review"]),
        strings(&["simulate", "case_review", "--events=[]"]),
        strings(&["instance", "ls"]),
        strings(&["instance", "show", "read-only-instance"]),
        strings(&["instance", "history", "read-only-instance"]),
        vec![
            "explain".into(),
            "read-only-instance".into(),
            format!("--seq={instance_seq}"),
        ],
        strings(&["journal", "verify"]),
        strings(&["journal", "replay"]),
        strings(&["doctor"]),
    ]
}

fn populate(data_dir: &Path) -> (Store, u64) {
    let mut store = Store::open(data_dir).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(definition, false, false).unwrap();
    store
        .create_instance("case_review", "read-only-instance", "create", None)
        .unwrap();
    let instance_seq = store
        .records
        .iter()
        .find(|record| record.kind == RecordKind::InstanceCreated)
        .unwrap()
        .seq;
    (store, instance_seq)
}

fn first_segment(data_dir: &Path) -> PathBuf {
    fs::read_dir(data_dir.join("journal"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("seg-") && name.ends_with(".jsonl"))
        })
        .unwrap()
}

fn directory_is_empty(path: &Path) -> bool {
    fs::read_dir(path).unwrap().next().is_none()
}

fn assert_non_directory_journal_diagnostic(output: &Output, data_dir: &Path) {
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let diagnostic = parse(&output.stderr, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        diagnostic
            .get("code")
            .and_then(fsm_core::json::Value::as_str),
        Some("io/read")
    );
    assert_eq!(
        diagnostic
            .get("docs")
            .and_then(fsm_core::json::Value::as_str),
        Some("fsm://docs/spec#io/read")
    );
    let message = diagnostic
        .get("message")
        .and_then(fsm_core::json::Value::as_str)
        .unwrap();
    assert!(
        message.contains("cannot inspect store format: read journal directory"),
        "{message}"
    );
    assert!(
        message.contains(&data_dir.join("journal").display().to_string()),
        "{message}"
    );
}

#[test]
fn corrupt_journal_path_is_a_typed_error_for_machine_list_and_doctor() {
    let directory = TestDirectory::create();
    let journal = directory.path().join("journal");
    fs::write(&journal, b"not a directory").unwrap();

    for arguments in [
        vec!["machine".into(), "ls".into(), "--json".into()],
        vec!["doctor".into(), "--json".into()],
    ] {
        let output = run(directory.path(), &arguments);
        assert_non_directory_journal_diagnostic(&output, directory.path());
        assert!(!directory.path().join("VERSION").exists());
        assert_eq!(fs::read(&journal).unwrap(), b"not a directory");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[cfg(unix)]
#[test]
fn inspections_do_not_follow_a_symlinked_snapshot_directory() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    let (writer, _) = populate(directory.path());
    drop(writer);
    let snapshots = directory.path().join("snapshots");
    fs::remove_dir_all(&snapshots).unwrap();

    let external = TestDirectory::create();
    let sentinel = external.path().join("snap-999.json");
    fs::write(&sentinel, b"external snapshot sentinel").unwrap();
    symlink(external.path(), &snapshots).unwrap();

    let listed = run(
        directory.path(),
        &["machine".into(), "ls".into(), "--json".into()],
    );
    assert!(listed.status.success());
    let doctor = run(directory.path(), &["doctor".into(), "--json".into()]);
    assert!(doctor.status.success());
    let report = parse(&doctor.stdout, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        report
            .get("snapshots")
            .and_then(fsm_core::json::Value::as_num),
        Some("0")
    );
    assert_eq!(fs::read(&sentinel).unwrap(), b"external snapshot sentinel");
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 1);
    fs::remove_file(snapshots).unwrap();
}

#[test]
fn inspections_create_nothing_for_absent_or_empty_data_dirs() {
    let parent = TestDirectory::create();
    let absent = parent.path().join("absent");
    let empty = parent.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let commands = inspection_commands(1);

    for arguments in &commands {
        let _ = run(&absent, arguments);
        assert!(
            !absent.exists(),
            "inspection {arguments:?} created an absent data directory"
        );

        let _ = run(&empty, arguments);
        assert!(
            directory_is_empty(&empty),
            "inspection {arguments:?} changed an empty data directory"
        );
    }
}

#[test]
fn inspections_coexist_with_a_live_writer() {
    let directory = TestDirectory::create();
    let (_writer, instance_seq) = populate(directory.path());
    let snapshots_before = fs::read_dir(directory.path().join("snapshots"))
        .unwrap()
        .count();

    for arguments in inspection_commands(instance_seq) {
        let output = run(directory.path(), &arguments);
        assert!(
            output.status.success(),
            "inspection {arguments:?} failed while writer held lock: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let snapshots_after = fs::read_dir(directory.path().join("snapshots"))
        .unwrap()
        .count();
    assert_eq!(snapshots_after, snapshots_before);
}

#[test]
fn store_inspection_uses_the_complete_prefix_while_doctor_reports_a_partial_tail() {
    let directory = TestDirectory::create();
    let (writer, _) = populate(directory.path());
    let segment = first_segment(directory.path());
    let complete_length = fs::metadata(&segment).unwrap().len();
    OpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap()
        .write_all(b"{\"seq\":")
        .unwrap();

    let listed = run(
        directory.path(),
        &["machine".into(), "ls".into(), "--json".into()],
    );
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let machines = parse(&listed.stdout, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        machines
            .get("machines")
            .and_then(fsm_core::json::Value::as_arr)
            .map(|values| values.len()),
        Some(1)
    );

    let doctor = run(directory.path(), &["doctor".into(), "--json".into()]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let report = parse(&doctor.stdout, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        report
            .get("readable")
            .and_then(fsm_core::json::Value::as_bool),
        Some(true)
    );
    assert!(
        report
            .get("verify")
            .and_then(fsm_core::json::Value::as_str)
            .is_some_and(|health| health.starts_with("TornTail"))
    );
    assert_eq!(fs::metadata(&segment).unwrap().len(), complete_length + 7);
    drop(writer);
}

#[test]
fn inspections_do_not_stamp_or_snapshot_a_migratable_store() {
    let directory = TestDirectory::create();
    let (writer, instance_seq) = populate(directory.path());
    drop(writer);
    let snapshots = directory.path().join("snapshots");
    let _ = fs::remove_dir_all(&snapshots);
    fs::write(directory.path().join("VERSION"), "7\n").unwrap();

    for arguments in inspection_commands(instance_seq) {
        let output = run(directory.path(), &arguments);
        assert!(
            output.status.success(),
            "legacy inspection {arguments:?} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("VERSION"))
                .unwrap()
                .trim(),
            "7",
            "inspection {arguments:?} stamped VERSION"
        );
        assert!(
            !snapshots.exists(),
            "inspection {arguments:?} created a snapshot directory"
        );
    }
}
