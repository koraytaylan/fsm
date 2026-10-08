//! Allocator-created operator lease whose inode cannot be replaced by its owner.

use super::{NOFOLLOW_NONBLOCK, io, protected_directory};
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, chown};
use std::path::Path;

pub(super) fn publish(directory: &Path, allocation: u64, operator: u32) -> Result<(), String> {
    protected_directory(directory)?;
    if allocation == 0 || operator == 0 || operator == u32::MAX {
        return Err("native owner lease identity invalid".into());
    }
    let path = directory.join(format!("owner-{allocation}.LOCK"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(&path)
        .map_err(io)?;
    chown(&path, Some(operator), None).map_err(io)?;
    file.sync_all().map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)?;
    check(directory, allocation, operator, &file)
}

fn open(directory: &Path, allocation: u64, operator: u32) -> Result<File, String> {
    protected_directory(directory)?;
    if allocation == 0 || operator == 0 || operator == u32::MAX {
        return Err("native owner lease identity invalid".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(directory.join(format!("owner-{allocation}.LOCK")))
        .map_err(io)?;
    check(directory, allocation, operator, &file)?;
    Ok(file)
}

fn check(directory: &Path, allocation: u64, operator: u32, file: &File) -> Result<(), String> {
    protected_directory(directory)?;
    let captured = file.metadata().map_err(io)?;
    let observed =
        fs::symlink_metadata(directory.join(format!("owner-{allocation}.LOCK"))).map_err(io)?;
    if !captured.is_file()
        || captured.uid() != operator
        || captured.mode() & 0o7777 != 0o600
        || captured.nlink() != 1
        || captured.len() != 0
        || captured.dev() != observed.dev()
        || captured.ino() != observed.ino()
        || captured.mode() != observed.mode()
        || captured.uid() != observed.uid()
        || observed.nlink() != 1
        || observed.len() != 0
    {
        return Err("native owner lease protection or identity differs".into());
    }
    Ok(())
}

pub(super) fn acquire(directory: &Path, allocation: u64, operator: u32) -> Result<File, String> {
    let file = open(directory, allocation, operator)?;
    file.try_lock()
        .map_err(|_| "original preparation owner remains active or lease locking is unavailable")?;
    check(directory, allocation, operator, &file)?;
    Ok(file)
}

pub(super) fn require_held(directory: &Path, allocation: u64, operator: u32) -> Result<(), String> {
    let file = open(directory, allocation, operator)?;
    match file.try_lock() {
        Err(std::fs::TryLockError::WouldBlock) => check(directory, allocation, operator, &file),
        Err(std::fs::TryLockError::Error(error)) => Err(io(error)),
        Ok(()) => Err("native preparation owner lease is not held".into()),
    }
}
