//! Root corruption of a genuine retired receipt cannot authorize public shutdown.

use super::*;
use fsm_core::record::execution::Claim;
use std::io::Write;

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn shutdown_refuses_closure_for_another_journal_claim() {
    let mut fixture = Fixture::new_for_operator(fixture_table::handler_table(false, false));
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    super::super::super::super::bind(&fixture.directory, &binding).unwrap();
    super::super::super::super::closure::complete(&fixture.directory, 1).unwrap();
    let receipt = fixture.directory.join("closure-1-1.json");
    let original = fs::read(&receipt).unwrap();
    let identity = super::super::super::super::identity(&fs::metadata(&receipt).unwrap());
    let records = Store::open_read_only(&fixture.store)
        .unwrap()
        .records
        .clone();
    broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    disconnect_cases::install_supervisor(&fixture.directory);
    disconnect_cases::permit_operator_store(&fixture.store);
    let handshake = fixture.store.join("shutdown-proof-handshake");
    fs::create_dir(&handshake).unwrap();
    std::os::unix::fs::chown(&handshake, Some(65534), Some(65534)).unwrap();
    fs::set_permissions(&handshake, fs::Permissions::from_mode(0o700)).unwrap();
    let mut daemon = Daemon::ready(&fixture.directory, 1);
    daemon.pause();
    let script = disconnect_cases::SUPERVISOR.replace(
        "::owned_request",
        "::shutdown_closure::refuse_another_journal_claim",
    );
    let stdout_path = fixture.directory.join("shutdown-proof.stdout");
    let stderr_path = fixture.directory.join("shutdown-proof.stderr");
    let mut child = OwnedChild(
        Command::new("/usr/bin/python3")
            .args(["-c", &script])
            .arg(fixture.directory.join("broker"))
            .arg(std::str::from_utf8(&canon_bytes(&binding)).unwrap())
            .env("FSM_SHUTDOWN_PROOF_HANDSHAKE", &handshake)
            .env("TMPDIR", &fixture.store)
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout_path).unwrap())
            .stderr(fs::File::create(&stderr_path).unwrap())
            .spawn()
            .unwrap(),
        stdout_path.clone(),
        stderr_path.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(25);
    wait(&mut child, &handshake, "request-sent", deadline);
    daemon.resume();
    wait(&mut child, &handshake, "transport-retired", deadline);
    let mut material = read_value(&receipt, true)
        .unwrap()
        .as_obj()
        .unwrap()
        .clone();
    let wrong = Value::Str(format!("sha256:{}", "0".repeat(64)));
    assert_ne!(binding.get("journal_claim"), Some(&wrong));
    material.insert("journal_claim".into(), wrong);
    rewrite(&receipt, &canon_bytes(&Value::Obj(material)));
    fs::write(handshake.join("corrupted"), b"corrupted").unwrap();
    wait(&mut child, &handshake, "refused", deadline);
    assert_eq!(
        super::super::super::super::identity(&fs::metadata(&receipt).unwrap()),
        identity
    );
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    rewrite(&receipt, &original);
    fs::write(handshake.join("restored"), b"restored").unwrap();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            let stdout = fs::read(&stdout_path).unwrap();
            let stderr = fs::read(&stderr_path).unwrap();
            assert!(stdout.len() <= 8192 && stderr.len() <= 8192);
            assert!(
                status.success(),
                "shutdown proof child failed: {} {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
            break;
        }
        assert!(Instant::now() < deadline, "shutdown proof child deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs::read(&receipt).unwrap(), original);
    assert_eq!(
        super::super::super::super::identity(&fs::metadata(&receipt).unwrap()),
        identity
    );
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(store.records, records);
    assert_eq!(
        store
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1),
        Some(&claim)
    );
    assert!(
        store
            .state
            .execution
            .stopped_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    drop(store);
    drop(daemon);
    fixture.cleanup().unwrap();
}

fn wait(child: &mut OwnedChild, directory: &Path, name: &str, deadline: Instant) {
    while !directory.join(name).exists() {
        if child.0.try_wait().unwrap().is_some() {
            let stdout = fs::read(&child.1).unwrap();
            let stderr = fs::read(&child.2).unwrap();
            assert!(stdout.len() <= 8192 && stderr.len() <= 8192);
            panic!(
                "shutdown proof child exited before {name}: {} {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
        assert!(
            Instant::now() < deadline,
            "shutdown proof handshake deadline: {name}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn rewrite(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
    fs::File::open(path.parent().unwrap())
        .unwrap()
        .sync_all()
        .unwrap();
}

struct OwnedChild(Child, PathBuf, PathBuf);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if self.0.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
