//! Private single-writer command owner, staged before transport adapters.
//!
//! Admission holds no Store borrow. The owner captures each result and journal
//! prefix before taking another command; stop and session close bypass the
//! application queue. Executor scheduling and bounded session egress are later
//! integration steps, so this module does not advertise autonomous execution.

// Transport construction lands in tasks 9001/9002; the private harness already
// drives these exact constructors and envelopes rather than a test-only owner.
#![allow(dead_code)]

use std::sync::{Arc, mpsc};

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};
use fsm_core::json::Value;

mod mailbox;
#[cfg(target_os = "linux")]
mod native;
use mailbox::{Admitted, Mailbox};

#[cfg(test)]
mod tests;

/// A request's immutable result and the prefix captured in its owner turn.
pub(super) struct Outcome {
    pub session_generation: u64,
    pub rpc_id: Value,
    pub result: Result<Value, ErrorObj>,
    pub committed_seq: u64,
    pub publication: Option<Publication>,
}

/// The complete appended interval, absent for reads or idempotent replay.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Publication {
    pub first_seq: u64,
    pub last_seq: u64,
}

/// Owned application data; transports retain no copy of its arguments.
pub(super) struct Command {
    pub rpc_id: Value,
    pub tool: String,
    pub arguments: Value,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum AdmissionError {
    Busy,
    Stopped,
    Closed,
    GenerationExhausted,
}

/// One original session incarnation; a replacement obtains another generation.
#[derive(Clone)]
pub(super) struct Session {
    mailbox: Arc<Mailbox>,
    original: Arc<mailbox::SessionState>,
}

impl Session {
    pub(super) fn submit(
        &self,
        command: Command,
    ) -> Result<mpsc::Receiver<Outcome>, AdmissionError> {
        self.mailbox.admit(Arc::clone(&self.original), command)
    }

    /// Close controls remain available when application admission is saturated.
    pub(super) fn close(&self) {
        self.original.close();
        self.mailbox.cancel_generation(self.original.generation);
        self.mailbox.wake();
    }

    /// Cancel only admitted requests of this original session incarnation.
    pub(super) fn cancel(&self, rpc_id: &Value) -> usize {
        self.mailbox.cancel(self.original.generation, rpc_id)
    }
}

#[derive(Clone)]
pub(super) struct Handle {
    mailbox: Arc<Mailbox>,
    #[cfg(target_os = "linux")]
    native_stop: Option<(fsm_execute::service::ExecutorControl, i64)>,
}

impl Handle {
    pub(super) fn session(&self) -> Result<Session, AdmissionError> {
        let original = self.mailbox.session()?;
        Ok(Session {
            mailbox: Arc::clone(&self.mailbox),
            original,
        })
    }

    /// A single coalesced stop bit has capacity independent of queued commands.
    pub(super) fn stop(&self) {
        #[cfg(target_os = "linux")]
        if let Some((control, timeout_ms)) = &self.native_stop {
            let _ = control.stop(fsm_execute::service::ShutdownMode::Abort, *timeout_ms);
        }
        self.mailbox.stop();
    }
}

/// Store and logical clock never leave this owner or enter its shared mailbox.
pub(super) struct Owner<C> {
    store: Store,
    clock: C,
    mailbox: Arc<Mailbox>,
}

impl<C: Clock> Owner<C> {
    pub(super) fn new(store: Store, clock: C) -> (Self, Handle) {
        let mailbox = Arc::new(Mailbox::default());
        let handle = Handle {
            mailbox: Arc::clone(&mailbox),
            #[cfg(target_os = "linux")]
            native_stop: None,
        };
        (
            Self {
                store,
                clock,
                mailbox,
            },
            handle,
        )
    }

    /// Run complete commands until stop; rejected queued work performs no I/O.
    pub(super) fn run(mut self) {
        while let Some(admitted) = self.mailbox.next() {
            self.apply(admitted);
        }
    }

    fn apply(&mut self, admitted: Admitted) {
        apply_command(&mut self.store, &mut self.clock, admitted);
    }
}

// Both writer-only and native owners use exactly this committing boundary.
fn apply_command(store: &mut Store, clock: &mut dyn Clock, mut admitted: Admitted) {
    // The charged admitted envelope remains alive through dispatch. No
    // transport output or client input occurs in this operation boundary.
    if !admitted.session.is_open() || admitted.cancel.cancelled() {
        return;
    }
    let before = store.journal.last_seq;
    let context = super::tools::ToolCtx {
        cancel: std::mem::take(&mut admitted.cancel),
        ..Default::default()
    };
    let result = super::tools::dispatch_with(
        store,
        clock,
        &admitted.command.tool,
        &admitted.command.arguments,
        &context,
    );
    let committed_seq = store.journal.last_seq;
    let publication = (committed_seq > before).then_some(Publication {
        first_seq: before.saturating_add(1),
        last_seq: committed_seq,
    });
    if admitted.session.is_open() {
        let outcome = Outcome {
            session_generation: admitted.session.generation,
            rpc_id: std::mem::replace(&mut admitted.command.rpc_id, Value::Null),
            result,
            committed_seq,
            publication,
        };
        // This channel has exactly one reserved slot. A lost receiver
        // cannot block the owner or reverse a committed operation.
        let _ = admitted.reply.try_send(outcome);
    }
}

impl<C> Drop for Owner<C> {
    fn drop(&mut self) {
        self.mailbox.stop();
    }
}
