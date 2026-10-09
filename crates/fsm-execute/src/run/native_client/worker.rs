//! Original helper polling with reserved capacity and cached retirement facts.

use super::{InlineRequest, NativeHelperProgress};
use fsm_core::json::Value;
use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

const WORKER_SLOTS: usize = 128;
const TRANSPORT_CHARGE: usize = 16 * 1024 * 1024;
const RESPONSE_STORAGE: usize = 2 * 1024 * 1024;
pub(crate) const CAPACITY_EXHAUSTED: &str = "native transport worker capacity exhausted";

#[cfg(test)]
pub(crate) fn exhaust_capacity() -> impl Sized {
    let budget = Arc::new(Budget::default());
    let scope = Scope::enter(Some(&budget));
    let tickets = (0..WORKER_SLOTS)
        .map(|_| reserve_current().unwrap().unwrap())
        .collect::<Vec<_>>();
    (scope, tickets)
}

#[derive(Default)]
pub(crate) struct Budget(AtomicUsize);

#[cfg(test)]
impl Budget {
    pub(super) fn reserved(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

pub(super) struct Ticket(Arc<Budget>);

impl Drop for Ticket {
    fn drop(&mut self) {
        self.0.0.fetch_sub(1, Ordering::AcqRel);
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Arc<Budget>>> = const { RefCell::new(None) };
}

pub(crate) struct Scope(Option<Arc<Budget>>);

impl Scope {
    pub(crate) fn enter(budget: Option<&Arc<Budget>>) -> Self {
        Self(CURRENT.with(|current| current.replace(budget.cloned())))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|current| current.replace(self.0.take()));
    }
}

pub(super) fn reserve_current() -> Result<Option<Arc<Ticket>>, String> {
    CURRENT.with(|current| {
        let Some(budget) = current.borrow().clone() else {
            return Ok(None);
        };
        let mut count = budget.0.load(Ordering::Acquire);
        loop {
            // Fixed per-slot charge couples the count and byte ceilings.
            if count >= WORKER_SLOTS
                || count.saturating_mul(TRANSPORT_CHARGE) >= WORKER_SLOTS * TRANSPORT_CHARGE
            {
                return Err(CAPACITY_EXHAUSTED.into());
            }
            match budget.0.compare_exchange_weak(
                count,
                count + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(Some(Arc::new(Ticket(budget)))),
                Err(observed) => count = observed,
            }
        }
    })
}

#[derive(Default)]
struct Shared {
    cancelled: AtomicBool,
    progress: AtomicU8,
    response: Mutex<Option<Result<Value, String>>>,
}

pub(super) struct Worker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    joined: bool,
    collected: bool,
}

impl Worker {
    pub(super) fn start_prepared(
        startup: super::startup::Startup,
        ticket: Arc<Ticket>,
    ) -> Result<Self, String> {
        Self::start_prepared_after_retirement(startup, ticket, || {})
    }

    fn start_prepared_after_retirement(
        startup: super::startup::Startup,
        ticket: Arc<Ticket>,
        after_retirement: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared::default());
        let published = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("fsm-native-transport".into())
            .spawn(move || {
                let _unwind = super::unwind::WorkerUnwind::enter();
                let _ticket = ticket;
                let request = if published.cancelled.load(Ordering::Acquire) {
                    Err(
                        "native client cancelled before startup; ownership remains uncertain"
                            .into(),
                    )
                } else if std::time::Instant::now() >= startup.deadline() {
                    Err("native client deadline before startup; ownership remains uncertain".into())
                } else {
                    startup.start()
                };
                match request {
                    Ok(request) => run(request, &published),
                    Err(error) => {
                        *published
                            .response
                            .lock()
                            .unwrap_or_else(|error| error.into_inner()) = Some(Err(error));
                        // No Child was returned by startup; actual reap/EOF bits
                        // remain false, and this observation is withheld until join.
                        published.progress.store(8, Ordering::Release);
                    }
                }
                after_retirement();
            })
            .map_err(|_| "native transport worker unavailable; no helper started")?;
        Ok(Self {
            shared,
            thread: Some(thread),
            joined: false,
            collected: false,
        })
    }

    pub(super) fn start(
        request: InlineRequest,
        ticket: Arc<Ticket>,
    ) -> Result<Self, Box<InlineRequest>> {
        Self::start_after_retirement(request, ticket, || {})
    }

    fn start_after_retirement(
        request: InlineRequest,
        ticket: Arc<Ticket>,
        after_retirement: impl FnOnce() + Send + 'static,
    ) -> Result<Self, Box<InlineRequest>> {
        Self::start_with_probes(request, ticket, |_| {}, after_retirement)
    }

