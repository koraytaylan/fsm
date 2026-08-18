//! Durable single-writer store for the `fsm` statechart engine.
//!
//! `fsm-core` is the pure engine: it folds records into state and steps
//! instances without touching the filesystem or a clock. This crate is the
//! shell around it — an append-only hash-chained journal, the machine and
//! instance store folded from that journal, disposable snapshots, and the one
//! wall-clock read in the system.
//!
//! Embedders that keep their own persistence do not need this crate; they use
//! `fsm-core` directly. See `docs/EMBEDDING.md` for both loops.
//!
//! # Concurrency contract
//!
//! Every operation on [`store::Store`] is **synchronous and blocking**, and a
//! store is a **single-writer** resource guarded by a process-wide advisory
//! lock. Callers on an async runtime must own a `Store` from one dedicated
//! blocking thread (a writer actor) rather than sharing it across tasks. See
//! `docs/EMBEDDING.md` for measured append latency and the actor pattern.

#![forbid(unsafe_code)]
#![allow(
    clippy::result_large_err,
    clippy::collapsible_if,
    clippy::collapsible_match
)]

pub mod clock;
pub mod journal_io;
pub mod snapshot;
pub mod store;
