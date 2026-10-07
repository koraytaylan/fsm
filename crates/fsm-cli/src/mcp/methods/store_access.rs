//! Shared method semantics over a borrowed writer or owned host commands.
//!
//! Only borrowed compatibility helpers receive a Store reference. The staged
//! hosted entry submits typed operations and waits in the session adapter.

use std::{io, path::PathBuf};

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
            Self::Hosted { session, .. } => read(session, id, ReadOperation::ResourcesList)?
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
            Self::Hosted { session, .. } => {
                read(session, id, ReadOperation::ResourceRead { uri: uri.into() })
            }
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
            Self::Hosted { session, .. } => Ok(read(
                session,
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
            Self::Hosted { session, .. } => receive(session.submit(Command {
                rpc_id: context.request_id.clone().unwrap_or(Value::Null),
                tool: name.into(),
                arguments,
            })),
        }
    }
}

fn read(
    session: &Session,
    id: &Value,
    operation: ReadOperation,
) -> io::Result<Result<Value, ErrorObj>> {
    receive(session.read(ReadCommand {
        rpc_id: id.clone(),
        operation,
    }))
}

fn receive(
    admission: Result<std::sync::mpsc::Receiver<Outcome>, AdmissionError>,
) -> io::Result<Result<Value, ErrorObj>> {
    let receiver = admission.map_err(|error| match error {
        AdmissionError::Busy => io::Error::new(io::ErrorKind::WouldBlock, AdmissionBusy),
        AdmissionError::Closed | AdmissionError::Stopped => io::Error::new(
            io::ErrorKind::BrokenPipe,
            format!("host command admission refused: {error:?}"),
        ),
        AdmissionError::GenerationExhausted => {
            io::Error::other("host command generation exhausted")
        }
    })?;
    receiver
        .recv()
        .map(|outcome| outcome.result)
        .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, Retired))
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
