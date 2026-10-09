//! Private single-writer command owner shared by transport adapters.
//!
//! Admission holds no Store borrow. The owner captures each result and journal
//! prefix before taking another command; stop and session close bypass the
//! application queue. On Linux, production stdio selects the native owner,
//! which drives quiet decision passes and retains the writer through shutdown;
//! HTTP integration and complete transport acceptance remain separate tasks.

// The private ownership harness drives production constructors and envelopes;
// tasks 9001/9002 retain their complete transport acceptance inventories.
#![allow(dead_code)]

use std::sync::{Arc, mpsc};

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};
use fsm_core::json::Value;

pub(super) mod interaction;
mod mailbox;
pub(super) mod operation;
use operation::{Operation, ReadCommand, ReadOperation};
#[cfg(target_os = "linux")]
pub(super) mod native;
use mailbox::{Admitted, Mailbox, Reply};

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
    #[cfg(target_os = "linux")]
    native_control: Option<fsm_execute::service::ExecutorControl>,
}

impl Session {
    #[cfg(test)]
    pub(in crate::mcp) fn pause_response(&self) {
        let hold = self.original.response_hold.lock().unwrap().take();
        if let Some((entered, release)) = hold {
            entered.send(()).unwrap();
            release
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("response acceptance barrier release missing");
        }
    }
    pub(in crate::mcp) fn committed_prefix(
        &self,
    ) -> Option<fsm_store::journal_io::CommittedPrefix> {
        self.mailbox.committed.clone()
    }
    pub(in crate::mcp) fn reserve_diagnostic(
        &self,
        command: Command,
        context: operation::HostedToolContext,
    ) -> Result<mailbox::Admitted, AdmissionError> {
        self.mailbox.reserve_diagnostic(
            Arc::clone(&self.original),
            Operation::HostedTool {
                command,
                context: Box::new(context),
            },
        )
    }

    pub(in crate::mcp) fn submit_hosted(
        &self,
        command: Command,
        context: operation::HostedToolContext,
    ) -> Result<mpsc::Receiver<Outcome>, AdmissionError> {
        self.mailbox.admit(
            Arc::clone(&self.original),
            Operation::HostedTool {
                command,
                context: Box::new(context),
            },
        )
    }

    pub(super) fn submit(
        &self,
        command: Command,
    ) -> Result<mpsc::Receiver<Outcome>, AdmissionError> {
        self.mailbox
            .admit(Arc::clone(&self.original), Operation::Tool(command))
    }

    /// Read operations share admission order and retained-allocation budgets.
    pub(super) fn read(
        &self,
        command: ReadCommand,
    ) -> Result<mpsc::Receiver<Outcome>, AdmissionError> {
        self.mailbox
            .admit(Arc::clone(&self.original), Operation::Read(command))
    }

    /// An adapter may retire its wait without waiting for the writer turn.
    pub(super) fn is_retired(&self) -> bool {
        if !self.original.is_open() {
            return true;
        }
        if self.mailbox.is_stopped() {
            self.close();
            return true;
        }
        #[cfg(target_os = "linux")]
        if self.native_control.as_ref().is_some_and(|control| {
            control.report().phase != fsm_execute::service::ExecutorPhase::Running
        }) {
            self.close();
            return true;
        }
        false
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
            #[cfg(target_os = "linux")]
            native_control: self
                .native_stop
                .as_ref()
                .map(|(control, _)| control.clone()),
        })
    }

    /// A single coalesced stop bit has capacity independent of queued commands.
    pub(super) fn stop(&self) {
        #[cfg(target_os = "linux")]
        if let Some((control, timeout_ms)) = &self.native_stop {
            let _ = control.stop(fsm_execute::service::ShutdownMode::Abort, *timeout_ms);
        }
        self.reject_queued();
    }

    /// An adapter already holding the original lifecycle stop need not escalate it.
    pub(super) fn reject_queued(&self) {
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
        let mailbox = Arc::new(Mailbox::with_committed_prefix(
            store.journal.committed_prefix(),
        ));
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
        apply_command(&mut self.store, &mut self.clock, admitted, None);
    }
}

