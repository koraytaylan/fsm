//! Provisioned operator access, lifetime leadership and irreversible epochs.

use super::{
    NOFOLLOW_NONBLOCK, closed, identity, io, number, object, protected_directory, publish_once,
    read_value, text,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{
    DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt, chown,
};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn configuration(directory: &Path, uid: u32) -> Result<Value, String> {
    protected_directory(directory)?;
    if uid == 0 || uid == u32::MAX || (61184..=65519).contains(&uid) {
        return Err("broker operator overlaps privileged or handler identity".into());
    }
    Ok(object([
        ("format", Value::Str("fsm.native-broker-config/1".into())),
        (
            "authority",
            identity(&fs::symlink_metadata(directory).map_err(io)?),
        ),
        ("operator", Value::Num(uid.to_string())),
        (
            "boot",
            Value::Str(
                fs::read_to_string("/proc/sys/kernel/random/boot_id")
                    .map_err(io)?
                    .trim()
                    .into(),
            ),
        ),
    ]))
}

fn counter(configuration: &Value, epoch: u64) -> Value {
    object([
        ("format", Value::Str("fsm.native-broker-counter/1".into())),
        ("configuration", configuration.clone()),
        ("epoch", Value::Num(epoch.to_string())),
    ])
}

pub(super) fn provision(directory: &Path, uid: u32) -> Result<(), String> {
    protected_directory(directory)?;
    for ancestor in directory.ancestors() {
        if fs::symlink_metadata(ancestor).map_err(io)?.mode() & 0o001 == 0 {
            return Err("broker route is not operator-traversable".into());
        }
    }
    let _lock = super::authority_lock(directory)?;
    let configuration = configuration(directory, uid)?;
    super::catalogue::read(directory)?;
    let base = directory.join("broker");
    fs::DirBuilder::new()
        .mode(0o755)
        .create(&base)
        .map_err(io)?;
    fs::set_permissions(&base, fs::Permissions::from_mode(0o755)).map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)?;
    publish_once(&base.join("configuration.json"), &configuration)?;
    publish_once(&base.join("counter.json"), &counter(&configuration, 0))?;
    publish_once(&base.join("LOCK"), &Value::Null)
}

pub(super) struct Guard {
    directory: PathBuf,
    configuration: Value,
    broker_identity: Value,
    lock_identity: Value,
    lock: File,
}

impl Guard {
    pub(super) fn check(&self) -> Result<(), String> {
        protected_directory(&self.directory)?;
        let base = self.directory.join("broker");
        protected_directory(&base)?;
        if identity(&fs::symlink_metadata(&base).map_err(io)?) != self.broker_identity
            || read_value(&base.join("configuration.json"), true)? != self.configuration
            || configuration(
                &self.directory,
                u32::try_from(number(&self.configuration, "operator")?)
                    .map_err(|_| "broker operator invalid")?,
            )? != self.configuration
        {
            return Err("broker authority or configuration changed".into());
        }
        let metadata = fs::symlink_metadata(base.join("LOCK")).map_err(io)?;
        if !metadata.is_file()
            || metadata.uid() != 0
            || metadata.mode() & 0o077 != 0
            || identity(&metadata) != self.lock_identity
            || identity(&self.lock.metadata().map_err(io)?) != self.lock_identity
        {
            return Err("broker leadership identity changed".into());
        }
        Ok(())
    }
}

pub(super) struct Endpoint {
    pub(super) guard: Arc<Guard>,
    pub(super) listener: UnixListener,
}

