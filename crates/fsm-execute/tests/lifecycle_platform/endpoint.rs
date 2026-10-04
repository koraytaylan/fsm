//! Private provisioned endpoint epochs. No stale socket is reclaimed or reused.
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, chown};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use super::super::identity_root::{put, put_public, read};

fn route(base: &Path) -> Result<(u64, String, u32, u64, u64), String> {
    let path = base.join("endpoint");
    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err("untrusted endpoint route".into());
    }
    let value = read(&path)?;
    let lines: Vec<_> = value.lines().collect();
    if lines.len() != 6 || lines[0] != "endpoint/2" || lines[2].len() != 36 {
        return Err("invalid endpoint route".into());
    }
    let epoch = lines[1].parse().map_err(|_| "invalid endpoint epoch")?;
    let uid = lines[3].parse().map_err(|_| "invalid endpoint operator")?;
    if epoch == 0 || uid == 0 {
        return Err("invalid endpoint identity".into());
    }
    let device = lines[4].parse().map_err(|_| "invalid authority device")?;
    let inode = lines[5].parse().map_err(|_| "invalid authority inode")?;
    Ok((epoch, lines[2].to_owned(), uid, device, inode))
}

pub(super) fn connect_path(base: &Path) -> Result<PathBuf, String> {
    let meta = fs::symlink_metadata(base).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err("untrusted endpoint namespace".into());
    }
    let (epoch, boot, _, _, _) = route(base)?;
    if read(Path::new("/proc/sys/kernel/random/boot_id"))?.trim() != boot {
        return Err("endpoint belongs to another boot".into());
    }
    Ok(base.join(format!("control-{epoch}.sock")))
}

pub(super) struct Authority {
    _lock: File,
    device: u64,
    inode: u64,
}

impl Authority {
    pub(super) fn check(&self, base: &Path) -> Result<(), String> {
        let meta = fs::symlink_metadata(base.join("data"))
            .map_err(|_| "broker authority directory unavailable")?;
        if !meta.is_dir()
            || meta.uid() != 0
            || meta.mode() & 0o077 != 0
            || meta.dev() != self.device
            || meta.ino() != self.inode
        {
            return Err("broker authority directory changed".into());
        }
        Ok(())
    }
}

pub(super) fn bind(base: &Path, uid: u32) -> Result<(Authority, UnixListener, PathBuf), String> {
    let data = base.join("data");
    let generation = fs::symlink_metadata(&data).map_err(|e| e.to_string())?;
    if !generation.is_dir() || generation.uid() != 0 || generation.mode() & 0o077 != 0 {
        return Err("broker authority directory changed".into());
    }
    // Check protected published lineage before opening a lock in a possibly
    // replaced directory. A copied counter is not the original authority.
    match fs::symlink_metadata(base.join("endpoint")) {
        Ok(_) => {
            let (_, _, _, device, inode) = route(base)?;
            if generation.dev() != device || generation.ino() != inode {
                return Err("broker authority directory changed".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(data.join("BROKER-LOCK"))
        .map_err(|e| e.to_string())?;
    lock.try_lock()
        .map_err(|_| "broker authority already held")?;
    // Missing counter is lost authority, never permission to initialize it.
    let counter_path = data.join("broker-counter");
    let previous: u64 = read(&counter_path)
        .map_err(|_| "missing or unreadable broker authority counter")?
        .trim()
        .parse()
        .map_err(|_| "invalid broker authority counter")?;
    match fs::symlink_metadata(base.join("endpoint")) {
        Ok(_) => {
            let (published, _, operator, _, _) = route(base)?;
            if previous < published || operator != uid {
                return Err("broker counter rollback or operator mismatch".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let epoch = previous.checked_add(1).ok_or("broker epoch exhausted")?;
    // Burn the epoch durably before any socket can be published.
    put(&counter_path, &format!("{epoch}\n"))?;
    let socket = base.join(format!("control-{epoch}.sock"));
    let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    chown(&socket, Some(uid), None).map_err(|e| e.to_string())?;
    let boot = read(Path::new("/proc/sys/kernel/random/boot_id"))?;
    let path = base.join("endpoint");
    put_public(
        &path,
        &format!(
            "endpoint/2\n{epoch}\n{}\n{uid}\n{}\n{}\n",
            boot.trim(),
            generation.dev(),
            generation.ino()
        ),
    )?;
    let authority = Authority {
        _lock: lock,
        device: generation.dev(),
        inode: generation.ino(),
    };
    authority.check(base)?;
    Ok((authority, listener, socket))
}
