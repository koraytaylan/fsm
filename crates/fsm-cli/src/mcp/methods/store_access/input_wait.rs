//! Owned input continues lifetime controls while an owner response is pending.

use crate::mcp::{
    host::{AdmissionError, Session},
    jsonrpc::{
        INVALID_REQUEST, Incoming, PARSE_ERROR, WireError, error_response, parse_line,
        result_response,
    },
    notify::{Notifier, SessionIo},
};
use fsm_core::json::Value;
use std::{cell::RefCell, io, sync::mpsc};

pub(super) fn wait<T>(
    session: &Session,
    output: &Notifier,
    admission: Result<mpsc::Receiver<T>, AdmissionError>,
    input: &RefCell<SessionIo<'_>>,
) -> io::Result<T> {
    let receiver = admission.map_err(super::admission_error)?;
    loop {
        super::check_wait(session, output)?;
        match receiver.try_recv() {
            Ok(result) => {
                super::check_wait(session, output)?;
                return Ok(result);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, super::Retired));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let line = match input.borrow_mut().read_line_interruptible() {
            Ok(Some(line)) => line,
            Ok(None) => {
                session.close();
                return Err(io::Error::new(io::ErrorKind::Interrupted, super::Retired));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                output.send(&error_response(Value::Null, PARSE_ERROR, "parse error"))?;
                continue;
            }
            Err(error) => {
                session.close();
                return Err(error);
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(&line) {
            Ok(Incoming::Notification { method, params }) => {
                if method == "notifications/cancelled"
                    && let Some(requested) =
                        params.as_ref().and_then(|params| params.get("requestId"))
                {
                    session.cancel(requested);
                    input.borrow_mut().cancel_deferred(requested);
                }
            }
            Ok(Incoming::Request { id, method, .. }) => {
                if method == "ping" {
                    output.send(&result_response(id, Value::Obj(Default::default())))?;
                } else if !input.borrow_mut().defer(line) {
                    output.send(&error_response(id, -32004, "Server busy"))?;
                }
            }
            Ok(Incoming::Response { .. }) => {}
            Err(WireError::Parse(_)) => {
                output.send(&error_response(Value::Null, PARSE_ERROR, "parse error"))?
            }
            Err(_) => output.send(&error_response(
                Value::Null,
                INVALID_REQUEST,
                "invalid request",
            ))?,
        }
    }
}
