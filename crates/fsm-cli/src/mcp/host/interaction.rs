//! One original admitted request retained across a client-owned conversation.
//!
//! The continuation carries its reservation and flag, never a Store borrow or
//! compiled machine; settlement returns to the same owner under the same slot.

use std::sync::{Arc, mpsc};

use fsm_core::json::Value;

use crate::{
    mcp::{
        cancel::CancelFlag,
        tools::elicitation::{self, PreparedElicitation},
    },
    store::{ErrorObj, Store},
};

use super::{
    AdmissionError, Outcome, Session,
    mailbox::{Admitted, Reply, value_charge},
    operation::{Operation, PrepareCommand, SettleCommand},
};

#[derive(Debug)]
pub(in crate::mcp) enum PrepareFailure {
    Tool(ErrorObj),
    Admission(AdmissionError),
}

pub(in crate::mcp) struct Continuation {
    prepared: Option<Box<PreparedElicitation>>,
    // Drop request last, so prepared allocations retire before their charge.
    request: Admitted,
}

impl Continuation {
    pub(in crate::mcp) fn request_id(&self) -> &str {
        self.prepared
            .as_ref()
            .expect("original prepared request")
            .request_id()
    }
    pub(in crate::mcp) fn take_params(&mut self) -> Value {
        self.prepared
            .as_mut()
            .expect("prepared continuation")
            .take_params()
    }

    pub(in crate::mcp) fn cancellation(&self) -> CancelFlag {
        self.request.cancel.clone()
    }

    #[cfg(test)]
    pub(super) fn charged_bytes(&self) -> usize {
        self.request.reservation.bytes()
    }
}

impl Session {
    pub(in crate::mcp) fn prepare(
        &self,
        command: PrepareCommand,
    ) -> Result<mpsc::Receiver<Result<Continuation, PrepareFailure>>, AdmissionError> {
        self.mailbox
            .prepare(Arc::clone(&self.original), Operation::Prepare(command))
    }

    pub(in crate::mcp) fn resume(
        &self,
        mut continuation: Continuation,
        answer: Value,
    ) -> Result<mpsc::Receiver<Outcome>, AdmissionError> {
        if !Arc::ptr_eq(&self.original, &continuation.request.session) {
            return Err(AdmissionError::Closed);
        }
        if self.is_retired() {
            return Err(AdmissionError::Closed);
        }
        continuation
            .request
            .reservation
            .grow(value_charge(&answer, 0))?;
        let (reply, receiver) = mpsc::sync_channel(1);
        continuation.request.reply = Reply::Outcome(reply);
        continuation.request.command = Operation::Settle(SettleCommand {
            rpc_id: continuation.request.command.take_rpc_id(),
            prepared: continuation.prepared.take(),
            answer,
        });
        self.mailbox.resume(continuation.request)?;
        Ok(receiver)
    }
}

/// This turn reads and derives the question; it never talks to the client.
pub(super) fn prepare(store: &Store, mut request: Admitted) {
    let Operation::Prepare(command) = &request.command else {
        unreachable!("preparation envelope")
    };
    let prepared = prepare_checked(store, command);
    let reply = std::mem::replace(&mut request.reply, Reply::Retired);
    let Reply::Prepared(reply) = reply else {
        unreachable!("preparation reply")
    };
    let mut prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            if request.session.is_open() && !request.cancel.cancelled() {
                let _ = reply.try_send(Err(PrepareFailure::Tool(error)));
            }
            return;
        }
    };
    prepared.guard_sequence(store);
    let extra = std::mem::size_of::<PreparedElicitation>()
        .saturating_add(prepared.string_capacity())
        .saturating_add(value_charge(prepared.params(), 0));
    if let Err(error) = request.reservation.grow(extra) {
        if request.session.is_open() && !request.cancel.cancelled() {
            let _ = reply.try_send(Err(PrepareFailure::Admission(error)));
        }
        return;
    }
    if !request.session.is_open() || request.cancel.cancelled() {
        return;
    }
    // Keep only the original RPC identity while the client waits; arguments
    // have served preparation and need not duplicate the question in memory.
    request.command = Operation::Awaiting {
        rpc_id: request.command.take_rpc_id(),
    };
    let _ = reply.try_send(Ok(Continuation {
        prepared: Some(Box::new(prepared)),
        request,
    }));
}

fn prepare_checked(
    store: &Store,
    command: &PrepareCommand,
) -> Result<PreparedElicitation, ErrorObj> {
    let arguments = &command.arguments;
    let result = (|| {
        if let Some(error) =
            crate::mcp::tools::dispatch::read_only_refusal(store, "instance_elicit", arguments)
        {
            return Err(error);
        }
        let definition = crate::mcp::tools::registry()
            .into_iter()
            .find(|tool| tool.name == "instance_elicit")
            .expect("registered elicitation tool");
        crate::mcp::tools::validate_args(&(definition.input_schema)(), arguments)?;
        if !command.client_elicitation {
            return Err(ErrorObj::new(
                "req/elicit_unsupported",
                "this client did not advertise the elicitation capability",
            )
            .hint("send the event directly with instance_send"));
        }
        elicitation::prepare_elicitation(store, arguments)
    })();
    result.map_err(|error| crate::mcp::tools::dispatch::attach_request_id(error, arguments))
}

/// Charge adapter-retained metadata and RPC copies as part of this request.
pub(in crate::mcp) fn adapter_charge(metadata: Option<&Value>, rpc_id: &Value) -> usize {
    metadata
        .map_or(0, |value| value_charge(value, 0))
        .saturating_add(value_charge(rpc_id, 0).saturating_mul(3))
}
