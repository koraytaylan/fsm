//! Shared-tick native host operations, including portable capability boundaries.

use super::{Pipeline, Runner};
use crate::{error::ExecError, sched::Scheduler, watch::Observation};
use fsm_store::{clock::Clock, store::Store};

impl Runner {
    pub(crate) fn native_start_claim(
        &mut self,
        store: &mut Store,
        claim: &fsm_core::record::execution::Claim,
        scheduler: &mut Scheduler,
        timeout: std::time::Duration,
    ) -> Result<(), ExecError> {
        #[cfg(target_os = "linux")]
        return self.native.install(store, claim, scheduler, timeout);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (store, claim, scheduler, timeout);
            Err(ExecError::new(
                "exec/mode",
                "native startup requires a supported provisioned backend",
            ))
        }
    }

    pub(crate) fn cancel_native(&mut self, effect: &str) -> Option<Result<(), ExecError>> {
        #[cfg(target_os = "linux")]
        return self.native.cancel(effect);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = effect;
            None
        }
    }

    pub(crate) fn recover_native_owners(
        &mut self,
        snapshot: &Store,
        observation: &mut Observation,
    ) -> Result<(), ExecError> {
        #[cfg(target_os = "linux")]
        return self.native.adopt(snapshot, observation);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (snapshot, observation);
            Ok(())
        }
    }

    pub(crate) fn native_ready(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.native.ready();
        #[cfg(not(target_os = "linux"))]
        false
    }

    pub(crate) fn apply_native(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
        scheduler: &mut Scheduler,
    ) -> Option<Result<String, ExecError>> {
        #[cfg(target_os = "linux")]
        return self.native.apply(store, clock, pipeline, scheduler);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (store, clock, pipeline, scheduler);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original_owner() -> fsm_core::record::execution::Claim {
        use fsm_core::json::{JsonLimits, parse};
        use fsm_core::record::execution::Claim;
        Claim::from_value(&parse(br#"{
      "run_id":1,"instance_id":"case-1","effect_id":"case-1/3/0","attempt":1,
      "handler_fingerprint":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "retry":{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]},
      "domain":{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}
    }"#, &JsonLimits::DEFAULT).unwrap()).unwrap()
    }

    #[test]
    fn memory_startup_refusal_never_installs_or_dispatches_native_work() {
        let mut store = Store::open_memory().unwrap();
        let state = store.state.clone();
        let records = store.records.clone();
        let head = (store.journal.last_seq, store.journal.last_hash.clone());
        let mut scheduler = Scheduler::new(crate::config::HandlerTable::default());
        let mut runner = Runner::new().unwrap();
        assert_eq!(
            runner
                .start_native(
                    &mut store,
                    &original_owner(),
                    &mut scheduler,
                    std::time::Duration::from_secs(1),
                )
                .unwrap_err()
                .code,
            "exec/mode"
        );
        assert!(!runner.native_ready());
        assert!(runner.cancel_native("case-1/3/0").is_none());
        assert!(runner.finished_effects().is_empty());
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
        assert_eq!(store.records, records);
        assert_eq!(
            (store.journal.last_seq, store.journal.last_hash.clone()),
            head
        );
    }
}
