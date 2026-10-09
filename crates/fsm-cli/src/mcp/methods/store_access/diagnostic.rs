//! Read-only diagnostic work retains original admission, never the writer owner.

use crate::mcp::{
    host::{Command, Session, operation::HostedToolContext},
    notify::Notifier,
    tools::ToolCtx,
};
use crate::{clock::Clock, store::ErrorObj};
use fsm_core::json::Value;
use std::{io, sync::mpsc};

enum Message {
    Time(mpsc::SyncSender<i64>),
    Complete(Result<Value, ErrorObj>),
}

struct Retirement<'a> {
    session: &'a Session,
    rpc_id: &'a Value,
    armed: bool,
}
impl Drop for Retirement<'_> {
    fn drop(&mut self) {
        // Adapter unwinding must cancel before its clock receiver disappears;
        // the worker retains the original reservation until it actually exits.
        if self.armed {
            self.session.cancel(self.rpc_id);
        }
    }
}

struct AdapterClock {
    messages: mpsc::SyncSender<Message>,
    last: i64,
}
impl Clock for AdapterClock {
    fn now_ms(&mut self) -> i64 {
        let (reply, receiver) = mpsc::sync_channel(1);
        if self.messages.send(Message::Time(reply)).is_ok() {
            if let Ok(now) = receiver.recv() {
                self.last = now;
            }
        }
        self.last
    }
}

pub(super) fn call(
    session: &Session,
    output: &Notifier,
    data_dir: &std::path::Path,
    clock: &mut dyn Clock,
    name: &str,
    arguments: Value,
    context: &ToolCtx<'_>,
) -> io::Result<Result<Value, ErrorObj>> {
    let rpc_id = context.request_id.clone().unwrap_or(Value::Null);
    let Some(hosted) = HostedToolContext::new(context.meta.as_ref(), &rpc_id, output) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "diagnostic requires hosted output",
        ));
    };
    let request = session
        .reserve_diagnostic(
            Command {
                rpc_id: rpc_id.clone(),
                tool: name.into(),
                arguments,
            },
            hosted,
        )
        .map_err(super::admission_error)?;
    let data_dir = data_dir.to_path_buf();
    let (messages, receiver) = mpsc::sync_channel(1);
    let worker = std::thread::Builder::new()
        .name("fsm-session-diagnostic".into())
        .spawn(move || {
            let mut clock = AdapterClock {
                messages: messages.clone(),
                last: 0,
            };
            let result = request.diagnostic(&data_dir, &mut clock);
            let _ = messages.send(Message::Complete(result));
            drop(request);
        })?;
    let mut retirement = Retirement {
        session,
        rpc_id: &rpc_id,
        armed: true,
    };
    let result = (|| loop {
        let message =
            if let Some(input) = context.io.filter(|input| input.borrow().has_owned_wait()) {
                super::input_wait::wait_receiver(session, output, &receiver, input)?
            } else {
                super::wait_receiver(session, output, &receiver)?
            };
        match message {
            Message::Time(reply) => {
                // This guard drops before `reply` during unwinding, so the
                // worker observes cancellation before the rendezvous releases.
                let mut clock_retirement = Retirement {
                    session,
                    rpc_id: &rpc_id,
                    armed: true,
                };
                let now = clock.now_ms();
                clock_retirement.armed = false;
                let _ = reply.send(now);
            }
            Message::Complete(result) => break Ok(result),
        }
    })();
    retirement.armed = result.is_err();
    drop(retirement);
    if result.is_err() {
        // Dropping the bounded receiver releases any clock rendezvous; coarse
        // loops observe the original cancellation, and retain admission meanwhile.
        drop(receiver);
    } else {
        worker
            .join()
            .map_err(|_| io::Error::other("diagnostic worker failed"))?;
    }
    result
}
