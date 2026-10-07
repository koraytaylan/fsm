//! Original immutable evidence reads; joining never grants journal ownership.

use super::{NativeHelperProgress, worker::Ticket};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Instant,
};

struct Shared<T> {
    cancelled: AtomicBool,
    result: Mutex<Option<Result<T, String>>>,
}

pub(super) struct ProofWorker<T> {
    shared: Arc<Shared<T>>,
    thread: Option<JoinHandle<()>>,
    joined: bool,
    collected: bool,
}

impl<T: Send + 'static> ProofWorker<T> {
    pub(super) fn start(
        verify: impl FnOnce() -> Result<T, String> + Send + 'static,
        ticket: Arc<Ticket>,
        deadline: Instant,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            cancelled: AtomicBool::new(false),
            result: Mutex::new(None),
        });
        let published = Arc::clone(&shared);
        #[cfg(test)]
        let hook = BEFORE_PROOF.with(|hook| hook.borrow_mut().take());
        #[cfg(test)]
        let after = AFTER_PROOF.with(|hook| hook.borrow_mut().take());
        let thread = std::thread::Builder::new()
            .name("fsm-native-proof".into())
            .spawn(move || {
                let _unwind = super::unwind::WorkerUnwind::enter();
                let _ticket = ticket;
                #[cfg(test)]
                if let Some(hook) = hook {
                    hook();
                }
                let refusal = || {
                    if published.cancelled.load(Ordering::Acquire) {
                        Some("native proof cancelled; original ownership remains uncertain")
                    } else if Instant::now() >= deadline {
                        Some("native proof deadline; original ownership remains uncertain")
                    } else {
                        None
                    }
                };
                let result = match refusal() {
                    Some(error) => Err(error.into()),
                    None => {
                        let result = verify();
                        match refusal() {
                            Some(error) => Err(error.into()),
                            None => result,
                        }
                    }
                };
                *published
                    .result
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(result);
                #[cfg(test)]
                if let Some(after) = after {
                    after();
                }
            })
            .map_err(|_| "native proof worker unavailable; original ownership remains uncertain")?;
        Ok(Self {
            shared,
            thread: Some(thread),
            joined: false,
            collected: false,
        })
    }

    pub(super) fn poll(&mut self) -> Result<Option<T>, String> {
        self.reap();
        if self.collected {
            return Err("native proof already collected".into());
        }
        if !self.joined {
            return Ok(None);
        }
        let mut result = match self.shared.result.try_lock() {
            Ok(result) => result,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        };
        match result.take() {
            Some(result) => {
                self.collected = true;
                result.map(Some)
            }
            None => Ok(None),
        }
    }

    pub(super) fn reap(&mut self) -> bool {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            let thread = self.thread.take().expect("finished original proof worker");
            let result = thread.join();
            self.joined = true;
            if result.is_err() {
                *self
                    .shared
                    .result
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(Err(
                    "native proof worker panicked; original ownership remains uncertain".into(),
                ));
            }
        }
        self.joined
    }
}

impl<T> ProofWorker<T> {
    pub(super) fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
    }

    pub(super) fn withhold_retirement(
        &self,
        mut helper: NativeHelperProgress,
    ) -> NativeHelperProgress {
        if !self.joined {
            helper.not_started = false;
            helper.reaped = false;
        }
        helper
    }
}

impl<T> Drop for ProofWorker<T> {
    fn drop(&mut self) {
        self.cancel();
        // An unfinished original reader retains its reservation after detachment.
    }
}

#[cfg(test)]
type Hook = Box<dyn FnOnce() + Send>;

#[cfg(test)]
thread_local! {
    static BEFORE_PROOF: std::cell::RefCell<Option<Hook>> = const { std::cell::RefCell::new(None) };
    static AFTER_PROOF: std::cell::RefCell<Option<Hook>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
enum HookPhase {
    Before,
    After,
}

#[cfg(test)]
pub(super) struct FixtureHook {
    previous: Option<Hook>,
    phase: HookPhase,
}

#[cfg(test)]
impl FixtureHook {
    pub(super) fn install(hook: impl FnOnce() + Send + 'static) -> Self {
        Self {
            previous: BEFORE_PROOF.with(|current| current.replace(Some(Box::new(hook)))),
            phase: HookPhase::Before,
        }
    }

    pub(super) fn after_result(hook: impl FnOnce() + Send + 'static) -> Self {
        Self {
            previous: AFTER_PROOF.with(|current| current.replace(Some(Box::new(hook)))),
            phase: HookPhase::After,
        }
    }
}

#[cfg(test)]
impl Drop for FixtureHook {
    fn drop(&mut self) {
        match self.phase {
            HookPhase::Before => BEFORE_PROOF.with(|current| current.replace(self.previous.take())),
            HookPhase::After => AFTER_PROOF.with(|current| current.replace(self.previous.take())),
        };
    }
}

#[cfg(test)]
mod tests;
