//! Downstream visibility of the claim-bound closure request primitive.

#[cfg(target_os = "linux")]
#[test]
fn closure_request_accepts_a_borrowed_snapshot_and_returns_an_opaque_proof() {
    use fsm_core::record::execution::Claim;
    use fsm_execute::run::native_client::{NativeHelperProgress, NativeShutdown};
    use fsm_store::store::{Store, VerifiedClosure};
    use std::time::Duration;

    let _: fn(&Store, &Claim, Duration) -> Result<NativeShutdown, String> = NativeShutdown::start;
    let _: fn(&mut NativeShutdown) -> Result<Option<VerifiedClosure>, String> =
        NativeShutdown::poll;
    let _: fn(&mut NativeShutdown) -> Result<bool, String> = NativeShutdown::reap;
    let _: fn(&NativeShutdown) -> NativeHelperProgress = NativeShutdown::progress;
    let _: fn(
        &NativeShutdown,
        &mut Store,
        &mut dyn fsm_store::clock::Clock,
    ) -> Result<fsm_core::json::Value, fsm_execute::error::ExecError> =
        NativeShutdown::settle_interrupted;
}

#[cfg(target_os = "linux")]
#[test]
fn interrupted_retirement_uses_original_runner_claim_and_separate_closure_transport() {
    use fsm_core::record::execution::Claim;
    use fsm_execute::{
        error::ExecError,
        run::{Runner, native_client::NativeShutdown},
        sched::Scheduler,
    };
    use fsm_store::store::Store;
    let _: fn(
        &mut Runner,
        &mut Store,
        &Claim,
        &mut NativeShutdown,
        &mut Scheduler,
    ) -> Result<bool, ExecError> = Runner::retire_native_interrupted;
}

#[cfg(target_os = "linux")]
#[test]
fn downstream_host_can_borrow_original_local_shutdown_targets() {
    let runner = fsm_execute::run::Runner::new_native().unwrap();
    let original: Option<&fsm_core::record::execution::Claim> = runner.local_native_claims().next();
    assert!(original.is_none());
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn downstream_admission_control_is_shared_and_bound_to_one_runner() {
    use fsm_execute::run::{NativeAdmissionControl, Runner};
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<NativeAdmissionControl>();
    let runner = Runner::new_native().unwrap();
    let control = runner.native_admission_control().unwrap();
    assert!(!control.is_closed());
    drop(runner);
    assert!(control.is_closed());
    let successor = Runner::new_native().unwrap();
    let successor_control = successor.native_admission_control().unwrap();
    control.close();
    assert!(!successor_control.is_closed());
    let legacy = Runner::new().unwrap();
    let Err(error) = legacy.native_admission_control() else {
        panic!("legacy control must refuse")
    };
    assert_eq!(error.code, "exec/mode");
}
