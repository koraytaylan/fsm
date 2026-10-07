//! Shared method semantics over a borrowed writer or owned host commands.
//!
//! Only borrowed compatibility helpers receive a Store reference. The staged
//! hosted entry submits typed operations and waits in the session adapter.

use std::{io, path::PathBuf};

mod interaction;

use fsm_core::json::Value;

use crate::mcp::{
    host::{
        AdmissionError, Command, Outcome, Session,
        operation::{ReadCommand, ReadOperation},
    },
    tools::{self, ToolCtx},
};
use crate::{
    clock::Clock,
    store::{ErrorObj, Store},
};

pub(super) enum StoreAccess<'a> {
    Borrowed(Option<&'a mut Store>),
    Hosted {
        session: &'a Session,
        data_dir: &'a std::path::Path,
        output: &'a crate::mcp::notify::Notifier,
    },
}

impl StoreAccess<'_> {
    pub(super) fn data_dir(&self) -> Option<PathBuf> {
        match self {
            Self::Borrowed(store) => store.as_ref().map(|store| store.data_dir.clone()),
            Self::Hosted { data_dir, .. } => Some(data_dir.to_path_buf()),
        }
    }

    pub(super) fn resources_list(&self, id: &Value) -> io::Result<Value> {
        match self {
            Self::Borrowed(store) => Ok(crate::mcp::resources::list(store.as_deref())),
            Self::Hosted {
                session, output, ..
            } => read(session, output, id, ReadOperation::ResourcesList)?
                .map_err(|error| io::Error::other(error.message)),
        }
    }

    pub(super) fn resource_read(
        &self,
        id: &Value,
        uri: &str,
        handlers: Option<&Value>,
    ) -> io::Result<Result<Value, ErrorObj>> {
        match self {
            Self::Borrowed(store) => Ok(crate::mcp::resources::read_with_executor(
                uri,
                store.as_deref(),
                handlers,
            )),
            Self::Hosted {
                session, output, ..
            } => read(
                session,
                output,
                id,
                ReadOperation::ResourceRead { uri: uri.into() },
            ),
        }
    }

    pub(super) fn complete(
        &self,
        id: &Value,
        parameters: Option<&Value>,
    ) -> io::Result<Result<Value, crate::mcp::complete::Invalid>> {
        match self {
            Self::Borrowed(store) => {
                Ok(crate::mcp::complete::complete(parameters, store.as_deref()))
            }
            Self::Hosted {
                session, output, ..
            } => Ok(read(
                session,
                output,
                id,
                ReadOperation::Complete {
                    parameters: parameters.cloned().unwrap_or(Value::Null),
                },
            )?
            .map_err(|error| crate::mcp::complete::Invalid(error.message))),
        }
    }

    pub(super) fn tool(
        &mut self,
        clock: &mut dyn Clock,
        name: &str,
        arguments: Value,
        context: &ToolCtx<'_>,
        degraded_dir: Option<PathBuf>,
    ) -> io::Result<Result<Value, ErrorObj>> {
        match self {
            Self::Borrowed(Some(store)) => Ok(tools::dispatch_with(
                store, clock, name, &arguments, context,
            )),
            Self::Borrowed(None) => Ok(match degraded_dir {
                Some(data_dir) => {
                    tools::dispatch_degraded(&data_dir, clock, name, &arguments, context)
                }
                None => Err(ErrorObj::new("io/read", "no store")),
            }),
            Self::Hosted {
                session, output, ..
            } if name == "instance_elicit" && context.io.is_some() => {
                interaction::call(session, output, clock, arguments, context)
            }
            Self::Hosted {
                session, output, ..
            } => {
                let command = Command {
                    rpc_id: context.request_id.clone().unwrap_or(Value::Null),
                    tool: name.into(),
                    arguments,
                };
                let admission = if let Some(hosted) =
                    crate::mcp::host::operation::HostedToolContext::new(
                        context.meta.as_ref(),
                        &command.rpc_id,
                        output,
                    ) {
                    session.submit_hosted(command, hosted)
                } else {
                    session.submit(command)
                };
                receive(session, output, admission)
            }
        }
    }
}

fn read(
    session: &Session,
    output: &crate::mcp::notify::Notifier,
    id: &Value,
    operation: ReadOperation,
) -> io::Result<Result<Value, ErrorObj>> {
    receive(
        session,
        output,
        session.read(ReadCommand {
            rpc_id: id.clone(),
            operation,
        }),
    )
}

fn receive(
    session: &Session,
    output: &crate::mcp::notify::Notifier,
    admission: Result<std::sync::mpsc::Receiver<Outcome>, AdmissionError>,
) -> io::Result<Result<Value, ErrorObj>> {
    Ok(wait(session, output, admission)?.result)
}

fn admission_error(error: AdmissionError) -> io::Error {
    match error {
        AdmissionError::Busy => io::Error::new(io::ErrorKind::WouldBlock, AdmissionBusy),
        AdmissionError::Closed | AdmissionError::Stopped => io::Error::new(
            io::ErrorKind::BrokenPipe,
            format!("host command admission refused: {error:?}"),
        ),
        AdmissionError::GenerationExhausted => {
            io::Error::other("host command generation exhausted")
        }
    }
}

fn wait<T>(
    session: &Session,
    output: &crate::mcp::notify::Notifier,
    admission: Result<std::sync::mpsc::Receiver<T>, AdmissionError>,
) -> io::Result<T> {
    let receiver = admission.map_err(admission_error)?;
    loop {
        check_wait(session, output)?;
        match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(outcome) => {
                check_wait(session, output)?;
                return Ok(outcome);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(io::ErrorKind::Interrupted, Retired));
            }
        }
    }
}

fn check_wait(session: &Session, output: &crate::mcp::notify::Notifier) -> io::Result<()> {
    if output.is_broken() {
        session.close();
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "hosted protocol output failed while waiting",
        ));
    }
    if session.is_retired() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, Retired));
    }
    Ok(())
}

#[derive(Debug)]
struct AdmissionBusy;
impl std::fmt::Display for AdmissionBusy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("host command admission is full")
    }
}
impl std::error::Error for AdmissionBusy {}

#[derive(Debug)]
struct Retired;
impl std::fmt::Display for Retired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("original host request retired without a reply")
    }
}
impl std::error::Error for Retired {}

pub(super) fn is_busy(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|cause| cause.is::<AdmissionBusy>())
}

pub(super) fn is_retired(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|cause| cause.is::<Retired>())
}
