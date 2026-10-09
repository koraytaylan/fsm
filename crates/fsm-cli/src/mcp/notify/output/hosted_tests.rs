//! Hosted bounds include the actual writer's blocked in-flight frame.

use super::ProtocolOutput;
use crate::mcp::notify::{Notifier, SharedSink};
use fsm_core::json::Value;
use std::{
    io::{self, Write},
    sync::mpsc,
    time::Duration,
};

// Independent SPEC boundary; changing a production limit must break this proof.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

#[test]
fn hosted_notifier_saturation_runs_close_hook_once_outside_the_queue_lock() {
    let (notifier, output, observed, release) = held();
    let (closed, received) = mpsc::channel();
    let inspect = output.clone();
    output.on_failure(move || {
        assert!(inspect.is_broken());
        release.send(()).unwrap();
        closed.send(()).unwrap();
    });
    notifier.send(&Value::Null).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 1..64 {
        notifier.send(&Value::Null).unwrap();
    }
    assert_eq!(
        notifier.send(&Value::Null).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        notifier.send(&Value::Null).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(received.try_recv().is_err());
}

struct FailedWriter;
impl Write for FailedWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "actual transport failure",
        ))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn hosted_notifier_actual_write_failure_runs_registered_close_hook() {
    let (notifier, output) = Notifier::hosted_queued(Box::new(FailedWriter)).unwrap();
    let (closed, received) = mpsc::channel();
    let inspect = output.clone();
    output.on_failure(move || {
        assert!(inspect.is_broken());
        closed.send(()).unwrap();
    });
    notifier.send(&Value::Null).unwrap();
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(output.is_broken());
    assert!(!output.drained());
}

#[test]
fn hosted_notifier_late_close_hook_registration_observes_actual_write_failure() {
    let (notifier, output) = Notifier::hosted_queued(Box::new(FailedWriter)).unwrap();
    notifier.send(&Value::Null).unwrap();
    let watchdog = std::time::Instant::now() + Duration::from_secs(2);
    while !output.is_broken() {
        assert!(std::time::Instant::now() < watchdog);
        std::thread::sleep(Duration::from_millis(1));
    }
    let (closed, received) = mpsc::channel();
    output.on_failure(move || closed.send(()).unwrap());
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    output.on_failure(|| panic!("failure hook invoked twice"));
    assert!(received.try_recv().is_err());
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
                .recv_timeout(Duration::from_secs(30))
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "fixture writer release missing")
                })?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn held() -> (
    Notifier,
    ProtocolOutput,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::channel();
    let (notifier, output) = Notifier::hosted_queued(Box::new(HeldWriter {
        entered: Some(entered),
        release: resumed,
    }))
    .unwrap();
    (notifier, output, observed, release)
}

#[test]
fn hosted_notifier_accepts_64_including_inflight_and_refuses_65() {
    let (notifier, output, observed, release) = held();
    notifier.send(&Value::Null).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 1..64 {
        notifier.send(&Value::Null).unwrap();
    }
    let exact = output.0.0.lock().unwrap().charged_frames;
    let refused = notifier.send(&Value::Null).unwrap_err();
    let failed = output.is_broken();
    release.send(()).unwrap();
    assert_eq!(exact, 64);
    assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
    assert!(failed && notifier.is_broken());
    assert!(!output.drained());
}

#[test]
fn hosted_notifier_retained_byte_limit_accepts_exact_and_refuses_one_more() {
    for extra in [0, 1] {
        let (notifier, output, observed, release) = held();
        notifier
            .send(&Value::Str("x".repeat(MAX_FRAME_BYTES - 3)))
            .unwrap();
        observed.recv_timeout(Duration::from_secs(2)).unwrap();
        notifier
            .send(&Value::Str("x".repeat(MAX_FRAME_BYTES - 5 + extra)))
            .unwrap();
        let final_frame = notifier.send(&Value::Num("0".into()));
        let retained = output.0.0.lock().unwrap().charged_bytes;
        output.close();
        release.send(()).unwrap();
        if extra == 0 {
            assert!(final_frame.is_ok());
            assert_eq!(retained, 32 * 1024 * 1024);
        } else {
            assert_eq!(final_frame.unwrap_err().kind(), io::ErrorKind::WouldBlock);
            assert!(output.is_broken());
            assert_eq!(retained, 32 * 1024 * 1024 - 1);
        }
    }
}

#[test]
fn hosted_notifier_encoded_limit_accepts_exact_and_refuses_one_more_without_publishing() {
    let (notifier, output, observed, release) = held();
    notifier
        .send(&Value::Str("x".repeat(MAX_FRAME_BYTES - 3)))
        .unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    let retained = output.0.0.lock().unwrap().charged_bytes;
    output.close();
    release.send(()).unwrap();
    assert_eq!(retained, MAX_FRAME_BYTES);

    // Refuse on an empty queue: a 32 MiB queue guard must not mask the
    // independent 16 MiB encoded-frame guard.
    let sink = SharedSink::new();
    let (notifier, output) = Notifier::hosted_queued(Box::new(sink.writer())).unwrap();
    let refused = notifier
        .send(&Value::Str("x".repeat(MAX_FRAME_BYTES - 2)))
        .unwrap_err();
    let retained = output.0.0.lock().unwrap().charged_bytes;
    assert_eq!(refused.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(retained, 0);
    assert!(sink.text().is_empty());
    assert!(output.is_broken() && notifier.is_broken());
}
