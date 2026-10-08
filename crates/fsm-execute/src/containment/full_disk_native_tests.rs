//! A strictly capped, swap-disabled filesystem for actual ENOSPC acceptance.
use super::*;
use std::process::{Command, Stdio};

pub(super) enum Storage {
    Ordinary,
    Bounded,
}

pub(super) fn install(store: &Path, storage: Storage) -> Option<Value> {
    if matches!(storage, Storage::Ordinary) {
        return None;
    }
    // Mount before registration or any journal writer: every actor observes
    // the same physical store and LOCK; the host filesystem is never filled.
    fs::DirBuilder::new().mode(0o700).create(store).unwrap();
    run(Command::new("/usr/bin/mount")
        .args([
            "-t",
            "tmpfs",
            "-o",
            "size=2m,noswap,mode=0700",
            "fsm-shutdown-enospc",
        ])
        .arg(store))
    .unwrap();
    Some(identity(&fs::symlink_metadata(store).unwrap()))
}

pub(super) fn retire(store: &Path, expected: &Value) -> Result<(), String> {
    if identity(&fs::symlink_metadata(store).map_err(io)?) != *expected {
        return Err("full-disk fixture refuses changed mounted store".into());
    }
    // The caller has already proved every original native domain closed;
    // failed scenarios retain the mount and journal rather than discarding it.
    run(Command::new("/usr/bin/umount").arg(store))
}

fn run(command: &mut Command) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(io)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().map_err(io)? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "bounded full-disk mount operation failed: {status}"
                ))
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err("bounded full-disk mount operation timed out; retain fixture".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

impl Fixture {
    pub(super) fn new_for_full_disk_workflow(table: Value) -> Self {
        Self::new_for_storage(table, true, Storage::Bounded)
    }
}
