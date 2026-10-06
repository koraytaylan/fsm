//! Complete-frame output queue; in-flight allocation remains charged while blocked.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Condvar, Mutex};

const MAX_FRAMES: usize = 256;
const MAX_ALLOCATION_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct State {
    frames: VecDeque<Vec<u8>>,
    charged_frames: usize,
    charged_bytes: usize,
    closed: bool,
    broken: bool,
}

/// Bounded complete-frame output control; queue acceptance is not delivery.
#[derive(Clone)]
pub struct ProtocolOutput(Arc<(Mutex<State>, Condvar)>);

impl ProtocolOutput {
    pub(crate) fn start(mut writer: impl Write + Send + 'static) -> io::Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("fsm-protocol-output".into())
            .spawn(move || {
                loop {
                    let (lock, wake) = &*worker;
                    let frame = {
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        while state.frames.is_empty() && !state.closed {
                            state = wake.wait(state).unwrap_or_else(|error| error.into_inner());
                        }
                        let Some(frame) = state.frames.pop_front() else {
                            return;
                        };
                        frame
                    };
                    let result = writer.write_all(&frame).and_then(|()| writer.flush());
                    let allocation = frame.capacity();
                    drop(frame);
                    let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                    state.charged_frames -= 1;
                    state.charged_bytes -= allocation;
                    if result.is_err() {
                        state.broken = true;
                        state.closed = true;
                        state.frames.clear();
                        state.charged_frames = 0;
                        state.charged_bytes = 0;
                        wake.notify_all();
                        return;
                    }
                    wake.notify_all();
                }
            })?;
        Ok(Self(shared))
    }

    // Caller supplies one canonical protocol frame including its final newline.
    // Accounting measures retained Vec allocation capacity, including the frame
    // currently in write_all; serialization temporaries are outside this budget.
    pub(crate) fn enqueue(&self, frame: Vec<u8>) -> io::Result<()> {
        if frame.last() != Some(&b'\n') || frame[..frame.len() - 1].contains(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output frame differs",
            ));
        }
        self.enqueue_diagnostic(frame)
    }

    // Operator diagnostics preserve rendered bytes, including human multiline
    // errors; they share the protocol queue's retained/in-flight accounting.
    // Only enqueue above accepts a protocol frame and validates its framing.
    pub(crate) fn enqueue_diagnostic(&self, frame: Vec<u8>) -> io::Result<()> {
        let (lock, wake) = &*self.0;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "output closed"));
        }
        let charged = state.charged_bytes.checked_add(frame.capacity());
        if state.charged_frames == MAX_FRAMES
            || charged.is_none_or(|bytes| bytes > MAX_ALLOCATION_BYTES)
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "output queue budget exhausted",
            ));
        }
        state.charged_bytes = charged.unwrap();
        state.charged_frames += 1;
        state.frames.push_back(frame);
        wake.notify_one();
        Ok(())
    }

    /// Whether the actual output worker has failed a write or flush.
    pub fn is_broken(&self) -> bool {
        self.0
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .broken
    }

    /// Close admission without waiting for a blocked writer.
    pub fn close(&self) {
        let (lock, wake) = &*self.0;
        lock.lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed = true;
        wake.notify_all();
    }

    /// True only after closed admission and successful delivery of all frames.
    pub fn drained(&self) -> bool {
        let state = self.0.0.lock().unwrap_or_else(|error| error.into_inner());
        state.closed && !state.broken && state.charged_frames == 0
    }
}

