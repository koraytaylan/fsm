//! Shared admission closure only; not a shutdown report or native closure proof.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Close one original runner's admission fence independently of its worker.
/// Closing is irreversible and does not wait for journal or native I/O.
/// This handle does not prove worker retirement, writer release or domain closure.
#[derive(Clone)]
pub struct NativeAdmissionControl {
    closed: Arc<AtomicBool>,
}

impl NativeAdmissionControl {
    pub(super) fn from_state(closed: Arc<AtomicBool>) -> Self {
        Self { closed }
    }

    /// Close admission immediately; repeated closure leaves it closed.
    /// Already authorized transitions retain their original local reservations.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
    }

    /// Whether this original admission fence has been closed.
    /// This is not a worker health or cleanup-completion observation.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn a_control_clone_closes_the_original_fence_while_the_worker_is_held() {
        let state = Arc::new(AtomicBool::new(false));
        let control = NativeAdmissionControl::from_state(state.clone());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker_state = state.clone();
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            worker_state.load(Ordering::Acquire)
        });
        ready_rx.recv().unwrap();
        let other = control.clone();
        other.close();
        assert!(control.is_closed());
        control.close();
        assert!(other.is_closed());
        release_tx.send(()).unwrap();
        assert!(worker.join().unwrap());
    }

    #[test]
    fn a_stale_control_cannot_close_a_new_runner_incarnation() {
        let original = NativeAdmissionControl::from_state(Arc::new(AtomicBool::new(false)));
        let successor = NativeAdmissionControl::from_state(Arc::new(AtomicBool::new(false)));
        original.close();
        assert!(original.is_closed());
        assert!(!successor.is_closed());
    }
}
