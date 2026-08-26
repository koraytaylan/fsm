//! Disposable store snapshots: write, verify, keep-3, and open fast-path.

mod decode;
mod encode;
mod files;
mod open;

pub use decode::*;
pub use encode::*;
pub use files::*;
pub use open::*;
