//! Protected execution lease spanning entry, closure and result publication.

use super::{NOFOLLOW_NONBLOCK, io, protected_directory};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub(super) fn acquire(directory: &Path, allocation: u64) -> Result<File, String> {
    protected_directory(directory)?;
    if allocation == 0 {
        return Err("runner lease allocation invalid".into());
    }
    let path = directory.join(format!("runner-{allocation}.LOCK"));
    let lease = open(&path, 0)?;
    lease.sync_all().map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)?;
    protected_directory(directory)?;
    let observed = fs::symlink_metadata(&path).map_err(io)?;
    let captured = lease.metadata().map_err(io)?;
    if observed.dev() != captured.dev() || observed.ino() != captured.ino() {
        return Err("runner lease identity changed".into());
    }
    Ok(lease)
}

fn open(path: &Path, owner: u32) -> Result<File, String> {
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(path)
        .map_err(io)?;
    let metadata = lease.metadata().map_err(io)?;
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.mode() & 0o7777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() != 0
    {
        return Err("runner lease is not protected".into());
    }
    lease
        .try_lock()
        .map_err(|_| "original runner remains active or lease locking is unavailable")?;
    Ok(lease)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_open_refuses_held_lease_and_accepts_after_original_retirement() {
        let temporary_root = std::path::PathBuf::from(
            std::env::var_os("TMPDIR")
                .expect("runner lease tests require an explicit cache TMPDIR"),
        );
        assert!(!temporary_root.starts_with("/tmp"));
        let directory = temporary_root.join(format!("runner-lease-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let owner = fs::metadata(&directory).unwrap().uid();
        let path = directory.join("runner-1.LOCK");
        let original = open(&path, owner).unwrap();
        assert!(open(&path, owner).is_err());
        drop(original);
        let successor = open(&path, owner).unwrap();
        drop(successor);
        fs::remove_dir_all(directory).unwrap();
    }
}
