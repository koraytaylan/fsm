//! Provisioned native evidence fixture; default runs do not exercise native proof.
//! The external driver must provide a unique protected namespace and real domains.

#[cfg(target_os = "linux")]
#[allow(dead_code)] // Reuse the proved native authority; this fixture does not need its entry gate.
#[path = "../../fsm-execute/tests/lifecycle_platform/identity_root.rs"]
mod identity_root;

#[cfg(target_os = "linux")]
#[path = "execution_native/fixture.rs"]
mod fixture;

#[test]
fn native_evidence_fixture() {
    let Some(directory) = std::env::var_os("FSM_STORE_NATIVE_EVIDENCE_DIRECTORY") else {
        return;
    };
    let operation = std::env::var("FSM_STORE_NATIVE_EVIDENCE_OPERATION").unwrap();
    #[cfg(target_os = "linux")]
    fixture::run(std::path::Path::new(&directory), &operation).unwrap();
    #[cfg(not(target_os = "linux"))]
    panic!("native evidence requires provisioned Linux: {directory:?} {operation}");
}
