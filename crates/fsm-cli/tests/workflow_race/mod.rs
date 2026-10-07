//! Actual standalone competition while an embedded handler tree remains live.
use super::*;
use fsm_core::record::RecordKind;
use fsm_store::store::Store;

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
}
impl Drop for Competitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Competitor {
    pub(super) fn stop(&mut self, directory: &Directory) {
        let report = fsm_cli::local_control::stop(
            &self.root,
            &directory.store(),
            fsm_execute::service::ShutdownMode::Drain,
            5000,
        )
        .unwrap();
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
            "embedded handler tree did not enter"
        );
        client.call("instance_get", value(r#"{"instance_id":"inst-run"}"#));
        std::thread::sleep(Duration::from_millis(10));
    };
    let entry = directory.1.as_ref().unwrap();
    let root = PathBuf::from(text(entry, "home")).join("race-control");
    let output = directory.0.join("race-stdout");
    let errors = directory.0.join("race-stderr");
    let child = Command::new(directory.executable())
        .env("HOME", text(entry, "home"))
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
    let mut competitor = Competitor { child, root };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            competitor.child.try_wait().unwrap().is_none(),
            "standalone exited: {}",
            bounded_executor_errors(&errors)
        );
        if fs::read_to_string(&output)
            .unwrap()
            .contains("observed pending")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "standalone never observed original pending effect: {}",
            bounded_executor_errors(&errors)
        );
        client.call("instance_get", value(r#"{"instance_id":"inst-run"}"#));
        std::thread::sleep(Duration::from_millis(10));
    }
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
