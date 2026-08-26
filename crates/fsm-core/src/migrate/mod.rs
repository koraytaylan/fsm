//! Moving a running instance onto a corrected definition.
//!
//! The mapping is declared by the **new** definition (`supersedes`), so it is
//! inside that machine's `machine_id` and cannot be reinterpreted later. This
//! module is what reads it: admission checks that need both definitions in
//! hand, the pure function that moves one instance, the carry-over rules for
//! everything an instance holds besides its active state, and the preview
//! that answers "what would this do" before anything is written.
//!
//! Plan 0011.

pub mod apply;
pub mod carryover;
pub mod preview;
pub mod validate;
