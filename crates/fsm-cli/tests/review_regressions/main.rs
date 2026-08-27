//! Targeted regressions for store identity, replay, and CLI contracts.

// Several rows hand the store's own `ErrorObj` back through a `Result`, which
// is how the code under test reports a failure. Boxing it here would only make
// every assertion dereference to read a code.
#![allow(clippy::result_large_err)]

mod harness;

mod cli_commands_and_journal;
mod cli_mcp_parity;
mod output_schema_and_wire_format;
mod replay_and_migration;
mod snapshot_divergence;
mod spec_and_expr_semantics;
mod state_persistence;
