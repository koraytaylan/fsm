//! Independent control metadata for the owned native driver.
//! Never hold the state mutex across native, journal or protocol I/O.

#[path = "request_state.rs"]
mod request_state;
use crate::{error::ExecError, run::NativeAdmissionControl};
use request_state::RequestState;
pub use request_state::{Phase as ExecutorPhase, StopMode as ShutdownMode};
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Instant,
};

#[derive(Clone, Debug)]
pub struct ShutdownReport {
    pub phase: ExecutorPhase,
    pub admission_closed: bool,
    /// Last observed original local IDs; completeness is explicit below.
    pub unresolved_run_ids: Vec<u64>,
    pub unclaimed_reservations: Option<usize>,
    pub inventory_complete: bool,
    pub helpers_retired: bool,
    pub writer_released: bool,
}

#[derive(Clone)]
pub struct ExecutorControl {
    admission: NativeAdmissionControl,
    shared: Arc<(Mutex<State>, Condvar)>,
}

pub struct ShutdownRequest {
    control: ExecutorControl,
    deadline: Instant,
}

struct State {
    request: RequestState,
    run_ids: Vec<u64>,
    unclaimed: Option<usize>,
    complete: bool,
    helpers_retired: bool,
    writer_released: bool,
    poisoned: bool,
}

impl State {
    fn report(&self, now: Instant, admission_closed: bool) -> ShutdownReport {
        ShutdownReport {
            admission_closed,
            phase: if self.poisoned {
                ExecutorPhase::Uncertain
            } else {
                self.request.phase(now)
            },
            unresolved_run_ids: self.run_ids.clone(),
            unclaimed_reservations: self.unclaimed,
            inventory_complete: self.complete && !self.poisoned,
            helpers_retired: self.helpers_retired && !self.poisoned,
            writer_released: self.writer_released && !self.poisoned,
        }
    }
}

impl ExecutorControl {
    // Only the owned driver constructs this, using its original runner's fence.
    pub(super) fn new(admission: NativeAdmissionControl) -> Self {
        Self {
            admission,
            shared: Arc::new((
                Mutex::new(State {
                    request: RequestState::new(),
                    run_ids: Vec::new(),
                    unclaimed: None,
                    complete: false,
                    helpers_retired: false,
                    writer_released: false,
                    poisoned: false,
                }),
                Condvar::new(),
            )),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        match self.shared.0.lock() {
            Ok(state) => state,
            Err(error) => {
                let mut state = error.into_inner();
                state.poisoned = true;
                state
            }
        }
    }

    /// Validate before closing admission, preserve the first absolute deadline.
    pub fn stop(&self, mode: ShutdownMode, timeout_ms: i64) -> Result<ShutdownRequest, ExecError> {
        let now = Instant::now();
        RequestState::validate_deadline(now, timeout_ms)?;
        self.admission.close();
        let mut state = self.state();
        let deadline = state.request.request(mode, timeout_ms, now)?;
        // A pre-request snapshot cannot prove quiescence of an in-flight worker.
        if state.request.phase(now) != ExecutorPhase::Stopped {
            state.complete = false;
        }
        drop(state);
        self.shared.1.notify_all();
        Ok(ShutdownRequest {
            control: self.clone(),
            deadline,
        })
    }

    pub fn report(&self) -> ShutdownReport {
        self.state()
            .report(Instant::now(), self.admission.is_closed())
    }

    pub(super) fn publish(
        &self,
        run_ids: Vec<u64>,
        unclaimed: usize,
        helpers_retired: bool,
        complete: bool,
        writer_released: bool,
    ) {
        let mut state = self.state();
        state.run_ids = run_ids;
        state.unclaimed = Some(unclaimed);
        state.helpers_retired = helpers_retired;
        state.complete = complete;
        state.writer_released = writer_released;
        if complete
            && helpers_retired
            && writer_released
            && state.run_ids.is_empty()
            && unclaimed == 0
        {
            state.request.record_stopped();
        }
        drop(state);
        self.shared.1.notify_all();
    }

    pub(super) fn requested(&self) -> bool {
        self.admission.is_closed()
    }

    pub(super) fn closure_requested(&self) -> bool {
        self.state().request.should_close_domains(Instant::now())
    }
}

impl ShutdownRequest {
    pub fn poll(&self) -> ShutdownReport {
        self.control.report()
    }

    /// Wait only on control metadata until actual completion or the first deadline.
    /// Worker I/O is never called here; expired/incomplete cleanup stays uncertain.
    pub fn wait(&self) -> ShutdownReport {
        let mut state = self.control.state();
        loop {
            let now = Instant::now();
            let report = state.report(now, self.control.admission.is_closed());
            if matches!(
                report.phase,
                ExecutorPhase::Stopped | ExecutorPhase::Uncertain
            ) {
                return report;
            }
            let Some(remaining) = self.deadline.checked_duration_since(now) else {
                return state.report(Instant::now(), self.control.admission.is_closed());
            };
            let outcome = self.control.shared.1.wait_timeout(state, remaining);
            state = match outcome {
                Ok((state, _)) => state,
                Err(error) => {
                    let (mut state, _) = error.into_inner();
                    state.poisoned = true;
                    state
                }
            };
        }
    }
}

// Only the owned driver publishes inventory and confirmed writer release.

#[cfg(all(test, any(target_arch = "x86_64", target_arch = "aarch64")))]
mod tests {
    use super::*;
    use crate::run::Runner;
    use std::sync::mpsc;

    #[test]
    fn invalid_stop_does_not_close_the_actual_runner_fence() {
        let runner = Runner::new_native().unwrap();
        let admission = runner.native_admission_control().unwrap();
        let control = ExecutorControl::new(admission.clone());
        let Err(error) = control.stop(ShutdownMode::Abort, 0) else {
            panic!("zero must refuse")
        };
        assert_eq!(error.code, "exec/config");
        assert!(!admission.is_closed());
        assert_eq!(control.report().phase, ExecutorPhase::Running);
    }

    #[test]
    fn an_unobserved_held_worker_cannot_hold_the_deadline_report() {
        let runner = Runner::new_native().unwrap();
        let control = ExecutorControl::new(runner.native_admission_control().unwrap());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            runner.native_admission_control().unwrap().is_closed()
        });
        ready_rx.recv().unwrap();
        let request = control.stop(ShutdownMode::Drain, 10).unwrap();
        let report = request.wait();
        assert_eq!(report.phase, ExecutorPhase::Uncertain);
        assert!(report.admission_closed);
        assert!(!report.inventory_complete);
        assert_eq!(report.unclaimed_reservations, None);
        assert!(!report.helpers_retired);
        assert!(!report.writer_released);
        // No worker observation or native cleanup proof has been published.
        release_tx.send(()).unwrap();
        assert!(worker.join().unwrap());
    }

    #[test]
    fn control_clones_escalate_without_renewing_the_original_deadline() {
        let runner = Runner::new_native().unwrap();
        let control = ExecutorControl::new(runner.native_admission_control().unwrap());
        let first = control.stop(ShutdownMode::Drain, 10000).unwrap();
        let second = control.clone().stop(ShutdownMode::Abort, 100000).unwrap();
        assert_eq!(first.deadline, second.deadline);
        assert_eq!(control.report().phase, ExecutorPhase::Stopping);
        let third = control.stop(ShutdownMode::Drain, 100000).unwrap();
        assert_eq!(first.deadline, third.deadline);
        assert_eq!(third.poll().phase, ExecutorPhase::Stopping);
        assert!(control.closure_requested());
    }
}
