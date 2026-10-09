//! Shared-tick native host operations, including portable capability boundaries.

use super::{Pipeline, Runner};
use crate::{error::ExecError, sched::Scheduler, watch::Observation};
use fsm_store::{clock::Clock, store::Store};

impl Runner {
    #[cfg(target_os = "linux")]
    pub(crate) fn native_worker_scope(&self) -> super::native_client::worker::Scope {
        super::native_client::worker::Scope::enter_existing_or(&self.native_workers)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn take_native_cleanup_diagnostic(&mut self) -> Option<String> {
        self.native.take_cleanup_diagnostic()
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn native_preparation_inventory(&self) -> [usize; 10] {
        self.native.preparation_inventory()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn native_shutdown_inventory(&self) -> (Vec<u64>, usize, bool) {
        self.native.shutdown_inventory()
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn native_has_completion(&self, claim: &fsm_core::record::execution::Claim) -> bool {
        self.native.has_completion(claim)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn native_helper_retired(&self, claim: &fsm_core::record::execution::Claim) -> bool {
        self.native.helper_retired(claim)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn cancel_foreign_native_helpers(&mut self) {
        self.native.cancel_foreign_helpers();
    }

    pub(crate) fn uses_native_admission(&self) -> bool {
        self.native_admission
    }

    pub(crate) fn queue_native(
        &mut self,
        snapshot: &Store,
        effect: &crate::effect::PendingEffect,
        handler: &crate::config::HandlerSpec,
        scheduler: &mut Scheduler,
    ) -> Result<(), ExecError> {
        crate::contract::check_pending(snapshot, effect, scheduler.handler_table())?;
        #[cfg(target_os = "linux")]
        return self.native.queue(snapshot, effect, handler, scheduler);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (snapshot, effect, handler, scheduler);
            Err(ExecError::new(
                "exec/mode",
                "native admission requires a supported provisioned backend",
            ))
        }
    }

    pub(crate) fn accepts_native_writer(&self, store: &Store) -> bool {
        #[cfg(target_os = "linux")]
        return !store.journal.is_read_only()
            && !store.journal.is_memory()
            && !store.journal.poisoned
            && self.native.matches_store(store);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = store;
            false
        }
    }

    pub(crate) fn release_native_preparations(&mut self, scheduler: &mut Scheduler) {
        #[cfg(target_os = "linux")]
        self.native.release_preparations(scheduler);
        #[cfg(not(target_os = "linux"))]
        let _ = scheduler;
    }

    pub(crate) fn start_native_preparations(&mut self) {
        #[cfg(target_os = "linux")]
        self.native.start_preparations();
    }
    pub(crate) fn native_start_claim(
        &mut self,
        store: &mut Store,
        claim: &fsm_core::record::execution::Claim,
        scheduler: &mut Scheduler,
        timeout: std::time::Duration,
    ) -> Result<(), ExecError> {
        #[cfg(target_os = "linux")]
        {
            if self.native.admission_is_closed() {
                return Err(ExecError::new(
                    "exec/inflight_deferred",
                    "native admission is closed",
                ));
            }
            self.native.install(store, claim, scheduler, timeout)
        }
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
        scheduler: &mut Scheduler,
    ) -> Result<(), ExecError> {
        #[cfg(target_os = "linux")]
        return self.native.adopt(snapshot, observation, scheduler);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (snapshot, observation, scheduler);
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
    #[cfg(target_os = "linux")]
    pub(crate) fn native_retire_interrupted(
        &mut self,
        store: &mut Store,
        claim: &fsm_core::record::execution::Claim,
        shutdown: &mut super::native_client::NativeShutdown,
        scheduler: &mut Scheduler,
    ) -> Result<bool, ExecError> {
        self.native
            .retire_interrupted(store, claim, shutdown, scheduler)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn apply_native_completed(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
        scheduler: &mut Scheduler,
    ) -> Option<Result<String, ExecError>> {
        self.native
            .apply_completed(store, clock, pipeline, scheduler)
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
    #[test]
    fn native_preparation_checks_later_contract_before_backend_authority() {
        use crate::config::HandlerTable;
        use fsm_core::json::{JsonLimits, parse};
        let mut store = Store::open_memory().unwrap();
        let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
        let machine = parse(br#"{"format":"fsm.machine/1","name":"preparation_contract","context":[],"events":[{"name":"next","fields":[]}],"effects":[{"name":"work","fields":[]},{"name":"restore","fields":[]}],"states":[{"name":"first","entry":{"emit":[{"effect":"work"}]}},{"name":"later","entry":{"emit":[{"effect":"restore"}]}}],"initial":"first","transitions":[{"from":"first","on":"next","to":"later"}]}"#, &JsonLimits::DEFAULT).unwrap();
        store
            .define_machine_on(&mut clock, machine, false, false)
            .unwrap();
        store
            .create_instance_ctx_on(
                &mut clock,
                "preparation_contract",
                "case-1",
                "create",
                None,
                &std::collections::BTreeMap::new(),
                &[],
            )
            .unwrap();
        let effect =
            crate::effect::resolve(&store, &store.state.instances["case-1"].pending[0]).unwrap();
        let table = HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{"effect":"work","argv":["/operator/work"],"timeout_ms":1000},{"effect":"restore","argv":["/operator/restore"],"timeout_ms":1000,"on_ok":{"event":"undeclared","payload":{}}}]}"#).unwrap();
        let handler = table.handlers["work"].clone();
        let mut scheduler = Scheduler::new(table);
        let mut runner = Runner::new().unwrap();
        let state = store.state.clone();
        let records = store.records.clone();
        assert_eq!(
            runner
                .queue_native(&store, &effect, &handler, &mut scheduler)
                .unwrap_err()
                .code,
            "exec/contract_invalid"
        );
        assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
        assert_eq!(store.records, records);
        #[cfg(target_os = "linux")]
        assert_eq!(runner.native_preparation_inventory(), [0; 10]);
    }
}