// Both writer-only and native owners use exactly this committing boundary.
fn apply_command(
    store: &mut Store,
    clock: &mut dyn Clock,
    mut admitted: Admitted,
    handlers: Option<&Value>,
) {
    // The charged admitted envelope remains alive through dispatch. No
    // transport output or client input occurs in this operation boundary.
    if !admitted.session.is_open() {
        return;
    }
    if admitted.cancel.cancelled() && !matches!(&admitted.command, Operation::Settle(_)) {
        return;
    }
    if matches!(&admitted.command, Operation::Prepare(_)) {
        interaction::prepare(store, admitted);
        return;
    }
    let before = store.journal.last_seq;
    let context = super::tools::ToolCtx {
        cancel: std::mem::take(&mut admitted.cancel),
        ..Default::default()
    };
    let result = match &mut admitted.command {
        Operation::Tool(command) => {
            super::tools::dispatch_with(store, clock, &command.tool, &command.arguments, &context)
        }
        Operation::HostedTool {
            command,
            context: hosted,
        } => {
            let context = super::tools::ToolCtx {
                notifier: Some(&hosted.notifier),
                meta: hosted.metadata.take(),
                cancel: context.cancel.clone(),
                ..Default::default()
            };
            super::tools::dispatch_with(store, clock, &command.tool, &command.arguments, &context)
        }
        Operation::Read(command) => match &command.operation {
            ReadOperation::ResourcesList => Ok(super::resources::list(Some(store))),
            ReadOperation::ResourceRead { uri } => super::resources::read_with_executor_mode(
                uri,
                Some(store),
                handlers,
                handlers.is_some(),
            ),
            ReadOperation::Complete { parameters } => {
                super::complete::complete(Some(parameters), Some(store))
                    .map_err(|error| ErrorObj::new("req/args_invalid", error.0))
            }
        },
        Operation::Settle(command) => {
            let request_key = command
                .prepared
                .as_ref()
                .expect("original prepared request")
                .request_id()
                .to_owned();
            let result = if context.cancel.cancelled() {
                Err(super::cancel::CancelFlag::refusal())
            } else {
                super::tools::elicitation::settle_elicitation(
                    store,
                    clock,
                    *command.prepared.take().expect("original prepared request"),
                    std::mem::replace(&mut command.answer, Value::Null),
                )
            };
            result.map_err(|error| error.request_id(&request_key))
        }
        Operation::Prepare(_) => unreachable!("preparation has its own result"),
        Operation::Awaiting { .. } => unreachable!("client-owned continuation is not queued"),
    };
    let committed_seq = store.journal.last_seq;
    let publication = (committed_seq > before).then_some(Publication {
        first_seq: before.saturating_add(1),
        last_seq: committed_seq,
    });
    if admitted.session.is_open() {
        let outcome = Outcome {
            session_generation: admitted.session.generation,
            rpc_id: admitted.command.take_rpc_id(),
            result,
            committed_seq,
            publication,
        };
        // This channel has exactly one reserved slot. A lost receiver
        // cannot block the owner or reverse a committed operation.
        let Reply::Outcome(reply) = &admitted.reply else {
            unreachable!("operation reply")
        };
        let _ = reply.try_send(outcome);
    }
}

impl<C> Drop for Owner<C> {
    fn drop(&mut self) {
        self.mailbox.stop();
    }
}

impl mailbox::Admitted {
    pub(in crate::mcp) fn diagnostic(
        &self,
        data_dir: &std::path::Path,
        clock: &mut dyn Clock,
    ) -> Result<Value, ErrorObj> {
        #[cfg(test)]
        {
            let hold = self.session.diagnostic_hold.lock().unwrap().take();
            if let Some((entered, release)) = hold {
                entered.send(self.cancel.clone()).unwrap();
                release
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("diagnostic acceptance barrier release missing");
            }
        }
        if !self.session.is_open() || self.cancel.cancelled() {
            return Err(super::cancel::CancelFlag::refusal());
        }
        let mut store = Store::open_read_only(data_dir)?;
        let Operation::HostedTool { command, context } = &self.command else {
            unreachable!("original diagnostic request")
        };
        let tool_context = super::tools::ToolCtx {
            notifier: Some(&context.notifier),
            meta: context.metadata.clone(),
            cancel: self.cancel.clone(),
            ..Default::default()
        };
        super::tools::dispatch_with(
            &mut store,
            clock,
            &command.tool,
            &command.arguments,
            &tool_context,
        )
    }
}
