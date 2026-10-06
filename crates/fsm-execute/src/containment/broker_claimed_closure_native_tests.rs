//! Root-side assertions for the actual unprivileged claimed closure client.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Retained,
    Interrupted,
    OwnedInterrupted,
    PairedInterrupted,
}

pub(super) fn run(
    fixture: &mut Fixture,
    binding: &Value,
    effect: &str,
    successor: &NativeDomain,
    outcome: Outcome,
) {
    // The dispatching broker fixture already transferred this store; a second
    // root-only transfer would reject its deliberately unprivileged owner.
    let operator_store = fs::symlink_metadata(&fixture.store).unwrap();
    assert!(operator_store.is_dir());
    assert_eq!(operator_store.uid(), 65534);
    assert_eq!(operator_store.gid(), 65534);
    assert_eq!(operator_store.mode() & 0o777, 0o700);
    let (test, marker) = match outcome {
        Outcome::Interrupted => (
            "::claimed_closure::bound_claimed_interruption",
            "FSM_NATIVE_BOUND_INTERRUPTION",
        ),
        Outcome::PairedInterrupted => (
            "::claimed_closure::bound_paired_driver_writer_contention",
            "FSM_NATIVE_BOUND_PAIRED_DRIVER",
        ),
        Outcome::OwnedInterrupted => (
            "::claimed_closure::bound_owned_driver_interruption",
            "FSM_NATIVE_BOUND_OWNED_DRIVER",
        ),
        Outcome::Retained => (
            "::claimed_closure::bound_claimed_closure",
            "FSM_NATIVE_BOUND_CLOSURE",
        ),
    };
    let script = disconnect_cases::SUPERVISOR.replace("::owned_request", test);
    let handshake = fixture.store.join("paired-proof-handshake");
    if outcome == Outcome::PairedInterrupted {
        fs::create_dir(&handshake).unwrap();
        std::os::unix::fs::chown(&handshake, Some(65534), Some(65534)).unwrap();
        fs::set_permissions(&handshake, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut command = Command::new("/usr/bin/python3");
    command.env("FSM_PAIRED_PROOF_HANDSHAKE", &handshake);
    let child = command
        .env(
            "TMPDIR",
            fixture.directory.parent().unwrap().join("operator-store"),
        )
        .args(["-c", &script])
        .arg(fixture.directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(binding)).unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = PairedChild(Some(child));
    if outcome == Outcome::PairedInterrupted {
        verify_paired_before_writer_release(fixture, binding, successor, &handshake);
    }
    let output = child.0.take().unwrap().wait_with_output().unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "claimed closure control: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| *line == marker)
            .count(),
        1
    );
    assert_eq!(
        read_value(&fixture.directory.join("binding-1.json"), true).unwrap(),
        *binding
    );
    let claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let proof = VerifiedClosure::read(
        &fixture
            .directory
            .join(format!("closure-1-{}.json", claim.run_id())),
    )
    .unwrap();
    assert!(proof.matches_claim(&claim, text(binding, "journal_claim").unwrap()));
    proof.check_store(&fixture.store).unwrap();
    if matches!(
        outcome,
        Outcome::OwnedInterrupted | Outcome::PairedInterrupted
    ) {
        for name in [
            "launch",
            "entry",
            "handoff",
            "manager-stopped",
            "manager-retired",
        ] {
            assert!(!fixture.directory.join(format!("{name}-1.json")).exists());
        }
    }
    assert!(
        !cgroup(&origin(&fixture.directory).unwrap(), 1)
            .unwrap()
            .exists()
    );
    let successor_allocation = number(&successor.to_value(), "allocation").unwrap();
    assert_eq!(successor_allocation, 2);
    assert!(
        cgroup(&origin(&fixture.directory).unwrap(), successor_allocation)
            .unwrap()
            .exists()
    );
    assert!(!fixture.directory.join("closing-2.json").exists());
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 2);
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        store.state.execution.claim_for("instance", effect),
        if outcome != Outcome::Retained {
            None
        } else {
            Some(&claim)
        }
    );
    if outcome != Outcome::Retained {
        assert!(
            store
                .state
                .dedup
                .contains_key(&format!("exec-interrupted-{effect}-{}", claim.run_id()))
        );
        assert!(
            !store
                .state
                .dedup
                .contains_key(&fsm_execute::rid::ack_rid(effect))
        );
    }
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", effect)
            .is_none()
    );
    assert!(
        store.state.instances["instance"]
            .pending
            .contains(&effect.to_owned())
    );
    drop(store);
    fixture.cleanup().unwrap();
}

struct PairedChild(Option<Child>);
impl Drop for PairedChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
// Insert in the provisioned root harness after spawning the exact paired probe.
// Handshake lives in a fresh dedicated operator-writable fixture directory.
fn verify_paired_before_writer_release(
    fixture: &Fixture,
    binding: &Value,
    successor: &NativeDomain,
    handshake: &Path,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !handshake.join("writer-held").exists() {
        assert!(
            Instant::now() < deadline,
            "paired child never reached held-writer stage"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let proof_path = fixture
        .directory
        .join(format!("closure-1-{}.json", claim.run_id()));
    let closure_deadline = Instant::now() + Duration::from_secs(8);
    let proof = loop {
        if let Ok(proof) = VerifiedClosure::read(&proof_path)
            && !cgroup(&origin(&fixture.directory).unwrap(), 1)
                .unwrap()
                .exists()
        {
            break proof;
        }
        assert!(
            Instant::now() < closure_deadline,
            "original closure proof did not arrive while writer held"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(proof.matches_claim(&claim, text(binding, "journal_claim").unwrap()));
    proof.check_store(&fixture.store).unwrap();
    assert_eq!(
        read_value(&fixture.directory.join("binding-1.json"), true).unwrap(),
        *binding
    );
    assert!(
        !cgroup(&origin(&fixture.directory).unwrap(), 1)
            .unwrap()
            .exists()
    );
    let successor_allocation = number(&successor.to_value(), "allocation").unwrap();
    assert_eq!(successor_allocation, 2);
    assert!(
        cgroup(&origin(&fixture.directory).unwrap(), successor_allocation)
            .unwrap()
            .exists()
    );
    assert!(!fixture.directory.join("closing-2.json").exists());
    let reader = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(
        reader
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1),
        Some(&claim)
    );
    assert!(
        reader
            .state
            .execution
            .stopped_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    // Prove the actual original writer is still held at the proof observation.
    assert!(Store::open(&fixture.store).is_err());
    std::fs::write(handshake.join("root-proof-checked"), b"continue").unwrap();
}
