//! Opt-in owner-only control transport for an actual owned native driver.
//! Production CLI backend selection is separate from endpoint publication.

mod client;
mod endpoint;
mod protocol;
mod server;

pub use client::stop;
pub use endpoint::LocalControlEndpoint;

use std::{fs, io, os::unix::fs::MetadataExt, path::Path};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn owner_uid() -> io::Result<u32> {
    Ok(fs::metadata("/proc/self")?.uid())
}

fn private_directory(path: &Path, uid: u32) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o777 != 0o700 {
        return Err(invalid("control directory must be original and owner-only"));
    }
    Ok(())
}

fn finite_timeout(timeout_ms: i64) -> io::Result<std::time::Duration> {
    if !(1..=fsm_execute::config::MAX_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(invalid("control timeout outside finite bounds"));
    }
    Ok(std::time::Duration::from_millis(timeout_ms as u64))
}
