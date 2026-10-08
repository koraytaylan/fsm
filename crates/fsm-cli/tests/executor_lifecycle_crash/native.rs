//! Provisioned production-host candidate crash, observed outside the executor.
//! The Root coordinator supplies protected artifacts/catalogue and owns teardown.

use super::*;
use fsm_core::record::RecordKind;
use fsm_store::store::{Store, VerifiedClosure};
use std::{
    io::Read,
    os::unix::{fs::MetadataExt, process::ExitStatusExt},
    path::Path,
};

const MACHINE: &[u8] = br#"{
 "format":"fsm.machine/1","name":"lifecycle_crash","context":[],
 "events":[{"name":"done","fields":[]}],"effects":[{"name":"notify","fields":[]}],
 "states":[{"name":"running","entry":{"emit":[{"effect":"notify","args":{}}]}},
           {"name":"finished","terminal":true}],
 "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
}"#;

#[path = "native/authorization.rs"]
mod authorization;

#[path = "native/repeated.rs"]
mod repeated;

struct Host {
    process: Child,
    input: Option<std::process::ChildStdin>,
    reader: Option<std::thread::JoinHandle<()>>,
    errors: PathBuf,
}

impl Host {
    fn start(manifest: &Value, label: &str) -> Self {
        let store = PathBuf::from(field(manifest, "store"));
        let errors = store.join(format!("{label}-stderr"));
        let embedded = field(manifest, "host") == "embedded";
        let mut command = Command::new(field(manifest, "cli"));
        command
            .env("HOME", field(manifest, "home"))
            .env_remove("FSM_LIFECYCLE_JOURNAL_CUT")
            .arg("--data-dir")
            .arg(&store);
        if label == "original" && journal_cut(manifest).is_some() {
            command.env(
                "FSM_LIFECYCLE_JOURNAL_CUT",
                Path::new(field(manifest, "authority")).join("crash-journal-barrier.json"),
            );
        }
        if embedded {
            command
                .args(["serve", "--execute", "--handlers"])
                .arg(store.join("handlers.json"));
        } else {
            command
                .arg("--json")
                .args(["execute", "--handlers"])
                .arg(store.join("handlers.json"))
                .args(["--poll-interval-ms", "5", "--control-dir"])
                .arg(Path::new(field(manifest, "home")).join(format!("{label}-control")));
        }
        let mut process = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&errors).unwrap())
            .spawn()
            .unwrap();
        let mut input = process.stdin.take().unwrap();
        let stdout = process.stdout.take().unwrap();
        let (sender, responses) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if line.len() > 65_536 {
                    break;
                }
                let Ok(value) = parse(line.as_bytes(), &JsonLimits::DEFAULT) else {
                    continue;
                };
                if value.get("id") == Some(&Value::Num("1".into())) {
                    let _ = sender.send(value);
                }
            }
        });
        if embedded {
            input.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"lifecycle-matrix\",\"version\":\"1\"}}}\n").unwrap();
            input.flush().unwrap();
            let response = responses
                .recv_timeout(Duration::from_secs(8))
                .unwrap_or_else(|error| {
                    let mut bytes = Vec::new();
                    fs::File::open(&errors)
                        .unwrap()
                        .take(8192)
                        .read_to_end(&mut bytes)
                        .unwrap();
                    panic!(
                        "embedded initialization: {error}; stderr: {}",
                        String::from_utf8_lossy(&bytes)
                    );
                });
            assert!(
                response.get("result").is_some(),
                "embedded initialization refused: {response:?}"
            );
            input
                .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
                .unwrap();
            input.flush().unwrap();
        }
        Self {
            process,
            input: Some(input),
            reader: Some(reader),
            errors,
        }
    }

    fn kill_and_wait(&mut self) {
        self.process.kill().unwrap();
        assert_eq!(self.process.wait().unwrap().signal(), Some(9));
    }

    fn terminate_and_wait(&mut self, signal: i32) {
        assert!(matches!(signal, 2 | 15));
        assert!(self.process.try_wait().unwrap().is_none());
        // The unreaped owned child keeps its PID reserved; signal exactly this
        // executor, never its group, descendants or an inferred process tree.
        let mut sender = Command::new("/usr/bin/python3")
            .args([
                "-c",
                "import os,sys;os.kill(int(sys.argv[1]),int(sys.argv[2]))",
            ])
            .arg(self.process.id().to_string())
            .arg(signal.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = sender.try_wait().unwrap() {
                assert!(status.success(), "native signal sender failed");
                break;
            }
            if Instant::now() >= deadline {
                sender.kill().unwrap();
                sender.wait().unwrap();
                panic!("native signal sender exceeded deadline");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                assert_eq!(status.signal(), Some(signal));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native signal did not terminate executor"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn diagnostics(&self) -> String {
        let mut bytes = Vec::new();
        fs::File::open(&self.errors)
            .unwrap()
            .take(8192)
            .read_to_end(&mut bytes)
            .unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.input.take();
        if self.process.try_wait().ok().flatten().is_none() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .expect("protected fixture field")
}

fn identity(pid: u32) -> (u32, String) {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    (
        pid,
        stat.rsplit_once(") ")
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap()
            .into(),
    )
}

fn live(identity: &(u32, String)) -> bool {
    let stat = match fs::read_to_string(format!("/proc/{}/stat", identity.0)) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => panic!("original process observation: {error}"),
    };
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[0] != "Z" && fields[19] == identity.1
}

#[test]
#[ignore = "requires disposable native CI, protected catalogue and exact staged fixture artifacts"]
fn production_candidate_result_crash_retains_original_tree_until_verified_closure() {
    let path = PathBuf::from(
        std::env::var_os("FSM_LIFECYCLE_NATIVE_MANIFEST").expect("native coordinator manifest"),
    );
    use std::os::unix::fs::OpenOptionsExt;
    let mut source = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000)
        .open(&path)
        .unwrap();
    let metadata = source.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 65_536);
    assert_eq!(
        (metadata.uid(), metadata.mode() & 0o7777, metadata.nlink()),
        (0, 0o444, 1)
    );
    let mut encoded = Vec::new();
    Read::by_ref(&mut source)
        .take(65_537)
        .read_to_end(&mut encoded)
        .unwrap();
    assert!(encoded.len() <= 65_536);
    let current = fs::symlink_metadata(path).unwrap();
    assert_eq!(
        (metadata.dev(), metadata.ino()),
        (current.dev(), current.ino())
    );
    let manifest = parse(&encoded, &JsonLimits::DEFAULT).unwrap();
    assert!(matches!(
        field(&manifest, "host"),
        "standalone" | "embedded"
    ));
    assert!(matches!(field(&manifest, "kind"), "process" | "mcp"));
    let store = PathBuf::from(field(&manifest, "store"));
    let resource = PathBuf::from(field(&manifest, "resource"));
    if field(&manifest, "behavior") == "repeated-noisy" {
        repeated::observe(&manifest, &store, &resource);
        return;
    }
    let mut writer = Store::open(&store).unwrap();
    writer
        .define_machine(parse(MACHINE, &JsonLimits::DEFAULT).unwrap(), false, false)
        .unwrap();
    writer
        .create_instance("lifecycle_crash", "instance", "create", None)
        .unwrap();
    drop(writer);
    let mut original = Host::start(&manifest, "original");
    if field(&manifest, "behavior") == "authorization" {
        authorization::observe(&manifest, &store, &resource, &mut original);
        return;
    }
    if field(&manifest, "behavior") == "claimed-result" {
        observe_claim_crash(&manifest, &store, &resource, &mut original);
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !resource.join("root-candidate").is_file() {
        assert!(
            original.process.try_wait().unwrap().is_none(),
            "original exited: {}",
            original.diagnostics()
        );
        assert!(
            Instant::now() < deadline,
            "candidate cut absent: {}",
            original.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!resource.join("root-published").exists());
    let original_marker = fs::read(resource.join("root-entered")).unwrap();
    let members: Vec<_> = ["root", "child", "grandchild"]
        .into_iter()
        .map(|role| {
            identity(
                fs::read_to_string(resource.join(format!("{role}-entered")))
                    .unwrap()
                    .parse()
                    .unwrap(),
            )
        })
        .collect();
    assert!(
        members.iter().all(live),
        "cut must reach a genuine live tree"
    );
    let snapshot = Store::open_read_only(&store).unwrap();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    let claim = snapshot
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .clone();
    let hash = snapshot.current_execution_claim_hash(&claim).unwrap();
    let mut records = snapshot.records.clone();
    drop(snapshot);
    let supervisor_death = field(&manifest, "behavior") == "supervisor-death";
    if supervisor_death {
        fs::write(
            resource.join("supervisor-death-request"),
            b"kill owned broker",
        )
        .unwrap();
        wait_for_supervisor(&manifest, "dead", 1);
        assert!(original.process.try_wait().unwrap().is_none());
        assert!(members.iter().all(live));
        assert_eq!(Store::open_read_only(&store).unwrap().records, records);
        assert_eq!(
            fs::read(resource.join("root-entered")).unwrap(),
            original_marker
        );
    }
    let closed_result = field(&manifest, "behavior") == "closed-result";
    let journal_cut = journal_cut(&manifest);
    let collected_result = field(&manifest, "behavior") == "collected-result" || closed_result;
    if collected_result || journal_cut.is_some() {
        fs::write(resource.join("root-release"), b"release").unwrap();
    }
    if let Some(cut) = journal_cut {
        let observed = observe_journal_cut(
            &store,
            &manifest,
            &claim,
            &hash,
            cut,
            &mut original,
            &members,
        );
        assert!(observed.starts_with(&records));
        records = observed;
    }
    if collected_result || field(&manifest, "behavior") == "collected-timeout" {
        let cut = if closed_result {
            "domain-closed"
        } else {
            "candidate"
        };
        let ready = Path::new(field(&manifest, "authority"))
            .join(format!("crash-{cut}-{}.json", claim.run_id()));
        let deadline = Instant::now() + Duration::from_secs(8);
        while !fs::symlink_metadata(&ready).is_ok_and(|metadata| {
            metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o7777 == 0o444
        }) {
            assert!(original.process.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "native candidate was never collected"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let observed = parse(&fs::read(&ready).unwrap(), &JsonLimits::DEFAULT).unwrap();
        let metadata = fs::symlink_metadata(&ready).unwrap();
        assert!(metadata.is_file());
        assert_eq!((metadata.uid(), metadata.mode() & 0o7777), (0, 0o444));
        assert_eq!(observed.get("claim"), Some(&claim.to_value()));
        assert_eq!(observed.get("cut"), Some(&Value::Str(cut.into())));
        assert_eq!(
            observed.get("candidate"),
            Some(&Value::Str(
                if collected_result {
                    field(&manifest, "kind")
                } else {
                    "timeout"
                }
                .into()
            ))
        );
        if closed_result {
            assert!(members.iter().all(|member| !live(member)));
            assert!(
                !Path::new(field(&manifest, "authority"))
                    .join(format!(
                        "completed-{}-{}.json",
                        claim
                            .domain()
                            .to_value()
                            .get("allocation")
                            .unwrap()
                            .as_num()
                            .unwrap(),
                        claim.run_id()
                    ))
                    .exists()
            );
            let receipt = Path::new(field(&manifest, "authority")).join(format!(
                "closure-{}-{}.json",
                claim
                    .domain()
                    .to_value()
                    .get("allocation")
                    .unwrap()
                    .as_num()
                    .unwrap(),
                claim.run_id()
            ));
            let proof = VerifiedClosure::read(&receipt).unwrap();
            assert!(proof.matches_claim(&claim, &hash));
            proof.check_store(&store).unwrap();
        } else {
            assert!(members[1..].iter().all(live));
        }
        if !closed_result && (!collected_result || field(&manifest, "kind") == "mcp") {
            assert!(live(&members[0]));
        }
        assert_eq!(Store::open_read_only(&store).unwrap().records, records);
    }
    match field(&manifest, "behavior") {
        "signal-int" => original.terminate_and_wait(2),
        "signal-term" => original.terminate_and_wait(15),
        _ => original.kill_and_wait(),
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let held = loop {
        match Store::open(&store) {
            Ok(writer) => break writer,
            Err(error) if error.code == "store/lock" => {
                assert!(Instant::now() < deadline, "killed host retained its writer");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("original writer reopen: {error:?}"),
        }
    };
    assert_eq!(held.records, records);
    let blocked = Host::start(&manifest, "immediate-restart");
    if collected_result || field(&manifest, "behavior") == "collected-timeout" {
        if closed_result {
            assert!(members.iter().all(|member| !live(member)));
        } else {
            assert!(
                members[1..].iter().all(live),
                "immediate restart must begin while original descendants are alive"
            );
        }
        assert_eq!(
            fs::read(resource.join("root-entered")).unwrap(),
            original_marker
        );
        assert_eq!(Store::open_read_only(&store).unwrap().records, records);
        fs::write(resource.join("closure-release"), b"restart observed").unwrap();
    }
    let domain = claim.domain().to_value();
    let receipt = Path::new("/var/lib/fsm-containment")
        .join(field(&domain, "namespace"))
        .join(format!(
            "authority-{}",
            domain.get("generation").unwrap().as_num().unwrap()
        ))
        .join(format!(
            "closure-{}-{}.json",
            domain.get("allocation").unwrap().as_num().unwrap(),
            claim.run_id()
        ));
    let successor = if supervisor_death {
        assert!(members.iter().all(live));
        assert_eq!(Store::open_read_only(&store).unwrap().records, records);
        assert_eq!(
            fs::read(resource.join("root-entered")).unwrap(),
            original_marker
        );
        fs::write(
            resource.join("supervisor-restart-request"),
            b"restart owned broker",
        )
        .unwrap();
        wait_for_supervisor(&manifest, "restarted", 2);
        assert!(members.iter().all(live));
        assert!(VerifiedClosure::read(&receipt).is_err());
        drop(blocked);
        drop(held);
        Host::start(&manifest, "verified-restart")
    } else {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert_eq!(Store::open_read_only(&store).unwrap().records, records);
            assert_eq!(
                fs::read(resource.join("root-entered")).unwrap(),
                original_marker,
                "replacement entered while original ownership was retained"
            );
            if let Ok(proof) = VerifiedClosure::read(&receipt) {
                assert!(proof.matches_claim(&claim, &hash));
                proof.check_store(&store).unwrap();
                assert!(members.iter().all(|member| !live(member)));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "original closure absent: {}",
                blocked.diagnostics()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(blocked);
        drop(held);
        Host::start(&manifest, "verified-restart")
    };
    if collected_result || journal_cut.is_some() {
        wait_for_completion(&store, &successor);
        assert_eq!(
            fs::read(resource.join("root-entered")).unwrap(),
            original_marker
        );
        let snapshot = Store::open_read_only(&store).unwrap();
        for kind in [
            RecordKind::ExecutionClaimed,
            RecordKind::ExecutionStopped,
            RecordKind::ExecutionSettled,
            RecordKind::EventApplied,
        ] {
            assert_eq!(
                snapshot
                    .records
                    .iter()
                    .filter(|record| record.kind == kind)
                    .count(),
                1
            );
        }
        assert!(members.iter().all(|member| !live(member)));
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    while fs::read(resource.join("root-entered")).unwrap() == original_marker {
        assert!(
            Instant::now() < deadline,
            "verified successor absent: {}",
            successor.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(members.iter().all(|member| !live(member)));
    let proof = VerifiedClosure::read(&receipt).unwrap();
    assert!(proof.matches_claim(&claim, &hash));
    proof.check_store(&store).unwrap();
    let snapshot = Store::open_read_only(&store).unwrap();
    let stopped = snapshot
        .records
        .iter()
        .find(|record| {
            record.kind == RecordKind::ExecutionStopped
                && record.body.get("run_id") == Some(&Value::Num(claim.run_id().to_string()))
        })
        .unwrap();
    let next = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionClaimed && record.seq > stopped.seq)
        .unwrap();
    let settled = snapshot
        .records
        .iter()
        .find(|record| {
            record.kind == RecordKind::ExecutionSettled
                && record.body.get("run_id") == Some(&Value::Num(claim.run_id().to_string()))
        })
        .unwrap();
    assert!(
        stopped.seq < settled.seq && settled.seq < next.seq,
        "durable original stop and settlement must precede successor claim"
    );
    drop(snapshot);
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    wait_for_completion(&store, &successor);
}

fn observe_claim_crash(manifest: &Value, store: &Path, resource: &Path, original: &mut Host) {
    let ready = store.join("journal-cut-ready.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready.is_file() {
        assert!(
            original.process.try_wait().unwrap().is_none(),
            "claim host exited: {}",
            original.diagnostics()
        );
        assert!(
            Instant::now() < deadline,
            "claim cut absent: {}",
            original.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut encoded = Vec::new();
    fs::File::open(ready)
        .unwrap()
        .take(65_537)
        .read_to_end(&mut encoded)
        .unwrap();
    assert!(encoded.len() <= 65_536);
    let observed = parse(&encoded, &JsonLimits::DEFAULT).unwrap();
    let snapshot = Store::open_read_only(store).unwrap();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    let claim = snapshot
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .clone();
    let hash = snapshot.current_execution_claim_hash(&claim).unwrap();
    let records = snapshot.records.clone();
    assert_eq!(snapshot.state.instances["instance"].pending.len(), 1);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.kind == RecordKind::ExecutionClaimed)
            .count(),
        1
    );
    assert_eq!(observed.get("claim"), Some(&claim.to_value()));
    assert_eq!(observed.get("cut"), Some(&Value::Str("claimed".into())));
    assert_eq!(
        observed.get("pid"),
        Some(&Value::Num(original.process.id().to_string()))
    );
    assert_eq!(
        observed.get("seq"),
        Some(&Value::Num(snapshot.journal.last_seq.to_string()))
    );
    assert!(!records.iter().any(|record| matches!(
        record.kind,
        RecordKind::ExecutionStopped | RecordKind::ExecutionSettled | RecordKind::EventApplied
    )));
    let domain = claim.domain().to_value();
    let allocation = domain.get("allocation").unwrap().as_num().unwrap();
    assert!(
        !Path::new(field(manifest, "authority"))
            .join(format!("binding-{allocation}.json"))
            .exists()
    );
    assert_no_handler_entry(resource);
    drop(snapshot);
    original.kill_and_wait();
    let deadline = Instant::now() + Duration::from_secs(5);
    let held = loop {
        match Store::open(store) {
            Ok(writer) => break writer,
            Err(error) if error.code == "store/lock" => {
                assert!(
                    Instant::now() < deadline,
                    "claim host retained writer after SIGKILL"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("claim writer reopen: {error:?}"),
        }
    };
    assert_eq!(held.records, records);
    let blocked = Host::start(manifest, "immediate-restart");
    assert_eq!(Store::open_read_only(store).unwrap().records, records);
    assert_no_handler_entry(resource);
    drop(blocked);
    drop(held);
    let successor = Host::start(manifest, "verified-restart");
    let deadline = Instant::now() + Duration::from_secs(12);
    while !resource.join("root-candidate").is_file() {
        assert!(
            Instant::now() < deadline,
            "claim successor absent: {}",
            successor.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let receipt = Path::new(field(manifest, "authority"))
        .join(format!("closure-{allocation}-{}.json", claim.run_id()));
    let proof = VerifiedClosure::read(&receipt).unwrap();
    assert!(proof.matches_claim(&claim, &hash));
    proof.check_store(store).unwrap();
    let snapshot = Store::open_read_only(store).unwrap();
    assert!(snapshot.records.starts_with(&records));
    let stopped = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionStopped)
        .unwrap();
    let settled = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionSettled)
        .unwrap();
    let next = snapshot
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionClaimed && record.seq > stopped.seq)
        .unwrap();
    assert!(stopped.seq < settled.seq && settled.seq < next.seq);
    assert_eq!(
        stopped.body.get("run_id"),
        Some(&Value::Num(claim.run_id().to_string()))
    );
    assert_eq!(
        settled.body.get("run_id"),
        Some(&Value::Num(claim.run_id().to_string()))
    );
    assert_eq!(
        settled.body.get("disposition"),
        Some(&Value::Str("interrupted".into()))
    );
    assert_eq!(next.body.get("attempt"), claim.to_value().get("attempt"));
    assert_ne!(
        next.body.get("run_id"),
        Some(&Value::Num(claim.run_id().to_string()))
    );
    assert!(
        !snapshot
            .records
            .iter()
            .any(|record| record.kind == RecordKind::EventApplied)
    );
    drop(snapshot);
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    wait_for_completion(store, &successor);
    let snapshot = Store::open_read_only(store).unwrap();
    assert_eq!(
        snapshot
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::EventApplied)
            .count(),
        1
    );
}

fn assert_no_handler_entry(resource: &Path) {
    for role in ["root", "child", "grandchild"] {
        assert!(
            fs::read(resource.join(format!("{role}-entered")))
                .unwrap()
                .is_empty()
        );
    }
    assert!(!resource.join("root-candidate").exists());
    assert!(!resource.join("root-published").exists());
}

fn journal_cut(manifest: &Value) -> Option<&str> {
    field(manifest, "behavior")
        .strip_suffix("-result")
        .filter(|cut| matches!(*cut, "stopped" | "acked" | "event" | "claimed"))
}

fn observe_journal_cut(
    store: &Path,
    manifest: &Value,
    claim: &fsm_core::record::execution::Claim,
    hash: &str,
    cut: &str,
    original: &mut Host,
    members: &[(u32, String)],
) -> Vec<fsm_core::record::Record> {
    let ready = store.join("journal-cut-ready.json");
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready.is_file() {
        assert!(
            original.process.try_wait().unwrap().is_none(),
            "original exited: {}",
            original.diagnostics()
        );
        assert!(
            Instant::now() < deadline,
            "journal cut {cut} absent: {}",
            original.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut bytes = Vec::new();
    fs::File::open(ready)
        .unwrap()
        .take(65_537)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 65_536);
    let observed = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(observed.get("claim"), Some(&claim.to_value()));
    assert_eq!(observed.get("cut"), Some(&Value::Str(cut.into())));
    assert_eq!(
        observed.get("pid"),
        Some(&Value::Num(original.process.id().to_string()))
    );
    let snapshot = Store::open_read_only(store).unwrap();
    assert_eq!(
        observed.get("seq"),
        Some(&Value::Num(snapshot.journal.last_seq.to_string()))
    );
    let count = |kind| {
        snapshot
            .records
            .iter()
            .filter(|record| record.kind == kind)
            .count()
    };
    assert_eq!(count(RecordKind::ExecutionClaimed), 1);
    assert_eq!(count(RecordKind::ExecutionStopped), 1);
    assert_eq!(
        count(RecordKind::ExecutionSettled),
        usize::from(cut != "stopped")
    );
    assert_eq!(count(RecordKind::EventApplied), usize::from(cut == "event"));
    assert_eq!(
        snapshot.state.execution.unresolved().count(),
        usize::from(cut == "stopped")
    );
    assert_eq!(
        snapshot.state.execution_handoffs.outstanding().count(),
        usize::from(cut == "acked")
    );
    assert_eq!(
        snapshot.state.instances["instance"].pending.len(),
        usize::from(cut == "stopped")
    );
    assert_eq!(
        snapshot.state.instances["instance"].status,
        if cut == "event" {
            fsm_core::machine::Status::Completed
        } else {
            fsm_core::machine::Status::Running
        }
    );
    assert!(members.iter().all(|member| !live(member)));
    let domain = claim.domain().to_value();
    let receipt = Path::new(field(manifest, "authority")).join(format!(
        "closure-{}-{}.json",
        domain.get("allocation").unwrap().as_num().unwrap(),
        claim.run_id()
    ));
    let proof = VerifiedClosure::read(&receipt).unwrap();
    assert!(proof.matches_claim(claim, hash));
    proof.check_store(store).unwrap();
    snapshot.records.clone()
}

fn wait_for_supervisor(manifest: &Value, phase: &str, epoch: u64) {
    let ready =
        Path::new(field(manifest, "authority")).join(format!("crash-supervisor-{phase}.json"));
    let deadline = Instant::now() + Duration::from_secs(8);
    while !fs::symlink_metadata(&ready).is_ok_and(|metadata| {
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o7777 == 0o444
    }) {
        assert!(Instant::now() < deadline, "supervisor {phase} absent");
        std::thread::sleep(Duration::from_millis(5));
    }
    let observed = parse(&fs::read(ready).unwrap(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(observed.get("epoch"), Some(&Value::Num(epoch.to_string())));
}

fn wait_for_completion(store: &Path, successor: &Host) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = Store::open_read_only(store).unwrap();
        if snapshot.state.instances["instance"].pending.is_empty()
            && snapshot.state.execution.unresolved().count() == 0
            && snapshot.state.instances["instance"].status == fsm_core::machine::Status::Completed
            && snapshot.state.execution_handoffs.outstanding().count() == 0
        {
            assert!(
                snapshot
                    .records
                    .iter()
                    .any(|record| record.kind == RecordKind::ExecutionSettled)
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "successor failed to settle: {}",
            successor.diagnostics()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
