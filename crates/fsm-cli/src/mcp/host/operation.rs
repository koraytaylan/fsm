//! Owned store-backed protocol operations, with no transport/session borrows.

use fsm_core::json::Value;

use super::Command;
use crate::mcp::tools::elicitation::PreparedElicitation;

pub(in crate::mcp) struct PrepareCommand {
    pub rpc_id: Value,
    pub arguments: Value,
    pub client_elicitation: bool,
    /// Adapter-owned metadata retained throughout the client conversation.
    pub adapter_bytes: usize,
}

pub(super) struct SettleCommand {
    pub rpc_id: Value,
    pub prepared: Option<Box<PreparedElicitation>>,
    pub answer: Value,
}

pub(in crate::mcp) struct ReadCommand {
    pub rpc_id: Value,
    pub operation: ReadOperation,
}

pub(in crate::mcp) enum ReadOperation {
    ResourcesList,
    ResourceRead { uri: String },
    Complete { parameters: Value },
}

pub(super) enum Operation {
    Tool(Command),
    Read(ReadCommand),
    Prepare(PrepareCommand),
    Awaiting { rpc_id: Value },
    Settle(SettleCommand),
}

impl Operation {
    pub(super) fn rpc_id(&self) -> &Value {
        match self {
            Self::Tool(command) => &command.rpc_id,
            Self::Read(command) => &command.rpc_id,
            Self::Prepare(command) => &command.rpc_id,
            Self::Awaiting { rpc_id } => rpc_id,
            Self::Settle(command) => &command.rpc_id,
        }
    }

    pub(super) fn take_rpc_id(&mut self) -> Value {
        let id = match self {
            Self::Tool(command) => &mut command.rpc_id,
            Self::Read(command) => &mut command.rpc_id,
            Self::Prepare(command) => &mut command.rpc_id,
            Self::Awaiting { rpc_id } => rpc_id,
            Self::Settle(command) => &mut command.rpc_id,
        };
        std::mem::replace(id, Value::Null)
    }
}
