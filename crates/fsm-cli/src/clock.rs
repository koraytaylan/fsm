//! The only wall-clock read in the system.

#![allow(clippy::collapsible_if)]

use std::cell::Cell;
use std::sync::atomic::{AtomicI64, Ordering};

thread_local! {
    static PINNED: Cell<Option<i64>> = const { Cell::new(None) };
}

pub struct PinGuard;

impl Drop for PinGuard {
    fn drop(&mut self) {
        PINNED.with(|c| c.set(None));
    }
}

/// Pin the timestamp used by `now_ms` for the duration of a mutating call.
pub fn pin(ts: i64) -> PinGuard {
    PINNED.with(|c| c.set(Some(ts)));
    PinGuard
}

const UNSET: i64 = i64::MIN;
static FORCE: AtomicI64 = AtomicI64::new(UNSET);
static NEXT: AtomicI64 = AtomicI64::new(0);
static STEP: AtomicI64 = AtomicI64::new(1);

pub trait Clock {
    fn now_ms(&mut self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&mut self) -> i64 {
        wall_ms()
    }
}

pub struct FixedClock {
    pub now: i64,
    pub step: i64,
}

impl FixedClock {
    pub fn new(start: i64, step: i64) -> Self {
        Self { now: start, step }
    }
}

impl Clock for FixedClock {
    fn now_ms(&mut self) -> i64 {
        let t = self.now;
        self.now = self.now.saturating_add(self.step);
        t
    }
}

pub fn force_ms(start: i64) {
    FORCE.store(start, Ordering::SeqCst);
    NEXT.store(0, Ordering::SeqCst);
}

pub fn set_step(step: i64) {
    STEP.store(step.max(1), Ordering::SeqCst);
}

pub fn reset_injected() {
    FORCE.store(UNSET, Ordering::SeqCst);
    NEXT.store(0, Ordering::SeqCst);
    STEP.store(1, Ordering::SeqCst);
}

fn wall_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn now_ms() -> i64 {
    if let Some(ts) = PINNED.with(|c| c.get()) {
        return ts;
    }
    let forced = FORCE.load(Ordering::SeqCst);
    let step = STEP.load(Ordering::SeqCst).max(1);
    if forced != UNSET {
        return forced + NEXT.fetch_add(step, Ordering::SeqCst);
    }
    if let Ok(s) = std::env::var("FSM_CLOCK_MS") {
        if let Ok(start) = s.parse::<i64>() {
            return start + NEXT.fetch_add(step, Ordering::SeqCst);
        }
    }
    wall_ms()
}
