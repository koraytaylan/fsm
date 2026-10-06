//! Request/deadline metadata only; no native cleanup or stopped proof.

use crate::{config::MAX_TIMEOUT_MS, error::ExecError};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopMode {
    Drain,
    Abort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Draining,
    Stopping,
    Stopped,
    Uncertain,
}

pub(super) struct RequestState {
    mode: Option<StopMode>,
    deadline: Option<Instant>,
    stopped: bool,
}

impl RequestState {
    pub(super) fn new() -> Self {
        Self {
            mode: None,
            deadline: None,
            stopped: false,
        }
    }

    pub(super) fn validate_deadline(now: Instant, timeout_ms: i64) -> Result<Instant, ExecError> {
        if !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(ExecError::new(
                "exec/config",
                format!("shutdown timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"),
            ));
        }
        now.checked_add(Duration::from_millis(timeout_ms as u64))
            .ok_or_else(|| ExecError::new("exec/config", "shutdown deadline exceeds clock range"))
    }

    // The control wrapper validates before closing admission; mutation is metadata only.
    pub(super) fn request(
        &mut self,
        mode: StopMode,
        timeout_ms: i64,
        now: Instant,
    ) -> Result<Instant, ExecError> {
        let candidate = Self::validate_deadline(now, timeout_ms)?;
        let deadline = *self.deadline.get_or_insert(candidate);
        self.mode = Some(match (self.mode, mode) {
            (Some(StopMode::Abort), _) | (_, StopMode::Abort) => StopMode::Abort,
            _ => StopMode::Drain,
        });
        Ok(deadline)
    }

    pub(super) fn phase(&self, now: Instant) -> Phase {
        if self.stopped {
            return Phase::Stopped;
        }
        let Some(mode) = self.mode else {
            return Phase::Running;
        };
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            return Phase::Uncertain;
        }
        match mode {
            StopMode::Drain => Phase::Draining,
            StopMode::Abort => Phase::Stopping,
        }
    }

    pub(super) fn should_close_domains(&self, now: Instant) -> bool {
        self.mode == Some(StopMode::Abort) || self.deadline.is_some_and(|deadline| now >= deadline)
    }

    // Called only by the future owned driver after actual retirement and writer
    // release; request metadata alone must never invoke this or prove cleanup.
    pub(super) fn record_stopped(&mut self) -> bool {
        if self.mode.is_none() {
            return false;
        }
        self.stopped = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_request_preserves_running_metadata_before_admission_mutation() {
        let now = Instant::now();
        for timeout in [i64::MIN, -1, 0, MAX_TIMEOUT_MS + 1, i64::MAX] {
            let mut state = RequestState::new();
            assert_eq!(
                state
                    .request(StopMode::Abort, timeout, now)
                    .unwrap_err()
                    .code,
                "exec/config"
            );
            assert_eq!(state.phase(now), Phase::Running);
            assert!(!state.should_close_domains(now));
        }
        let mut state = RequestState::new();
        assert!(state.request(StopMode::Drain, MAX_TIMEOUT_MS, now).is_ok());
    }

    #[test]
    fn repeated_controls_preserve_first_deadline_and_abort_never_deescalates() {
        let now = Instant::now();
        let mut state = RequestState::new();
        let original = state.request(StopMode::Drain, 10, now).unwrap();
        assert_eq!(state.phase(now), Phase::Draining);
        assert!(!state.should_close_domains(now));
        assert_eq!(
            state
                .request(StopMode::Drain, 1000, now + Duration::from_millis(1))
                .unwrap(),
            original
        );
        assert_eq!(
            state
                .request(StopMode::Abort, 1000, now + Duration::from_millis(2))
                .unwrap(),
            original
        );
        assert_eq!(state.phase(now), Phase::Stopping);
        assert!(state.should_close_domains(now));
        state.request(StopMode::Drain, 1000, now).unwrap();
        assert_eq!(state.phase(now), Phase::Stopping);
    }

    #[test]
    fn elapsed_deadline_selects_uncertainty_without_worker_observation() {
        let now = Instant::now();
        let mut state = RequestState::new();
        let deadline = state.request(StopMode::Drain, 1, now).unwrap();
        assert_eq!(state.phase(deadline), Phase::Uncertain);
        assert!(state.should_close_domains(deadline));
        assert_eq!(
            state
                .request(StopMode::Drain, MAX_TIMEOUT_MS, deadline)
                .unwrap(),
            deadline
        );
        assert_eq!(state.phase(deadline), Phase::Uncertain);
    }

    #[test]
    fn stopped_transition_requires_an_existing_request_and_is_metadata_only() {
        let now = Instant::now();
        let mut state = RequestState::new();
        assert!(!state.record_stopped());
        state.request(StopMode::Abort, 1, now).unwrap();
        assert!(state.record_stopped());
        assert_eq!(state.phase(now), Phase::Stopped);
        assert_eq!(state.phase(now + Duration::from_secs(1)), Phase::Stopped);
    }
}
