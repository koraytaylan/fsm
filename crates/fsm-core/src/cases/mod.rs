//! Committed expectations about what a *machine* does.
//!
//! Everything else in this crate pins the engine: the oracle duplicates its
//! semantics, the fuzz targets attack its parser, the goldens fix its bytes. A
//! machine's own behaviour — whether the definition says what its author meant
//! — is pinned by nothing, and a definition that approves the wrong requests is
//! as well-formed as one that does not.
//!
//! A case file closes that gap the smallest way that works: a document beside a
//! machine that states what a scripted run should produce, and a runner that
//! executes it against the production stepper. Nothing is persisted, no store
//! is opened, and no clock is read — the script carries its own time — so a
//! case that passes on one platform passes on every platform, and a failure is
//! always the machine's fault.
//!
//! Reading the file is the caller's job; this module takes bytes.

pub mod delta;
pub mod expect;
pub mod format;
pub mod run;
