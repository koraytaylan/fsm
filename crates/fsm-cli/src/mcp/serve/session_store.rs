//! Session-local writer access; no second journal owner and no Send bounds on borrowed APIs.
use super::super::serve::ExecutorLoop;
use crate::{clock::Clock, store::Store};

pub(super) enum SessionStore<'a> {
    Borrowed(Option<&'a mut Store>),
    #[cfg(target_os = "linux")]
    Native(&'a mut fsm_execute::service::OwnedNativeExecutor),
}
impl SessionStore<'_> {
    // A mutable facade borrow deliberately bounds every native writer view to
    // one request/inspection; the caller never holds it across idle observation.
    pub(super) fn as_deref(&mut self) -> Option<&Store> {
        match self {
            Self::Borrowed(store) => store.as_deref(),
            #[cfg(target_os = "linux")]
            Self::Native(driver) => driver.store_mut().map(|store| &*store),
        }
    }
    pub(super) fn as_deref_mut(&mut self) -> Option<&mut Store> {
        match self {
            Self::Borrowed(store) => store.as_deref_mut(),
            #[cfg(target_os = "linux")]
            Self::Native(driver) => driver.store_mut(),
        }
    }
    pub(super) fn observes_admitted(&self) -> bool {
        match self {
            Self::Borrowed(_) => false,
            #[cfg(target_os = "linux")]
            Self::Native(_) => true,
        }
    }
    pub(super) fn is_embedded(&self, borrowed_executor: bool) -> bool {
        match self {
            Self::Borrowed(_) => borrowed_executor,
            #[cfg(target_os = "linux")]
            Self::Native(_) => true,
        }
    }
    pub(super) fn tick(
        &mut self,
        executor: Option<&mut ExecutorLoop>,
        clock: &mut dyn Clock,
    ) -> Vec<String> {
        match self {
            Self::Borrowed(store) => {
                let (Some(executor), Some(store)) = (executor, store.as_deref_mut()) else {
                    return Vec::new();
                };
                if store.journal.is_read_only() {
                    return Vec::new();
                }
                executor.tick(store, clock)
            }
            #[cfg(target_os = "linux")]
            Self::Native(driver) => {
                let now_ms = clock.now_ms();
                driver.tick(clock, now_ms)
            }
        }
    }
    pub(super) fn observe_admitted(&mut self, clock: &mut dyn Clock) -> Vec<String> {
        match self {
            // Existing borrowed/legacy sessions retain their explicit timing.
            Self::Borrowed(_) => {
                let _ = clock;
                Vec::new()
            }
            #[cfg(target_os = "linux")]
            Self::Native(driver) => {
                let now_ms = clock.now_ms();
                driver.poll(clock, now_ms)
            }
        }
    }
}

pub(super) struct SessionRuntime<'a> {
    pub(super) store: SessionStore<'a>,
    pub(super) executor: Option<&'a mut ExecutorLoop>,
    pub(super) handlers: Option<fsm_core::json::Value>,
    pub(super) bounded_shutdown: bool,
}

pub(super) struct SessionLive {
    pub(super) state: super::Live,
    pub(super) bounded_shutdown: bool,
}
impl std::ops::Deref for SessionLive {
    type Target = super::Live;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}
impl std::ops::DerefMut for SessionLive {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}
impl Drop for SessionLive {
    fn drop(&mut self) {
        if self.bounded_shutdown {
            if let Some(mut feed) = self.state.feed.take() {
                feed.request_stop_nonblocking();
            }
            super::watch::release_parked();
        } else {
            self.state.shutdown();
        }
    }
}
