//! Root-side assertions for the actual unprivileged claimed closure client.

use super::*;

pub(super) fn run(fixture: &mut Fixture, binding: &Value, effect: &str, successor: &NativeDomain) {
    disconnect_cases::permit_operator_store(&fixture.store);
    let script = disconnect_cases::SUPERVISOR.replace(
        "::owned_request",
        "::claimed_closure::bound_claimed_closure",
    );
    let output = Command::new("/usr/bin/python3")
        .env(
            "TMPDIR",
            fixture.directory.parent().unwrap().join("operator-store"),
        )
        .args(["-c", &script])
        .arg(fixture.directory.join("broker"))
        .arg(std::str::from_utf8(&canon_bytes(binding)).unwrap())
        .stdin(Stdio::null())
        .output()
        .unwrap();
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
            .filter(|line| *line == "FSM_NATIVE_BOUND_CLOSURE")
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
        Some(&claim)
    );
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
