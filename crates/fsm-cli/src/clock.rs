//! The only wall-clock read in the system.

#![allow(clippy::collapsible_if)]

use std::sync::atomic::{AtomicI64, Ordering};

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
        now_ms()
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
        force_ms(self.now);
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

pub fn now_ms() -> i64 {
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
