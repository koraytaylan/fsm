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
    let lease = open(&path, 0, Creation::Allowed)?;
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

enum Creation {
    Allowed,
    Forbidden,
}

pub(super) fn acquire_existing(directory: &Path, allocation: u64) -> Result<File, String> {
    protected_directory(directory)?;
    if allocation == 0 {
        return Err("runner lease allocation invalid".into());
    }
    let path = directory.join(format!("runner-{allocation}.LOCK"));
    let lease = open(&path, 0, Creation::Forbidden)?;
    let observed = fs::symlink_metadata(&path).map_err(io)?;
    let captured = lease.metadata().map_err(io)?;
    if observed.dev() != captured.dev() || observed.ino() != captured.ino() {
        return Err("runner lease identity changed".into());
    }
    protected_directory(directory)?;
    Ok(lease)
}

fn open(path: &Path, owner: u32, creation: Creation) -> Result<File, String> {
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(matches!(creation, Creation::Allowed))
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
    #[ignore = "requires a provisioned root-owned cache directory"]
    fn claimed_runner_refuses_held_lease_before_binding_without_launch() {
        let root = std::path::PathBuf::from(
            std::env::var_os("FSM_RUNNER_LEASE_NATIVE_ROOT")
                .expect("native lease test requires a protected cache root"),
        );
        assert!(!root.starts_with("/tmp"));
        protected_directory(&root).unwrap();
        let directory = root.join(format!("runner-entry-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        super::super::owner_lease::publish(&directory, 7, 65534).unwrap();
        assert!(super::super::owner_lease::publish(&directory, 7, 65534).is_err());
        assert!(super::super::owner_lease::require_held(&directory, 7, 65534).is_err());
        let owner = super::super::owner_lease::acquire(&directory, 7, 65534).unwrap();
        super::super::owner_lease::require_held(&directory, 7, 65534).unwrap();
        assert!(super::super::owner_lease::acquire(&directory, 7, 65534).is_err());
        assert!(super::super::owner_lease::acquire(&directory, 7, 65533).is_err());
        drop(owner);
        assert!(super::super::owner_lease::require_held(&directory, 7, 65534).is_err());
        let lease = acquire(&directory, 1).unwrap();
        let result = super::super::runner::execute(&directory, 1);
        let launched = directory.join("launch-1.json").exists();
        drop(lease);
        fs::remove_dir_all(&directory).unwrap();
        assert_eq!(
            result.unwrap_err(),
            "original runner remains active or lease locking is unavailable"
        );
        assert!(!launched);
    }

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
        assert!(open(&path, owner, Creation::Forbidden).is_err());
        assert!(!path.exists());
        let original = open(&path, owner, Creation::Allowed).unwrap();
        assert!(open(&path, owner, Creation::Forbidden).is_err());
        drop(original);
        let successor = open(&path, owner, Creation::Forbidden).unwrap();
        drop(successor);
        fs::remove_dir_all(directory).unwrap();
    }
}
