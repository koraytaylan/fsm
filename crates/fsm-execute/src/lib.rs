//! Effect executor for the `fsm` statechart engine.
//!
//! `fsm-core` decides and `fsm-store` journals; neither runs anything. A
//! transition emits a named effect into the instance's `effects_pending`
//! outbox and stops there, so until now a workflow only advanced while a
//! human or a model was online to run the work, acknowledge it, and send the
//! domain event the guards wait for. This crate is the missing half: it
//! watches a store, runs an operator-configured table of handlers as
//! subprocesses, acknowledges each outcome into the journal, and polls due
//! deadlines — unattended.
//!
//! # What it never does
//!
//! **Effects never drive transitions, and the executor never improvises.**
//! Every action it takes is either running a handler for an effect the
//! machine emitted, or sending a domain event the machine's own definition
//! already declares and the operator's handler table names. An effect with no
//! handler is a deliberate stall, not a guess.
//!
//! # What it guarantees
//!
//! **At-least-once execution, exactly-once journaling.** The executor holds no
//! state the journal cannot reconstruct: what still needs running is
//! `effects_pending`, and what has already been written is the store's own
//! claimed-`request_id` map. Every `request_id` it issues is derived from that
//! journaled content, so a restarted executor re-issues the identical key and
//! the store replays it instead of applying it twice. What a restart *can*
//! repeat is a handler process that was in flight when the executor died: the
//! child is orphaned, the effect is still pending, and the next executor
//! starts a fresh run. A handler whose work already reached the outside world
//! is not rolled back by `fsm` — model the undo as a compensating effect the
//! machine's failure path emits.
//!
//! # What it can reach
//!
//! A handler is either a **process**, whose exit status is the answer, or an
//! **MCP server**, which the executor talks to over its stdio and calls one
//! tool on. The second kind does not widen what the operator has authorised:
//! `argv[0]` is still a literal rooted path from their own table, the tool
//! name is fixed, and the arguments are a template they wrote. It does widen
//! what a workflow can *do* — an effect can now call another MCP server's
//! tool, which makes this engine an orchestrator of the ecosystem it belongs
//! to rather than only a member of it.
//!
//! The executor is **single-node**: it inherits the store's single-writer
//! ceiling, takes the writer only for the ticks that write, and observes
//! through `Store::open_read_only`, which takes no lock and coexists with a
//! concurrent MCP server. See `docs/EMBEDDING.md` for the run modes and the
//! handler-table format.

#![forbid(unsafe_code)]
#![allow(clippy::result_large_err, clippy::collapsible_if)]

pub mod config;
pub mod dead;
pub mod effect;
pub mod error;
pub mod mcp_client;
pub mod rid;
pub mod run;
pub mod sched;
pub mod service;
pub mod watch;