    fn start_with_probes(
        request: InlineRequest,
        ticket: Arc<Ticket>,
        after_pending: impl FnMut(&mut InlineRequest) + Send + 'static,
        after_retirement: impl FnOnce() + Send + 'static,
    ) -> Result<Self, Box<InlineRequest>> {
        // Retain the original request if OS thread creation fails; dropping a
        // failed spawn closure must not drop/reap its helper on the owner.
        let original = Arc::new(Mutex::new(Some(request)));
        let delivered = Arc::clone(&original);
        let shared = Arc::new(Shared::default());
        let published = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("fsm-native-transport".into())
            .spawn(move || {
                let _unwind = super::unwind::WorkerUnwind::enter();
                let _ticket = ticket;
                let request = delivered
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .expect("original helper transferred once");
                run_after_pending(request, &published, after_pending);
                after_retirement();
            });
        match thread {
            Ok(thread) => Ok(Self {
                shared,
                thread: Some(thread),
                joined: false,
                collected: false,
            }),
            Err(_) => Err(Box::new(
                original
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .expect("failed worker retains original helper"),
            )),
        }
    }

    fn join_finished(&mut self) {
        if self.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            let thread = self.thread.take().expect("finished original worker");
            let result = thread.join();
            self.joined = true;
            if result.is_err() {
                *self
                    .shared
                    .response
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(Err(
                    "native transport worker panicked; ownership remains uncertain".into(),
                ));
            }
        }
    }

    pub(super) fn poll(&mut self) -> Result<Option<Value>, String> {
        self.join_finished();
        if self.collected {
            return Err("native client response already collected".into());
        }
        // A successful response is not delivered before actual worker join.
        if !self.joined {
            return Ok(None);
        }
        let mut response = match self.shared.response.try_lock() {
            Ok(response) => response,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(None),
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        };
        match response.take() {
            Some(Ok(value)) => {
                self.collected = true;
                Ok(Some(value))
            }
            Some(Err(error)) => {
                self.collected = true;
                Err(error)
            }
            None => {
                self.collected = true;
                Err(
                    "native transport worker ended without a response; ownership remains uncertain"
                        .into(),
                )
            }
        }
    }

    pub(super) fn cancel(&self) {
        self.shared.cancelled.store(true, Ordering::Release);
    }

    pub(super) fn progress(&self) -> NativeHelperProgress {
        let bits = self.shared.progress.load(Ordering::Acquire);
        NativeHelperProgress {
            not_started: bits & 8 != 0 && self.joined,
            reaped: bits & 1 != 0 && self.joined,
            stdout_eof: bits & 2 != 0,
            stderr_eof: bits & 4 != 0,
        }
    }

    pub(super) fn reap(&mut self) -> bool {
        self.join_finished();
        let progress = self.progress();
        progress.is_retired()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        // A detached unfinished worker keeps its ticket and original helper;
        // no caller may report this drop as observed transport retirement.
    }
}

fn run(request: InlineRequest, shared: &Shared) {
    run_after_pending(request, shared, |_| {});
}

// A fixture may release its real helper after a pending poll; production does
// nothing here and keeps the same original request and absolute deadline.
fn run_after_pending(
    mut request: InlineRequest,
    shared: &Shared,
    mut after_pending: impl FnMut(&mut InlineRequest),
) {
    let mut response = None;
    loop {
        if shared.cancelled.load(Ordering::Acquire) {
            let _ = request.cancel();
        }
        if response.is_none() {
            match request.poll() {
                Ok(Some(value)) => {
                    response = Some(if storage_fits(&value) {
                        Ok(value)
                    } else {
                        Err("native client response retained storage exceeds bound".into())
                    });
                }
                Ok(None) => {}
                Err(error) => response = Some(Err(error)),
            }
        }
        if response.is_none() {
            after_pending(&mut request);
        }
        let retired = request.reap().unwrap_or(false);
        let progress = request.progress();
        shared.progress.store(
            u8::from(progress.reaped)
                | (u8::from(progress.stdout_eof) << 1)
                | (u8::from(progress.stderr_eof) << 2),
            Ordering::Release,
        );
        // Reap can observe exit/EOF after poll returned pending; decode on the
        // next original poll before retiring this worker (SPEC transport rule).
        if retired && response.is_some() {
            *shared
                .response
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = response;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn storage_fits(value: &Value) -> bool {
    fn charge(value: &Value, depth: u32, remaining: &mut usize) -> Option<()> {
        if matches!(value, Value::Arr(_) | Value::Obj(_))
            && depth >= fsm_core::json::JsonLimits::DEFAULT.max_depth
        {
            return None;
        }
        *remaining = remaining.checked_sub(std::mem::size_of::<Value>())?;
        match value {
            Value::Str(text) | Value::Num(text) => {
                *remaining = remaining.checked_sub(text.capacity())?;
            }
            Value::Arr(entries) => {
                *remaining = remaining.checked_sub(
                    entries
                        .capacity()
                        .checked_mul(std::mem::size_of::<Value>())?,
                )?;
                for entry in entries {
                    charge(entry, depth + 1, remaining)?;
                }
            }
            Value::Obj(entries) => {
                for (key, entry) in entries {
                    *remaining = remaining.checked_sub(4096usize.checked_add(key.capacity())?)?;
                    charge(entry, depth + 1, remaining)?;
                }
            }
            _ => {}
        }
        Some(())
    }
    let mut remaining = RESPONSE_STORAGE;
    charge(value, 0, &mut remaining).is_some()
}

#[cfg(test)]
mod tests;
