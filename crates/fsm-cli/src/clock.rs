//! The only wall-clock read in the system.

#![allow(clippy::collapsible_if)]

use std::cell::Cell;

thread_local! {
    static PINNED: Cell<Option<i64>> = const { Cell::new(None) };
    static FORCE: Cell<i64> = const { Cell::new(UNSET) };
    static NEXT: Cell<i64> = const { Cell::new(0) };
    static STEP: Cell<i64> = const { Cell::new(1) };
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

pub trait Clock {
    fn now_ms(&mut self) -> i64;
}

/// Wall / `FSM_CLOCK_MS` / test `force_ms` clock used by CLI store paths.
pub struct GlobalClock;

impl Clock for GlobalClock {
    fn now_ms(&mut self) -> i64 {
        now_ms()
    }
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
    FORCE.with(|c| c.set(start));
    NEXT.with(|c| c.set(0));
}

pub fn set_step(step: i64) {
    STEP.with(|c| c.set(step.max(1)));
}

pub fn reset_injected() {
    FORCE.with(|c| c.set(UNSET));
    NEXT.with(|c| c.set(0));
    STEP.with(|c| c.set(1));
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
    let forced = FORCE.with(|c| c.get());
    let step = STEP.with(|c| c.get()).max(1);
    if forced != UNSET {
        let n = NEXT.with(|c| {
            let v = c.get();
            c.set(v.saturating_add(step));
            v
        });
        return forced + n;
    }
    if let Ok(s) = std::env::var("FSM_CLOCK_MS") {
        if let Ok(start) = s.parse::<i64>() {
            let n = NEXT.with(|c| {
                let v = c.get();
                c.set(v.saturating_add(step));
                v
            });
            return start + n;
        }
    }
    wall_ms()
}
