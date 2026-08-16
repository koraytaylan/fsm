//! Pure library for the `fsm` statechart engine.
//!
//! This crate performs no I/O, reads no clock, and holds no platform-dependent
//! state; `src/` must not name `std::fs`, `std::net`, `std::time`, `f32`,
//! `f64`, or `HashMap`.

#![forbid(unsafe_code)] // POLICY_ALLOW: the forbid attribute must name the token

pub mod analyze;
pub mod canon;
pub mod decimal;
pub mod diagram;
pub mod error;
pub mod expr;
pub mod hashes;
pub mod ident;
pub mod json;
pub mod limits;
pub mod machine;
pub mod record;
pub mod replay;
pub mod sha256;
pub mod simulate;
pub mod spec;
pub mod step;
pub mod trace;
pub mod tree;
