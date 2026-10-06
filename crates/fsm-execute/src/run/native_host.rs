//! Shared-tick native host operations, including portable capability boundaries.

use super::{Pipeline, Runner};
use crate::{error::ExecError, watch::Observation};
use fsm_store::{clock::Clock, store::Store};

impl Runner {
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
    ) -> Option<Result<String, ExecError>> {
        #[cfg(target_os = "linux")]
        return self.native.apply(store, clock, pipeline);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (store, clock, pipeline);
            None
        }
    }
}
