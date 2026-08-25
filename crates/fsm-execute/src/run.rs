//! Subprocess execution and the one component that writes.
//!
//! The runner spawns, reaps, and kills handler processes and reports what
//! happened; the pipeline maps that outcome onto journaled reality through the
//! store's own idempotent mutators. The split is deliberate: the runner owns
//! no policy, and the pipeline spawns nothing.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Child;

use fsm_core::json::Value;
use fsm_store::clock::Clock;
use fsm_store::store::Store;

use crate::config::{Advance, HandlerSpec};
use crate::effect::PendingEffect;
use crate::error::ExecError;

/// Bytes of one captured stream that reach the journal.
pub const ACK_OUTPUT_CAP: usize = 4096;

/// A capped capture of one output stream, digested when it overflows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedBytes {
    /// At most [`ACK_OUTPUT_CAP`] bytes from the front of the stream.
    pub bytes: Vec<u8>,
    /// Whether the stream was longer than the cap.
    pub truncated: bool,
    /// Hex SHA-256 of the *whole* stream, present only when truncated.
    pub sha256: Option<String>,
}

impl BoundedBytes {
    /// Render the capture as a valid JSON string, lossily and on a character
    /// boundary. Handler output is arbitrary bytes; a record body is canonical
    /// JSON. This conversion must never fail.
    pub fn to_json_string(&self) -> String {
        let _ = (&self.bytes, self.truncated, &self.sha256);
        unimplemented!("task 3801")
    }
}

/// Why an in-flight handler was killed. A timeout and a cancel are different
/// facts about the run, and the ack records which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillReason {
    /// The run passed its handler's `timeout_ms`.
    Timeout,
    /// The instance was cancelled while the run was in flight.
    Cancelled,
}

/// What one handler run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// The process exited on its own.
    Completed {
        /// Exit status, or `-1` for a process killed by a signal.
        status: i32,
        /// Captured standard output.
        stdout: BoundedBytes,
        /// Captured standard error.
        stderr: BoundedBytes,
    },
    /// The executor stopped it.
    Killed {
        /// Timeout or cancellation.
        reason: KillReason,
    },
    /// It never started.
    SpawnFailed {
        /// The command that could not be spawned.
        argv0: String,
    },
}

impl RunOutcome {
    /// The deterministic `result` the ack is fingerprinted over.
    ///
    /// No timestamp, duration, or pid may enter it: the store keys idempotency
    /// on the content, so anything varying between the write and a later
    /// re-issue turns a replay into a conflict.
    pub fn ack_result(&self) -> Value {
        unimplemented!("task 3801")
    }
}

/// The only component that spawns processes.
pub struct Runner {
    scratch: PathBuf,
    children: BTreeMap<String, Child>,
}

impl Runner {
    /// Create the capture directory this runner owns.
    pub fn new() -> Result<Self, ExecError> {
        unimplemented!("task 3801")
    }

    /// Start one handler, capturing both streams to files.
    pub fn spawn(&mut self, effect_id: String, argv: &[String]) -> Result<(), ExecError> {
        let _ = (&self.scratch, &self.children, effect_id, argv);
        unimplemented!("task 3801")
    }

    /// Reap a finished child, non-blocking. The only caller of `try_wait`.
    pub fn poll(&mut self, effect_id: &str) -> Option<RunOutcome> {
        let _ = effect_id;
        unimplemented!("task 3801")
    }

    /// Stop an in-flight child and reap it.
    pub fn kill(&mut self, effect_id: &str, reason: KillReason) -> RunOutcome {
        let _ = (effect_id, reason);
        unimplemented!("task 3801")
    }
}

/// What settling one outcome did to the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    /// Acked, and the declared advance event was sent.
    Advanced,
    /// Acked, and no advance was sent — none declared, or not enabled.
    AckedNoAdvance,
    /// Another path had already settled the effect.
    AlreadySettled,
}

/// The one component that writes.
pub struct Pipeline;

impl Pipeline {
    /// Ack one outcome, then send the declared advance event when the engine
    /// says that event is enabled.
    pub fn settle(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect: &PendingEffect,
        outcome: RunOutcome,
        handler: &HandlerSpec,
    ) -> Result<SettleOutcome, ExecError> {
        let _ = (store, clock, effect, outcome, handler);
        unimplemented!("task 3802")
    }

    /// Send an advance for an effect already acknowledged in a previous life.
    pub fn advance_only(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect_id: &str,
        instance_id: &str,
        advance: &Advance,
    ) -> Result<SettleOutcome, ExecError> {
        let _ = (store, clock, effect_id, instance_id, advance);
        unimplemented!("task 3802")
    }

    /// Poll one due deadline under a derived key.
    pub fn poll(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        instance_id: &str,
        deadline: &str,
        due_ms: i64,
    ) -> Result<Value, ExecError> {
        let _ = (store, clock, instance_id, deadline, due_ms);
        unimplemented!("task 3802")
    }
}
