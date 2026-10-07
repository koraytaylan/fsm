//! Owned store-backed protocol operations, with no transport/session borrows.

use fsm_core::json::Value;

use super::Command;

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
}

impl Operation {
    pub(super) fn rpc_id(&self) -> &Value {
        match self {
            Self::Tool(command) => &command.rpc_id,
            Self::Read(command) => &command.rpc_id,
        }
    }

    pub(super) fn take_rpc_id(&mut self) -> Value {
        let id = match self {
            Self::Tool(command) => &mut command.rpc_id,
            Self::Read(command) => &mut command.rpc_id,
        };
        std::mem::replace(id, Value::Null)
    }
}
