//! Forced executor death at launch/verified-stop cuts, with an independent observer.
use super::*;
use fsm_store::store::VerifiedClosure;
use std::os::unix::{fs::MetadataExt, process::ExitStatusExt};

pub(in super::super) fn configure_table(table: &mut Value, failures: &str) {
    if !matches!(
        failures,
        "crash-launch"
            | "crash-stop"
            | "crash-embedded-launch"
            | "crash-embedded-stop"
            | "crash-term"
            | "crash-int"
            | "crash-embedded-term"
            | "crash-embedded-int"
            | "full-disk"
            | "full-disk-embedded"
            | "failed-stop"
            | "failed-stop-embedded"
            | "active-stop-abort"
            | "active-stop-abort-embedded"
            | "active-stop-eof-embedded"
            | "active-stop-output-embedded"
            | "active-stop-drain"
            | "active-stop-drain-embedded"
            | "active-stop-complete-drain"
            | "active-stop-complete-drain-embedded"
            | "active-stop-timeout-drain"
            | "active-stop-timeout-drain-embedded"
            | "expired-drain"
            | "expired-drain-embedded"
    ) {
        return;
    }
    let Value::Obj(table) = table else {
        panic!("handler table")
    };
    let Value::Arr(handlers) = table.get_mut("handlers").unwrap() else {
        panic!("handlers")
    };
    let Value::Obj(first) = &mut handlers[0] else {
        panic!("first handler")
    };
    // Signal cases must close through lease EOF before the handler deadline;
    // the eight-second closure observation cannot pass on this 30-second timeout.
    let timeout = if failures.ends_with("-term")
        || failures.ends_with("-int")
        || failures.starts_with("full-disk")
        || failures.starts_with("failed-stop")
        || failures.starts_with("expired-drain")
        || (failures.starts_with("active-stop") && !failures.starts_with("active-stop-timeout"))
    {
        "30000"
    } else {
        "3000"
    };
    first.insert("timeout_ms".into(), Value::Num(timeout.into()));
    first.insert(
        "retry".into(),
        value(r#"{"attempts":2,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]}"#),
    );
}

pub(in super::super) fn restart_at_cut(
    directory: &Directory,
    client: &mut Client,
    mut original: Option<&mut Competitor>,
    failures: &str,
) -> Competitor {
    let marker = directory.resource().join("tree-live");
    let deadline = Instant::now() + Duration::from_secs(10);
    let bytes = loop {
        if let Ok(bytes) = fs::read_to_string(&marker)
            && bytes.split_whitespace().count() == 2
        {
            break bytes;
        }
        assert!(Instant::now() < deadline, "original tree never entered");
        client.call("instance_get", value(r#"{"instance_id":"inst-run"}"#));
        std::thread::sleep(Duration::from_millis(5));
    };
    let identities: Vec<_> = bytes
        .split_whitespace()
        .map(|pid| process_identity(pid.parse().unwrap()))
        .collect();
    let identity = fs::symlink_metadata(&marker).unwrap();
    let mut writer = if original.is_none() {
        Store::open_read_only(&directory.store()).unwrap()
    } else {
        loop {
            match Store::open(&directory.store()) {
                Ok(writer) => break writer,
                Err(error) if error.code == "store/lock" => {
                    assert!(Instant::now() < deadline, "writer never became available");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("original writer: {error:?}"),
            }
        }
    };
    assert_eq!(writer.state.execution.unresolved().count(), 1);
    let claim = writer
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .clone();
    let hash = writer.current_execution_claim_hash(&claim).unwrap();
    let records = writer.records.clone();
    assert!(
        writer
            .state
            .execution
            .stopped_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    if failures.ends_with("-term")
        || failures.ends_with("-int")
        || failures.starts_with("full-disk")
        || failures.starts_with("failed-stop")
        || failures.starts_with("expired-drain")
        || (failures.starts_with("active-stop") && !failures.starts_with("active-stop-timeout"))
    {
        assert!(
            identities.iter().all(still_live),
            "signal must reach a live handler tree"
        );
    }
    let mut replacement = match failures {
        "crash-launch" => Some(kill_and_restart(directory, client, original.as_deref_mut())),
        "crash-embedded-launch" => {
            let (held, replacement) = kill_embedded_and_restart(directory, client, &records);
            writer = held;
            Some(replacement)
        }
        "crash-embedded-stop" => {
            pause_embedded(client);
            assert!(client.process.try_wait().unwrap().is_none());
            assert!(
                matches!(Store::open(&directory.store()), Err(error) if error.code == "store/lock")
            );
            None
        }
        "crash-term" | "crash-int" => {
            terminate_owned(&mut original.as_deref_mut().unwrap().child, failures);
            Some(start(directory, "after-signal"))
        }
        "crash-embedded-term" | "crash-embedded-int" => {
            terminate_owned(&mut client.process, failures);
            let until = Instant::now() + Duration::from_secs(3);
            writer = loop {
                match Store::open(&directory.store()) {
                    Ok(held) => break held,
                    Err(error) if error.code == "store/lock" => {
                        assert!(Instant::now() < until, "signal exit retained writer");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("signal recovery writer: {error:?}"),
                }
            };
            assert_eq!(writer.records, records);
            *client = Client::start_mode(directory, ExecutionMode::Standalone);
            Some(start(directory, "after-signal"))
        }
        "crash-stop" => None,
        _ => panic!("unknown crash cut"),
    };
    let domain = claim.domain().to_value();
    let receipt = PathBuf::from("/var/lib/fsm-containment")
        .join(text(&domain, "namespace"))
        .join(format!(
            "authority-{}",
            domain.get("generation").unwrap().as_num().unwrap()
        ))
        .join(format!(
            "closure-{}-{}.json",
            domain.get("allocation").unwrap().as_num().unwrap(),
            claim.run_id()
        ));
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let snapshot = Store::open_read_only(&directory.store()).unwrap();
        assert_eq!(
            snapshot.records, records,
            "writer-held recovery appended a record"
        );
        assert_eq!(snapshot.state.execution.unresolved().count(), 1);
        assert_eq!(
            fs::read_to_string(directory.resource().join("calls")).unwrap(),
            "check_prerequisite\n"
        );
        assert_eq!(fs::read_to_string(&marker).unwrap(), bytes);
        if let Ok(proof) = VerifiedClosure::read(&receipt) {
            assert!(proof.matches_claim(&claim, &hash));
            proof.check_store(&directory.store()).unwrap();
            assert!(identities.iter().all(|identity| !still_live(identity)));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "original native tree did not close"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    if replacement.is_none() {
        assert_eq!(
            Store::open_read_only(&directory.store()).unwrap().records,
            records
        );
        if let Some(original) = original {
            assert!(original.child.try_wait().unwrap().is_none());
            replacement = Some(kill_and_restart(directory, client, Some(original)));
        } else {
            assert!(client.process.try_wait().unwrap().is_none());
            assert!(
                matches!(Store::open(&directory.store()), Err(error) if error.code == "store/lock")
            );
            let (held, restarted) = kill_embedded_and_restart(directory, client, &records);
            writer = held;
            replacement = Some(restarted);
        }
    }
    // A killed handler cannot unlink its marker; only matched closure and dead
    // original identities permit retiring that exact fixture-owned link.
    let matched = fs::symlink_metadata(&marker).unwrap();
    assert_eq!(
        (matched.dev(), matched.ino()),
        (identity.dev(), identity.ino())
    );
    fs::remove_file(&marker).unwrap();
    fs::write(directory.resource().join("tree-release"), b"successor only").unwrap();
    drop(writer);
    replacement.unwrap()
}

// Deliver the actual OS signal to the fixture-owned incarnation and observe
// its documented signal exit independently of native closure verification.
fn terminate_owned(child: &mut std::process::Child, failures: &str) {
    let signal = if failures.ends_with("-int") { 2 } else { 15 };
    let pid = child.id();
    let identity = process_identity(pid);
    assert!(child.try_wait().unwrap().is_none());
    assert_eq!(process_identity(pid), identity);
    assert!(
        Command::new("/bin/kill")
            .args([format!("-{signal}"), pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.signal(), Some(signal));
            break;
        }
        assert!(
            Instant::now() < until,
            "owned executor signal exit exceeded bound"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn pause_embedded(client: &Client) {
    let pid = client.process.id();
    let original = process_identity(pid);
    assert!(
        Command::new("/bin/kill")
            .args(["-STOP", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        assert_eq!(process_identity(pid), original);
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        if stat.rsplit_once(") ").unwrap().1.split_whitespace().next() == Some("T") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "owned embedded server did not stop"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn kill_embedded_and_restart(
    directory: &Directory,
    client: &mut Client,
    records: &[fsm_core::record::Record],
) -> (Store, Competitor) {
    assert!(matches!(client.mode, ExecutionMode::Embedded));
    client.process.kill().unwrap();
    assert_eq!(client.process.wait().unwrap().signal(), Some(9));
    let deadline = Instant::now() + Duration::from_secs(3);
    let writer = loop {
        match Store::open(&directory.store()) {
            Ok(writer) => break writer,
            Err(error) if error.code == "store/lock" => {
                assert!(
                    Instant::now() < deadline,
                    "killed embedded writer did not release"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("embedded recovery writer: {error:?}"),
        }
    };
    assert_eq!(writer.records, records);
    *client = Client::start_mode(directory, ExecutionMode::Standalone);
    (writer, start(directory, "after-kill"))
}

fn kill_and_restart(
    directory: &Directory,
    client: &mut Client,
    original: Option<&mut Competitor>,
) -> Competitor {
    if let Some(original) = original {
        original.child.kill().unwrap();
        assert_eq!(original.child.wait().unwrap().signal(), Some(9));
    } else {
        assert!(matches!(client.mode, ExecutionMode::Embedded));
        client.process.kill().unwrap();
        assert_eq!(client.process.wait().unwrap().signal(), Some(9));
        // The held writer makes this fresh plain session a read-only observer.
        *client = Client::start_mode(directory, ExecutionMode::Standalone);
    }
    start(directory, "after-kill")
}

fn still_live(identity: &(u32, String)) -> bool {
    let stat = match fs::read_to_string(format!("/proc/{}/stat", identity.0)) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => panic!("original process observation failed: {error}"),
    };
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[19] == identity.1 && fields[0] != "Z"
}

#[test]
#[ignore = "requires the registered native workflow authority and independent tree observer"]
fn killed_standalone_recovers_without_overlapping_trees() {
    run_scenario_mode(
        "crash-launch",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Standalone,
    );
}

#[test]
#[ignore = "requires the registered native workflow authority and independent tree observer"]
fn killed_standalone_after_verified_stop_recovers_once() {
    run_scenario_mode(
        "crash-stop",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Standalone,
    );
}

#[test]
#[ignore = "requires registered native authority and independent tree observer"]
fn killed_embedded_recovers_without_overlapping_trees() {
    run_scenario_mode(
        "crash-embedded-launch",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Embedded,
    );
}

#[test]
#[ignore = "requires registered native authority and independent tree observer"]
fn killed_embedded_after_verified_stop_recovers_once() {
    run_scenario_mode(
        "crash-embedded-stop",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Embedded,
    );
}

#[test]
#[ignore = "requires registered native authority and independent tree observer"]
fn terminated_standalone_recovers_without_overlapping_trees() {
    run_scenario_mode(
        "crash-term",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Standalone,
    );
}

#[test]
#[ignore = "requires registered native authority and independent tree observer"]
fn interrupted_standalone_recovers_without_overlapping_trees() {
    run_scenario_mode(
        "crash-int",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Standalone,
    );
}

#[test]
#[ignore = "requires registered native authority and independent tree observer"]
fn terminated_embedded_recovers_without_overlapping_trees() {
    run_scenario_mode(
        "crash-embedded-term",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Embedded,
    );
}

#[test]
#[ignore = "requires registered native authority and independent tree observer"]
fn interrupted_embedded_recovers_without_overlapping_trees() {
    run_scenario_mode(
        "crash-embedded-int",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Embedded,
    );
}
