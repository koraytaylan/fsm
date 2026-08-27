//! The second transport: Streamable HTTP, hand-rolled over `std::net`.
//!
//! Every module this plan adds is declared here, and each lands as a shell
//! first, because a module cannot be declared without its file — the same
//! move plan 0008's crate scaffold made, and for the same reason: each later
//! task then stays inside one file.
//!
//! The posture is the workspace's own. Blocking threads, no async runtime,
//! no dependencies, and every bound a stranger can reach stated as a
//! constant. There is no TLS here and there will not be one: the server
//! binds loopback by default and anything else is an operator's decision
//! made behind a proxy that terminates TLS.
//!
//! Plan 0015.

pub mod endpoint;
pub mod request;
pub mod response;
pub mod security;
pub mod server;
pub mod session;
pub mod sse;
pub mod writer;
