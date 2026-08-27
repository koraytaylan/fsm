//! MCP stdio transport: JSON-RPC framing and the serve loop.

// Plan 0012's modules are declared together, so each task that fills one
// stays inside its own file: a module cannot be declared without its file,
// so the shells and the declarations land together or neither compiles.
pub mod cancel;
// Plan 0013's two modules land together for the same reason.
pub mod complete;
pub mod descriptions;
pub mod elicit;
pub mod jsonrpc;
pub mod logging;
pub mod notify;
pub mod progress;
pub mod prompts;
pub mod resources;
pub mod serve;
pub mod subscribe;
pub mod tools;
pub mod watch;
