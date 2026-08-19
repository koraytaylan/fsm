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

/// Supplies timestamps to the store's injectable-clock mutation methods.
///
/// Implementing only [`Clock::now_ms`] remains supported. The provided
/// reservation methods preserve that implementation's eager-consumption
/// behavior; override both methods when abandoned reservations must not advance
/// the clock.
pub trait Clock {
    fn now_ms(&mut self) -> i64;

    /// Reserve the timestamp a later journal append will consume.
    ///
    /// A reservation may be abandoned without a matching commit. An override
    /// that defers advancement must therefore leave its visible state unchanged
    /// until [`Clock::commit_reserved_ms`] is called.
    ///
    /// Custom clocks keep their existing behavior by consuming immediately.
    /// Built-in clocks override this so validation can inspect the exact value
    /// without advancing injected time when the request is rejected.
    fn reserve_ms(&mut self) -> i64 {
        self.now_ms()
    }

    /// Commit a timestamp returned by [`Clock::reserve_ms`].
    ///
    /// Overrides must return `reserved`; the store uses the result as both the
    /// operation timestamp and journal timestamp.
    ///
    /// The default reservation already consumed the clock, so no further work
    /// is required. Built-in injected clocks override this to advance here.
    fn commit_reserved_ms(&mut self, reserved: i64) -> i64 {
        reserved
    }
}

/// Wall / `FSM_CLOCK_MS` / test `force_ms` clock used by CLI store paths.
pub struct GlobalClock;

impl Clock for GlobalClock {
    fn now_ms(&mut self) -> i64 {
        now_ms()
    }

    fn reserve_ms(&mut self) -> i64 {
        current_ms(false)
    }

    fn commit_reserved_ms(&mut self, reserved: i64) -> i64 {
        let _ = current_ms(true);
        reserved
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

    fn reserve_ms(&mut self) -> i64 {
        self.now
    }

    fn commit_reserved_ms(&mut self, reserved: i64) -> i64 {
        self.now = self.now.saturating_add(self.step);
        reserved
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

fn current_ms(advance: bool) -> i64 {
    if let Some(ts) = PINNED.with(|c| c.get()) {
        return ts;
    }
    let forced = FORCE.with(|c| c.get());
    let step = STEP.with(|c| c.get()).max(1);
    if forced != UNSET {
        let n = NEXT.with(|c| {
            let v = c.get();
            if advance {
                c.set(v.saturating_add(step));
            }
            v
        });
        return forced.saturating_add(n);
    }
    if let Ok(s) = std::env::var("FSM_CLOCK_MS") {
        if let Ok(start) = s.parse::<i64>() {
            let n = NEXT.with(|c| {
                let v = c.get();
                if advance {
                    c.set(v.saturating_add(step));
                }
                v
            });
            return start.saturating_add(n);
        }
    }
    wall_ms()
}

pub fn now_ms() -> i64 {
    current_ms(true)
}

#[cfg(test)]
mod tests {
    use super::{Clock, GlobalClock, force_ms, now_ms, reset_injected, set_step};

    struct EagerCustomClock {
        next: i64,
    }

    impl Clock for EagerCustomClock {
        fn now_ms(&mut self) -> i64 {
            let timestamp = self.next;
            self.next += 1;
            timestamp
        }
    }

    struct ResetInjected;

    impl Drop for ResetInjected {
        fn drop(&mut self) {
            reset_injected();
        }
    }

    #[test]
    fn injected_clock_saturates_instead_of_overflowing() {
        let _reset = ResetInjected;

        force_ms(i64::MAX);
        assert_eq!(now_ms(), i64::MAX);
        assert_eq!(now_ms(), i64::MAX);

        force_ms(i64::MAX - 1);
        set_step(i64::MAX);
        assert_eq!(now_ms(), i64::MAX - 1);
        assert_eq!(now_ms(), i64::MAX);
        assert_eq!(now_ms(), i64::MAX);
    }

    #[test]
    fn injected_clock_reservation_advances_only_when_committed() {
        let _reset = ResetInjected;
        let mut clock = GlobalClock;

        force_ms(100);
        let timestamp = clock.reserve_ms();
        assert_eq!(timestamp, 100);
        assert_eq!(clock.reserve_ms(), 100);
        assert_eq!(clock.commit_reserved_ms(timestamp), 100);
        assert_eq!(clock.reserve_ms(), 101);
    }

    #[test]
    fn custom_clock_implementing_only_now_ms_keeps_eager_defaults() {
        let mut clock = EagerCustomClock { next: 200 };

        let timestamp = clock.reserve_ms();
        assert_eq!(timestamp, 200);
        assert_eq!(clock.next, 201);
        assert_eq!(clock.commit_reserved_ms(timestamp), 200);
        assert_eq!(clock.next, 201);
    }
}