fn publish_route(path: &Path, value: &Value) -> Result<(), String> {
    let pending = path.with_extension("json.pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&pending)
        .map_err(io)?;
    file.write_all(&canon_bytes(value)).map_err(io)?;
    file.set_permissions(fs::Permissions::from_mode(0o444))
        .map_err(io)?;
    file.sync_all().map_err(io)?;
    fs::rename(pending, path).map_err(io)?;
    File::open(path.parent().ok_or("broker route parent missing")?)
        .map_err(io)?
        .sync_all()
        .map_err(io)
}

pub(super) fn operator(directory: &Path) -> Result<u32, String> {
    protected_directory(directory)?;
    let base = directory.join("broker");
    protected_directory(&base)?;
    let configuration_value = read_value(&base.join("configuration.json"), true)?;
    closed(
        &configuration_value,
        &["format", "authority", "operator", "boot"],
    )?;
    let uid = u32::try_from(number(&configuration_value, "operator")?)
        .map_err(|_| "broker operator invalid")?;
    if configuration_value != configuration(directory, uid)? {
        return Err("broker provisioned configuration differs".into());
    }
    Ok(uid)
}

pub(super) fn open(directory: &Path) -> Result<Endpoint, String> {
    let uid = operator(directory)?;
    let base = directory.join("broker");
    let configuration_value = configuration(directory, uid)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(base.join("LOCK"))
        .map_err(io)?;
    let metadata = lock.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err("broker lock is not protected".into());
    }
    lock.try_lock()
        .map_err(|_| "broker authority already held")?;
    let guard = Arc::new(Guard {
        directory: directory.into(),
        configuration: configuration_value.clone(),
        broker_identity: identity(&fs::symlink_metadata(&base).map_err(io)?),
        lock_identity: identity(&metadata),
        lock,
    });
    guard.check()?;
    let value = read_value(&base.join("counter.json"), true)?;
    let last = number(&value, "epoch")?;
    if last > 4096 || value != counter(&configuration_value, last) {
        return Err("broker epoch counter differs".into());
    }
    let mut observed = 0;
    for (index, entry) in fs::read_dir(&base).map_err(io)?.enumerate() {
        if index >= 32768 {
            return Err("broker inventory exceeds bound".into());
        }
        let name = entry.map_err(io)?.file_name();
        if name == "counter.json.pending" || name == "route.json.pending" {
            return Err("broker publication remains uncertain".into());
        }
        if let Some(raw) = name
            .to_str()
            .and_then(|name| name.strip_prefix("epoch-"))
            .and_then(|name| name.strip_suffix(".json"))
        {
            let epoch = raw
                .parse::<u64>()
                .map_err(|_| "broker epoch history invalid")?;
            if epoch == 0
                || epoch > last
                || raw != epoch.to_string()
                || read_value(&base.join(&name), true)? != counter(&configuration_value, epoch)
            {
                return Err("broker epoch history or rollback differs".into());
            }
            observed += 1;
        }
    }
    if observed != last {
        return Err("broker epoch history missing".into());
    }
    match fs::symlink_metadata(base.join("route.json")) {
        Ok(_) => {
            let route = read_value(&base.join("route.json"), true)?;
            closed(&route, &["format", "configuration", "epoch", "socket"])?;
            if text(&route, "format")? != "fsm.native-broker-route/1"
                || route.get("configuration") != Some(&configuration_value)
                || number(&route, "epoch")? == 0
                || number(&route, "epoch")? > last
            {
                return Err("broker route lineage differs".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    // Permissions must be restrictive at bind time, before queued connections.
    let mut status = String::new();
    File::open("/proc/self/status")
        .map_err(io)?
        .take(8193)
        .read_to_string(&mut status)
        .map_err(io)?;
    let mask = status
        .lines()
        .find_map(|line| line.strip_prefix("Umask:"))
        .and_then(|raw| u32::from_str_radix(raw.trim(), 8).ok());
    if status.len() > 8192 || mask.is_none_or(|mask| mask & 0o077 != 0o077) {
        return Err("broker requires UMask=0077 before socket creation".into());
    }
    let next = last
        .checked_add(1)
        .filter(|next| *next <= 4096)
        .ok_or("broker epoch limit")?;
    let socket = base.join(format!("s-{next}"));
    if socket.as_os_str().len() > 107 {
        return Err("broker socket path exceeds native bound".into());
    }
    let next_value = counter(&configuration_value, next);
    publish_once(&base.join(format!("epoch-{next}.json")), &next_value)?;
    publish_once(&base.join("counter.json.pending"), &next_value)?;
    fs::rename(base.join("counter.json.pending"), base.join("counter.json")).map_err(io)?;
    File::open(&base).map_err(io)?.sync_all().map_err(io)?;
    let listener = UnixListener::bind(&socket).map_err(io)?;
    fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).map_err(io)?;
    chown(&socket, Some(uid), None).map_err(io)?;
    let socket_metadata = fs::symlink_metadata(&socket).map_err(io)?;
    if !socket_metadata.file_type().is_socket()
        || socket_metadata.uid() != uid
        || socket_metadata.mode() & 0o777 != 0o600
    {
        return Err("broker socket ownership or access differs".into());
    }
    let route = object([
        ("format", Value::Str("fsm.native-broker-route/1".into())),
        ("configuration", configuration_value),
        ("epoch", Value::Num(next.to_string())),
        ("socket", identity(&socket_metadata)),
    ]);
    File::open(&base).map_err(io)?.sync_all().map_err(io)?;
    guard.check()?;
    publish_route(&base.join("route.json"), &route)?;
    listener.set_nonblocking(true).map_err(io)?;
    Ok(Endpoint { guard, listener })
}
