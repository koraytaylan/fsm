//! Operator diagnostic admission uses the shared bounded output worker.
use crate::mcp::notify::OutputControl;
use std::io::{self, Write};

const MAX_LINE_BYTES: usize = 64 * 1024;

pub(crate) struct DiagnosticOutput {
    control: OutputControl,
    dropped: u64,
}
impl DiagnosticOutput {
    /// Share one bounded writer, retaining each producer's own rejection count.
    /// The composing caller must combine these counts when reporting delivery.
    pub(crate) fn fork(&self) -> Self {
        Self {
            control: self.control.clone(),
            dropped: 0,
        }
    }

    pub(crate) fn start(writer: impl Write + Send + 'static) -> io::Result<Self> {
        Ok(Self {
            control: OutputControl::start(writer)?,
            dropped: 0,
        })
    }
    // A saturated log stream cannot suspend the native owner. This count is
    // diagnostic loss, never execution loss or authenticated shutdown evidence.
    pub(crate) fn enqueue(&mut self, line: &str) -> io::Result<()> {
        if line.len() > MAX_LINE_BYTES || line.as_bytes().contains(&b'\n') {
            self.dropped = self.dropped.saturating_add(1);
            return Ok(());
        }
        let mut frame = Vec::with_capacity(line.len() + 1);
        frame.extend_from_slice(line.as_bytes());
        frame.push(b'\n');
        match self.control.enqueue_diagnostic(frame) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                self.dropped = self.dropped.saturating_add(1);
                Ok(())
            }
            outcome => outcome,
        }
    }
    pub(crate) fn dropped(&self) -> u64 {
        self.dropped
    }
    pub(crate) fn close(&self) {
        self.control.close();
    }
    pub(crate) fn drained(&self) -> bool {
        self.control.drained()
    }
    pub(crate) fn is_broken(&self) -> bool {
        self.control.is_broken()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    struct HeldWriter {
        ready: Option<mpsc::Sender<()>>,
        release: mpsc::Receiver<()>,
    }
    impl Write for HeldWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(ready) = self.ready.take() {
                ready.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(3)).unwrap();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn blocked_output_keeps_admission_and_close_bounded_with_loss_counted() {
        let (ready, observed) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let mut output = DiagnosticOutput::start(HeldWriter {
            ready: Some(ready),
            release: wait,
        })
        .unwrap();
        output.enqueue("first").unwrap();
        observed.recv_timeout(Duration::from_secs(1)).unwrap();
        let begin = Instant::now();
        for _ in 0..256 {
            output.enqueue("queued").unwrap();
        }
        output.close();
        let elapsed = begin.elapsed();
        let dropped = output.dropped();
        let drained = output.drained();
        // Release the real blocked writer before regression assertions.
        release.send(()).unwrap();
        assert!(elapsed < Duration::from_millis(500));
        assert_eq!(dropped, 1);
        assert!(!drained);
        let deadline = Instant::now() + Duration::from_secs(1);
        while !output.drained() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(output.drained());
    }

    #[test]
    fn malformed_and_oversized_lines_count_as_loss_without_breaking_writer() {
        let mut output = DiagnosticOutput::start(io::sink()).unwrap();
        output.enqueue("two\nlines").unwrap();
        output.enqueue(&"x".repeat(MAX_LINE_BYTES + 1)).unwrap();
        assert_eq!(output.dropped(), 2);
        assert!(!output.is_broken());
        output.close();
        assert!(output.drained());
    }
}
