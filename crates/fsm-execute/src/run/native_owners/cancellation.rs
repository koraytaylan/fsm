//! Cancel the original helper once, then collect its protected published result.
//!
//! SPEC execution settlement keeps cancelled claims until authenticated closure;
//! helper death is not proof, and recovery here never binds or launches work.

use super::{ExecError, NativeOwners, NativeRunPhase, Owner, deferred};

impl NativeOwners {
    pub(in crate::run) fn cancel(&mut self, effect: &str) -> Option<Result<(), ExecError>> {
        if let Some(result) = self.admissions.cancel(effect) {
            return Some(result);
        }
        let owner = self.owners.values_mut().find(|owner| {
            owner.claim.effect().1 == effect && owner.execution.progress().retained
        })?;
        if !owner.locally_admitted {
            return Some(Err(deferred()));
        }
        // Repeated scheduler Kill directives must not kill the read-only
        // recovery helper which consumes this original cancellation result.
        if owner.cancellation_requested {
            return Some(Ok(()));
        }
        owner.cancellation_requested = true;
        owner.cancelled_before_entry |= !owner.entry_requested;
        owner.entry_requested = true;
        Some(owner.execution.cancel().map_err(|error| {
            ExecError::new("exec/inflight_deferred", error)
                .hint("retain original native ownership until authenticated reconciliation")
        }))
    }
}

impl Owner {
    pub(super) fn can_collect_cancelled_completion(&self) -> bool {
        let progress = self.execution.progress();
        self.locally_admitted
            && self.cancellation_requested
            && !self.cancelled_before_entry
            && !self.cancelled_completion_requested
            && self.execution.completion().is_none()
            && progress.phase == NativeRunPhase::Uncertain
            && progress.helper.is_some_and(|helper| helper.is_retired())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::native_client::NativeExecution;
    use fsm_core::{
        json::{JsonLimits, parse},
        record::execution::Claim,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn cancelled_local_transport_can_collect_once_only_after_actual_retirement() {
        // Ordinary owned protocol helper, with no native domain/proof authority.
        let fixture = parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        let claim = Claim::from_value(fixture.get("claim").unwrap()).unwrap();
        let mut owners = NativeOwners::default();
        owners.retain(&claim, None).unwrap();
        let owner = owners.owners.get_mut(&claim.run_id()).unwrap();
        owner.locally_admitted = true;
        owner.execution = NativeExecution::bound_transport_fixture(&claim);
        owner.entry_requested = true;
        assert!(!owner.can_collect_cancelled_completion());
        owners.cancel(claim.effect().1).unwrap().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let owner = owners.owners.get_mut(&claim.run_id()).unwrap();
        while !owner.execution.reap().unwrap() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(owner.execution.progress().retained);
        assert!(owner.can_collect_cancelled_completion());
        owner.cancelled_completion_requested = true;
        assert!(!owner.can_collect_cancelled_completion());
        owner.cancelled_completion_requested = false;
        owner.cancelled_before_entry = true;
        assert!(!owner.can_collect_cancelled_completion());
        owner.cancelled_before_entry = false;
        owner.locally_admitted = false;
        assert!(!owner.can_collect_cancelled_completion());
        owner.locally_admitted = true;
        // A repeat does not cancel a newly installed collection transport.
        owner.execution = NativeExecution::bound_transport_fixture(&claim);
        owners.cancel(claim.effect().1).unwrap().unwrap();
        let owner = &owners.owners[&claim.run_id()];
        assert_eq!(owner.execution.progress().phase, NativeRunPhase::Bound);
        assert!(owner.execution.progress().retained);
    }
}
