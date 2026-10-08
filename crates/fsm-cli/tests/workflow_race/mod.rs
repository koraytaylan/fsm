//! Actual standalone competition while the original handler tree remains live.
use super::*;
use fsm_core::record::RecordKind;
use fsm_store::store::Store;

mod crash;
pub(super) use crash::{configure_table, restart_at_cut};

pub(super) fn holds_tree(argument: &str) -> bool {
    matches!(
        argument,
        "handler-failures=race"
            | "handler-failures=crash-launch"
            | "handler-failures=crash-stop"
            | "handler-failures=crash-embedded-launch"
    )
}

pub(super) fn hold_tree(directory: &Path) {
    // The root-owned template keeps the marker outside DynamicUser RemoveIPC.
    fs::hard_link(
        directory.join(".work-template"),
        directory.join("tree-live"),
    )
    .expect("one live handler tree only");
    let mut descendant = Command::new("/usr/bin/sleep").arg("30").spawn().unwrap();
    fs::write(
        directory.join("tree-live"),
        format!("{} {}", std::process::id(), descendant.id()),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !directory.join("tree-release").exists() {
        assert!(
            Instant::now() < deadline,
            "race observer failed to release original tree"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    descendant.kill().unwrap();
    descendant.wait().unwrap();
    fs::remove_file(directory.join("tree-live")).unwrap();
}

pub(super) struct Competitor {
    child: Child,
    root: PathBuf,
    errors: PathBuf,
}
impl Drop for Competitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Competitor {
    pub(super) fn observation(&self, directory: &Directory) -> Value {
        fsm_cli::local_control::observe(&self.root, &directory.store(), 250).unwrap_or(Value::Null)
    }
    pub(super) fn stop(&mut self, directory: &Directory) {
        let root = self.root.clone();
        let data = directory.store();
        let stopped = std::thread::spawn(move || {
            fsm_cli::local_control::stop(
                &root,
                &data,
                fsm_execute::service::ShutdownMode::Drain,
                5000,
            )
        });
        let observed_until = Instant::now() + Duration::from_secs(7);
        let mut last = Value::Null;
        while !stopped.is_finished() && Instant::now() < observed_until {
            if let Ok(report) = fsm_cli::local_control::observe(&self.root, &directory.store(), 250)
            {
                last = report;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            stopped.is_finished(),
            "original bounded stop client did not retire"
        );
        let report = stopped.join().unwrap().unwrap_or_else(|error| {
            panic!("standalone drain transport: {error}; actual last inventory: {last:?}; final stderr: {}",
                bounded_executor_errors(&self.errors));
        });
        assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "standalone failed to exit after confirmed stop"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(self.child.wait().unwrap().success());
    }
}

pub(super) fn start(directory: &Directory, label: &str) -> Competitor {
    let entry = directory.1.as_ref().unwrap();
    let root = PathBuf::from(text(entry, "home")).join(format!("{label}-control"));
    let output = directory.0.join(format!("{label}-stdout"));
    let errors = directory.0.join(format!("{label}-stderr"));
    let child = Command::new(directory.executable())
        .env("HOME", text(entry, "home"))
        .arg("--json")
        .arg("--data-dir")
        .arg(directory.store())
        .args(["execute", "--handlers"])
        .arg(directory.0.join("handlers.json"))
        .args(["--poll-interval-ms", "5", "--control-dir"])
        .arg(&root)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output).unwrap())
        .stderr(fs::File::create(&errors).unwrap())
        .spawn()
        .unwrap();
    let mut competitor = Competitor {
        child,
        root,
        errors: errors.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fs::read_to_string(&errors).unwrap().contains("mode=paired") {
        assert!(competitor.child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline, "standalone did not initialize");
        std::thread::sleep(Duration::from_millis(10));
    }
    competitor
}

pub(super) fn contend(directory: &Directory, client: &mut Client) -> Competitor {
    assert!(
        directory.1.is_some(),
        "genuine registered native fixture required"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let marker = directory.resource().join("tree-live");
    let identities = loop {
        if let Ok(bytes) = fs::read_to_string(&marker) {
            let identifiers: Vec<u32> = bytes
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            if identifiers.len() == 2 {
                break identifiers
                    .into_iter()
                    .map(process_identity)
                    .collect::<Vec<_>>();
            }
        }
        assert!(
            Instant::now() < deadline,
            "original handler tree did not enter"
        );
        client.call("instance_get", value(r#"{"instance_id":"inst-run"}"#));
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut competitor = start(directory, "race");
    // A waiting foreign owner has no Start directive and is deliberately quiet.
    // Observe kernel reads across an interval after startup instead of requiring
    // a launch-oriented diagnostic that would misclassify correct exclusion.
    std::thread::sleep(Duration::from_millis(100));
    let before = read_characters(competitor.child.id());
    let observed =
        fsm_cli::local_control::observe(&competitor.root, &directory.store(), 1000).unwrap();
    assert_eq!(
        observed.get("phase").and_then(Value::as_str),
        Some("running")
    );
    assert_eq!(observed.get("admission_closed"), Some(&Value::Bool(false)));
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        client.call("instance_get", value(r#"{"instance_id":"inst-run"}"#));
        for identity in &identities {
            assert_eq!(
                process_identity(identity.0),
                *identity,
                "original live tree changed"
            );
        }
        let store = Store::open_read_only(&directory.store()).unwrap();
        assert_eq!(
            store
                .records
                .iter()
                .filter(|record| record.kind == RecordKind::ExecutionClaimed)
                .count(),
            1
        );
        assert_eq!(store.state.execution.unresolved().count(), 1);
        assert_eq!(
            fs::read_to_string(directory.resource().join("calls")).unwrap(),
            "check_prerequisite\n"
        );
        assert!(competitor.child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        read_characters(competitor.child.id()) > before,
        "standalone never observed subsequent prefixes"
    );
    fs::write(
        directory.resource().join("tree-release"),
        b"release original only",
    )
    .unwrap();
    competitor
}

fn process_identity(identifier: u32) -> (u32, String) {
    let stat = fs::read_to_string(format!("/proc/{identifier}/stat")).unwrap();
    let fields: Vec<&str> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    assert_ne!(fields[0], "Z", "original tree must be live");
    (identifier, fields[19].to_owned())
}

fn read_characters(identifier: u32) -> u64 {
    let counters = fs::read_to_string(format!("/proc/{identifier}/io")).unwrap();
    counters
        .lines()
        .find_map(|line| line.strip_prefix("rchar: "))
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}
