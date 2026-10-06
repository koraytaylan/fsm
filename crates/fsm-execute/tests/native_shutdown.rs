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
