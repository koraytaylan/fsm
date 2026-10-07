//! Client-owned elicitation between two writer-owner turns.

use std::io;

use fsm_core::json::Value;

use crate::{
    clock::Clock,
    mcp::{
        host::{
            Session,
            interaction::{PrepareFailure, adapter_charge},
            operation::PrepareCommand,
        },
        notify::Notifier,
        tools::ToolCtx,
    },
    store::ErrorObj,
};

pub(super) fn call(
    session: &Session,
    output: &Notifier,
    clock: &mut dyn Clock,
    arguments: Value,
    context: &ToolCtx<'_>,
) -> io::Result<Result<Value, ErrorObj>> {
    let rpc_id = context.request_id.clone().unwrap_or(Value::Null);
    let adapter_bytes = adapter_charge(context.meta.as_ref(), &rpc_id);
    let prepared = session.prepare(PrepareCommand {
        rpc_id,
        arguments,
        client_elicitation: context.client_elicitation,
        adapter_bytes,
    });
    let mut continuation = match super::wait_input(session, output, prepared, context.io)? {
        Ok(continuation) => continuation,
        Err(PrepareFailure::Tool(error)) => return Ok(Err(error)),
        Err(PrepareFailure::Admission(error)) => return Err(super::admission_error(error)),
    };
    let params = continuation.take_params();
    let cancel = continuation.cancellation();
    let answer = crate::mcp::elicit::ask_hosted(
        context.io.expect("selected client conversation"),
        params,
        clock,
        session,
        &cancel,
    );
    super::check_wait(session, output)?;
    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => return Ok(Err(error.request_id(continuation.request_id()))),
    };
    context
        .io
        .expect("selected client conversation")
        .borrow_mut()
        .hold_publication();
    super::receive_input(
        session,
        output,
        session.resume(continuation, answer),
        context.io,
    )
}
