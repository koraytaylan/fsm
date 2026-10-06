//! Downstream queued notifier framing and actual blocked-writer admission.

use fsm_cli::mcp::notify::{Notifier, OutputControl, SharedSink};
use fsm_core::json::Value;
use std::{
    io::{self, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

fn await_drain(control: &OutputControl) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !control.drained() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(control.drained());
    assert!(!control.is_broken());
}

#[test]
fn downstream_notifier_clones_preserve_complete_frame_order() {
    let sink = SharedSink::new();
    let (notifier, control) = Notifier::queued(Box::new(sink.writer())).unwrap();
    notifier.send(&Value::Str("first\nline".into())).unwrap();
    notifier
        .clone_handle()
        .send(&Value::Str("second".into()))
        .unwrap();
    control.close();
    await_drain(&control);
    assert_eq!(sink.bytes(), b"\"first\\nline\"\n\"second\"\n");
    assert_eq!(
        notifier.send(&Value::Null).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
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
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "fixture release missing"))?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn notifier_admission_counts_actual_blocked_inflight_frame() {
    let (entered, observed) = mpsc::sync_channel(1);
    let (release, resumed) = mpsc::channel();
    let (notifier, control) = Notifier::queued(Box::new(HeldWriter {
        entered: Some(entered),
        release: resumed,
    }))
    .unwrap();
    notifier.send(&Value::Null).unwrap();
    observed.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 1..256 {
        notifier.send(&Value::Null).unwrap();
    }
    assert_eq!(
        notifier.send(&Value::Null).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    control.close();
    assert!(!control.drained());
    release.send(()).unwrap();
    await_drain(&control);
}
