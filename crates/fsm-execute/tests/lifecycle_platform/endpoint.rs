//! Private provisioned endpoint epochs. No stale socket is reclaimed or reused.
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, chown};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use super::super::identity_root::{put, read};

fn route(base: &Path) -> Result<(u64, String, u32), String> {
    let path = base.join("endpoint");
    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err("untrusted endpoint route".into());
    }
    let value = read(&path)?;
    let lines: Vec<_> = value.lines().collect();
    if lines.len() != 4 || lines[0] != "endpoint/1" || lines[2].len() != 36 {
        return Err("invalid endpoint route".into());
    }
    let epoch = lines[1].parse().map_err(|_| "invalid endpoint epoch")?;
    let uid = lines[3].parse().map_err(|_| "invalid endpoint operator")?;
    if epoch == 0 || uid == 0 {
        return Err("invalid endpoint identity".into());
    }
    Ok((epoch, lines[2].to_owned(), uid))
}

pub(super) fn connect_path(base: &Path) -> Result<PathBuf, String> {
    let meta = fs::symlink_metadata(base).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.uid() != 0 || meta.mode() & 0o022 != 0 {
        return Err("untrusted endpoint namespace".into());
    }
    let (epoch, boot, _) = route(base)?;
    if read(Path::new("/proc/sys/kernel/random/boot_id"))?.trim() != boot {
        return Err("endpoint belongs to another boot".into());
    }
    Ok(base.join(format!("control-{epoch}.sock")))
}

pub(super) fn bind(base: &Path, uid: u32) -> Result<(File, UnixListener, PathBuf), String> {
    let data = base.join("data");
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
            let (published, _, operator) = route(base)?;
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
    put(
        &path,
        &format!("endpoint/1\n{epoch}\n{}\n{uid}\n", boot.trim()),
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).map_err(|e| e.to_string())?;
    File::open(path)
        .map_err(|e| e.to_string())?
        .sync_all()
        .map_err(|e| e.to_string())?;
    Ok((lock, listener, socket))
}
