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
            .arg("--data-dir")
            .arg(&store);
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
    let mut writer = Store::open(&store).unwrap();
    writer
        .define_machine(parse(MACHINE, &JsonLimits::DEFAULT).unwrap(), false, false)
        .unwrap();
    writer
        .create_instance("lifecycle_crash", "instance", "create", None)
        .unwrap();
    drop(writer);
    let mut original = Host::start(&manifest, "original");
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
    let records = snapshot.records.clone();
    drop(snapshot);
    let collected_result = field(&manifest, "behavior") == "collected-result";
    if collected_result {
        fs::write(resource.join("root-release"), b"release").unwrap();
    }
    if collected_result || field(&manifest, "behavior") == "collected-timeout" {
        let ready = Path::new(field(&manifest, "authority"))
            .join(format!("crash-candidate-{}.json", claim.run_id()));
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
        assert!(members[1..].iter().all(live));
        if !collected_result || field(&manifest, "kind") == "mcp" {
            assert!(live(&members[0]));
        }
        assert_eq!(Store::open_read_only(&store).unwrap().records, records);
    }
    original.kill_and_wait();
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
        assert!(
            members[1..].iter().all(live),
            "immediate restart must begin while original descendants are alive"
        );
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
    let successor = Host::start(&manifest, "verified-restart");
    if collected_result {
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
