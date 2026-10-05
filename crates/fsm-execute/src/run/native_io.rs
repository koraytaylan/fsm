//! Linux owned stream adapters; construction does not authorize native launch.

use super::{BoundedBytes, ExecError, mcp_worker::McpWorker, stream::StreamCapture};
use crate::mcp_client::McpOutcome;
use fsm_core::json::Value;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Stdio;

/// Shared bounded capture for an independently enrolled native child.
pub struct NativeCapture(StreamCapture);

impl NativeCapture {
    /// Create owned socket endpoints without a capture file or launch.
    pub fn open() -> Result<(Self, Stdio), ExecError> {
        StreamCapture::open(Path::new("unused"), "native-gate")
            .map(|(capture, writer)| (Self(capture), writer))
    }

    /// Drain within the public runner's fixed per-poll work budget.
    pub fn poll(&mut self) {
        self.0.poll();
    }

    /// Finish with truthful EOF, prefix and digest accounting.
    pub fn finish(self) -> BoundedBytes {
        self.0.finish()
    }
}

/// Owned MCP exchange over supplied enrolled streams, with cancel and join.
pub struct NativeProtocol(McpWorker);

impl NativeProtocol {
    /// Adopt streams; the caller remains responsible for claim and containment.
    pub fn from_streams(
        stdin: UnixStream,
        stdout: UnixStream,
        tool: String,
        arguments: Value,
    ) -> std::io::Result<Self> {
        McpWorker::from_streams(stdin, stdout, tool, arguments).map(Self)
    }

    /// Independently shut down both protocol directions.
    pub fn cancel(&self) {
        self.0.cancel();
    }

    /// Join only a finished worker; false retains unresolved handle ownership.
    pub fn reap(&mut self) -> bool {
        self.0.reap()
    }

    /// Return a candidate answer only after observing the worker join.
    pub fn collect(&mut self) -> Option<McpOutcome> {
        self.0.collect()
    }
}