// Production integration must explicitly close this queue; clone Drop cannot
// promise drainage, and the lifecycle report must not join a blocked writer.
// Required tests: exact allocation/frame ceilings, plus-one, blocked in-flight
// charging, single-line ordering, output failure and bounded close observation.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn parked_queue() -> ProtocolOutput {
        ProtocolOutput(Arc::new((Mutex::new(State::default()), Condvar::new())))
    }

    fn frame(capacity: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(b"{}\n");
        assert_eq!(bytes.capacity(), capacity);
        bytes
    }

    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn diagnostic_delivery_preserves_actual_human_and_json_error_rendering() {
        for json in [false, true] {
            let error = crate::store::ErrorObj::new("exec/inflight_deferred", "cleanup uncertain")
                .hint("inspect the original owner before retrying");
            let mut rendered = Vec::new();
            crate::render::write_error(json, false, &error, &mut rendered);
            if !json {
                assert_eq!(
                    parked_queue().enqueue(rendered.clone()).unwrap_err().kind(),
                    io::ErrorKind::InvalidInput
                );
            }
            let captured = Arc::new(Mutex::new(Vec::new()));
            let output = ProtocolOutput::start(CapturedWriter(captured.clone())).unwrap();
            output.enqueue_diagnostic(rendered.clone()).unwrap();
            output.close();
            let deadline = Instant::now() + Duration::from_secs(2);
            while !output.drained() && Instant::now() < deadline {
                std::thread::yield_now();
            }
            assert!(output.drained());
            assert_eq!(*captured.lock().unwrap(), rendered);
        }
    }

    #[test]
    fn diagnostic_frames_share_allocation_and_frame_ceilings() {
        let output = parked_queue();
        output
            .enqueue_diagnostic(frame(MAX_ALLOCATION_BYTES))
            .unwrap();
        assert_eq!(
            output.enqueue_diagnostic(frame(3)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            output.0.0.lock().unwrap().charged_bytes,
            MAX_ALLOCATION_BYTES
        );
        let oversized = parked_queue();
        assert_eq!(
            oversized
                .enqueue_diagnostic(frame(MAX_ALLOCATION_BYTES + 1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(oversized.0.0.lock().unwrap().charged_bytes, 0);
        let empty = parked_queue();
        for _ in 0..MAX_FRAMES {
            empty.enqueue_diagnostic(Vec::new()).unwrap();
        }
        assert_eq!(
            empty.enqueue_diagnostic(Vec::new()).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(empty.0.0.lock().unwrap().charged_frames, MAX_FRAMES);
    }

    #[test]
    fn exact_frame_count_accepts_and_plus_one_refuses_atomically() {
        let output = parked_queue();
        for _ in 0..MAX_FRAMES {
            output.enqueue(frame(3)).unwrap();
        }
        assert_eq!(
            output.enqueue(frame(3)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        let state = output.0.0.lock().unwrap();
        assert_eq!(state.frames.len(), MAX_FRAMES);
        assert_eq!(state.charged_frames, MAX_FRAMES);
        assert_eq!(state.charged_bytes, MAX_FRAMES * 3);
    }

    #[test]
    fn allocation_capacity_ceiling_accepts_and_plus_one_refuses() {
        let output = parked_queue();
        output.enqueue(frame(MAX_ALLOCATION_BYTES)).unwrap();
        assert_eq!(
            output.enqueue(frame(3)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        let state = output.0.0.lock().unwrap();
        assert_eq!(state.charged_bytes, MAX_ALLOCATION_BYTES);
        assert_eq!(state.frames.len(), 1);
        drop(state);
        let oversized = parked_queue();
        assert_eq!(
            oversized
                .enqueue(frame(MAX_ALLOCATION_BYTES + 1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(oversized.0.0.lock().unwrap().charged_bytes, 0);
    }

    struct HeldWriter {
        entered: Option<mpsc::SyncSender<()>>,
        release: mpsc::Receiver<()>,
    }

    impl Write for HeldWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(entered) = self.entered.take() {
                entered.send(()).unwrap();
                self.release
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::TimedOut, "test writer release missing")
                    })?;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn blocked_inflight_allocation_stays_charged_until_actual_write_finishes() {
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, resumed) = mpsc::channel();
        let output = ProtocolOutput::start(HeldWriter {
            entered: Some(entered),
            release: resumed,
        })
        .unwrap();
        output.enqueue(frame(MAX_ALLOCATION_BYTES)).unwrap();
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            output.enqueue(frame(3)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        output.close();
        assert!(!output.drained());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !output.drained() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(output.drained());
    }
    #[test]
    fn malformed_frame_refuses_without_claiming_queue_budget() {
        let output = parked_queue();
        for bytes in [Vec::new(), b"{}".to_vec(), b"{}\n{}\n".to_vec()] {
            assert_eq!(
                output.enqueue(bytes).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let state = output.0.0.lock().unwrap();
        assert_eq!(state.charged_frames, 0);
        assert_eq!(state.charged_bytes, 0);
    }

    struct FailedWriter;
    impl Write for FailedWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture closed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn failed_write_closes_queue_without_claiming_successful_drain() {
        let output = ProtocolOutput::start(FailedWriter).unwrap();
        output.enqueue(frame(3)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if output.0.0.lock().unwrap().broken {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!output.drained());
        assert_eq!(
            output.enqueue(frame(3)).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        let state = output.0.0.lock().unwrap();
        assert_eq!(state.charged_frames, 0);
        assert_eq!(state.charged_bytes, 0);
    }
}
