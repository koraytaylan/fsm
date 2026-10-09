//! Original stdio EOF retirement and protocol validation for native workflows.

use super::{Client, bounded_executor_errors};
use std::time::{Duration, Instant};

mod quiet_retry;
pub(super) use quiet_retry::{configure_table, failed_operation, unacknowledged_attempts};

impl Client {
    pub(super) fn finish(&mut self) {
        // EOF must retire the original owner; killing the server would conceal
        // a writer or native worker that survives successful settlement.
        drop(self.input.take());
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "EOF shutdown: {}",
                    bounded_executor_errors(&self.errors)
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "EOF retirement timed out: {}",
                bounded_executor_errors(&self.errors)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        self.reader.take().unwrap().join().unwrap();
        for frame in self.responses.try_iter() {
            frame.expect("remaining stdout must be valid JSON-RPC");
        }
    }
}
