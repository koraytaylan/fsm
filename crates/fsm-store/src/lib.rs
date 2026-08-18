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

/// Flush a *directory* so that a file created or renamed inside it survives a
/// crash, not just the file's own contents.
///
/// Unix only, and deliberately a no-op elsewhere. On Windows there is no
/// portable equivalent: `File::open` on a directory fails outright with
/// `ERROR_ACCESS_DENIED`, and obtaining a flushable directory handle needs
/// `FILE_FLAG_BACKUP_SEMANTICS`, which `std` does not expose.
///
/// What this does **not** affect on any platform: journal record durability.
/// Every append fsyncs the segment *file* before returning, and that works
/// identically everywhere. What is weaker on Windows is only the durability of
/// the enclosing directory entry after a create or rename — segment rotation,
/// snapshot installation, and the request-id allocation file. A crash inside
/// that window can leave the entry missing even though the bytes were flushed.
/// The store classifies and repairs that on the next open rather than trusting
/// it, so the consequence is a recovery step, not silent loss.
pub(crate) fn sync_dir(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}
