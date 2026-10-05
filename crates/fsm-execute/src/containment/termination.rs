//! Submit kernel termination without manufacturing verified closure evidence.

use super::{NOFOLLOW_NONBLOCK, closing, identity, io, number, text};
use fsm_core::json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub(super) fn request(directory: &Path, allocation: u64) -> Result<(), String> {
    let (domain, _lock) = closing::revoke(directory, allocation)?;
    let group = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    ));
    for control in ["cgroup.freeze", "cgroup.kill"] {
        matched(&group, &domain)?;
        let mut file = OpenOptions::new()
            .write(true)
            .custom_flags(NOFOLLOW_NONBLOCK)
            .open(group.join(control))
            .map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
        let expected_device = number(
            domain.get("cgroup").ok_or("cgroup identity missing")?,
            "device",
        )?;
        if !metadata.is_file()
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
            || metadata.dev() != expected_device
        {
            return Err("native termination control is not protected".into());
        }
        // Hold the root authority lock and verify the parent again before
        // using the opened control; other authority operations cannot launch.
        matched(&group, &domain)?;
        file.write_all(b"1").map_err(io)?;
    }
    Ok(())
}

fn matched(group: &Path, domain: &Value) -> Result<(), String> {
    let metadata = fs::symlink_metadata(group).map_err(io)?;
    if !metadata.is_dir()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || domain.get("cgroup") != Some(&identity(&metadata))
    {
        return Err("native termination domain identity differs".into());
    }
    Ok(())
}
