//! Session-local writer access; no second journal owner and no Send bounds on borrowed APIs.
use super::super::serve::ExecutorLoop;
use super::{FEED_INTERVAL_MS, watch};
use crate::mcp::notify::{FeedHandle, Notifier};
use crate::{clock::Clock, store::Store};

pub(super) enum SessionStore<'a> {
    Borrowed(Option<&'a mut Store>),
    #[cfg(target_os = "linux")]
    Native(&'a mut fsm_execute::service::OwnedNativeExecutor),
    #[cfg(target_os = "linux")]
    Hosted {
        session: &'a crate::mcp::host::Session,
        data_dir: &'a std::path::Path,
    },
}
impl SessionStore<'_> {
    pub(super) fn hosted(&self) -> Option<(&crate::mcp::host::Session, &std::path::Path)> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Hosted { session, data_dir } => Some((session, data_dir)),
            _ => None,
        }
    }

    // A mutable facade borrow deliberately bounds every native writer view to
    // one request/inspection; the caller never holds it across idle observation.
    pub(super) fn as_deref(&mut self) -> Option<&Store> {
        match self {
            Self::Borrowed(store) => store.as_deref(),
            #[cfg(target_os = "linux")]
            Self::Native(driver) => driver.store_mut().map(|store| &*store),
            #[cfg(target_os = "linux")]
            Self::Hosted { .. } => None,
        }
    }
    pub(super) fn as_deref_mut(&mut self) -> Option<&mut Store> {
        match self {
            Self::Borrowed(store) => store.as_deref_mut(),
            #[cfg(target_os = "linux")]
            Self::Native(driver) => driver.store_mut(),
            #[cfg(target_os = "linux")]
            Self::Hosted { .. } => None,
        }
    }
    pub(super) fn observes_admitted(&self) -> bool {
        match self {
            Self::Borrowed(_) => false,
            #[cfg(target_os = "linux")]
            Self::Native(_) | Self::Hosted { .. } => true,
        }
    }
    pub(super) fn is_embedded(&self, borrowed_executor: bool) -> bool {
        match self {
            Self::Borrowed(_) => borrowed_executor,
            #[cfg(target_os = "linux")]
            Self::Native(_) | Self::Hosted { .. } => true,
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
            Self::Hosted { .. } => Vec::new(),
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
            Self::Hosted { .. } => Vec::new(),
            #[cfg(target_os = "linux")]
            Self::Native(driver) => {
                let now_ms = clock.now_ms();
                driver.poll(clock, now_ms)
            }
        }
    }
}

pub(super) struct SessionRuntime<'a> {
    #[cfg(target_os = "linux")]
    pub(super) diagnostics: Option<&'a mut crate::mcp::notify::diagnostic_output::DiagnosticOutput>,
    pub(super) store: SessionStore<'a>,
    pub(super) executor: Option<&'a mut ExecutorLoop>,
    pub(super) handlers: Option<fsm_core::json::Value>,
    pub(super) bounded_shutdown: bool,
}

pub(super) struct SessionLive {
    pub(super) state: super::Live,
    pub(super) bounded_shutdown: bool,
}
impl super::Live {
    pub(crate) fn for_http() -> (Self, std::sync::Arc<std::sync::atomic::AtomicBool>) {
        let retirement = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        (
            Self {
                http_retirement: Some(std::sync::Arc::clone(&retirement)),
                ..Self::default()
            },
            retirement,
        )
    }

    pub(crate) fn retire_http_feed(&mut self) {
        if let Some(mut feed) = self.feed.take() {
            feed.request_stop_nonblocking();
        }
    }
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

impl super::Live {
    /// Start the change feed if this session does not have one yet.
    ///
    /// The body is `5902`'s; until then a session's subscription is recorded
    /// and nothing polls. The lifecycle is decided here regardless, because
    /// deciding it after something is spawned is how a thread outlives its
    /// session.
    pub(crate) fn ensure_feed(&mut self, data_dir: Option<std::path::PathBuf>, output: &Notifier) {
        if self.feed.is_some()
            || self
                .http_retirement
                .as_ref()
                .is_some_and(|retirement| retirement.load(std::sync::atomic::Ordering::Acquire))
        {
            return;
        }
        let Some(data_dir) = data_dir else {
            return;
        };
        let writer = output.clone_handle();
        let watched = self.subscriptions.clone_handle();
        // The feed starts from wherever the journal is now: a subscriber
        // asked to be told what happens next, not what already had.
        let from_seq = crate::store::Store::open_read_only(&data_dir)
            .map(|store| store.journal.last_seq)
            .unwrap_or(0);
        // A test driving the feed by hand takes it here; everyone else gets
        // the timer. The session's own bookkeeping is the same either way,
        // so a hand-driven session is the same session.
        if watch::park(watch::Feed::new(&data_dir, watched, writer, from_seq)) {
            self.feed = Some(FeedHandle::parked());
            return;
        }
        let writer = output.clone_handle();
        let watched = self.subscriptions.clone_handle();
        let run = move |stop: &std::sync::atomic::AtomicBool| {
            let mut feed = watch::Feed::new(&data_dir, watched, writer, from_seq);
            feed.run(stop, FEED_INTERVAL_MS);
        };
        self.feed = Some(match &self.http_retirement {
            Some(retirement) => FeedHandle::spawn_with_stop(std::sync::Arc::clone(retirement), run),
            None => FeedHandle::spawn(run),
        });
    }
}
