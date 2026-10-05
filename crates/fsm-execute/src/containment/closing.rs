//! Durable admission revocation; never evidence that a domain has terminated.

use super::{
    authority_lock, authority_path, closed, identity, io, number, object, protected_directory,
    publish_once, read_value, text,
};
use fsm_core::json::Value;
use fsm_core::record::execution::NativeDomain;
use std::fs::{self, File};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) fn begin(directory: &Path, allocation: u64) -> Result<(), String> {
    revoke(directory, allocation).map(|_| ())
}

pub(super) fn revoke(directory: &Path, allocation: u64) -> Result<(Value, File), String> {
    protected_directory(directory)?;
    let lock = authority_lock(directory)?;
    let domain = revoke_locked(directory, allocation)?;
    Ok((domain, lock))
}

/// Caller retains the authority lock through this entire revocation.
pub(super) fn revoke_locked(directory: &Path, allocation: u64) -> Result<Value, String> {
    let domain = prepared_domain(directory, allocation)?;
    revoke_domain(directory, allocation, domain)
}

/// A verified completed handoff permits revocation after natural native exit;
/// caller holds the authority lock and still must prove manager retirement.
pub(super) fn revoke_after_handoff(directory: &Path, allocation: u64) -> Result<Value, String> {
    let domain = recorded_domain(directory, allocation)?;
    let group = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    ));
    protected_directory(group.parent().ok_or("closing native parent missing")?)?;
    match fs::symlink_metadata(&group) {
        Ok(observed) => {
            if !observed.is_dir()
                || observed.uid() != 0
                || observed.mode() & 0o022 != 0
                || domain.get("cgroup") != Some(&identity(&observed))
            {
                return Err("closing native domain identity changed".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    revoke_domain(directory, allocation, domain)
}

fn revoke_domain(directory: &Path, allocation: u64, domain: Value) -> Result<Value, String> {
    match fs::symlink_metadata(directory.join(format!("closed-{allocation}.json"))) {
        Ok(_) => return Err("allocation already carries closed evidence".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let material = object([
        ("format", Value::Str("fsm.native-closing/1".into())),
        ("domain", domain.clone()),
    ]);
    let marker = directory.join(format!("closing-{allocation}.json"));
    match fs::symlink_metadata(&marker) {
        Ok(_) => {
            if read_value(&marker, true)? != material {
                return Err("closing material differs".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            publish_once(&marker, &material)?
        }
        Err(error) => return Err(io(error)),
    }
    // A valid marker may survive an earlier sync failure: visibility alone
    // cannot establish the durable ordering required before revocation.
    File::open(&marker).map_err(io)?.sync_all().map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)?;
    for suffix in ["json", "json.pending"] {
        let grant = directory.join(format!("entry-{allocation}.{suffix}"));
        match fs::symlink_metadata(&grant) {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
                    return Err("closing refuses unexpected grant ownership or type".into());
                }
                fs::remove_file(grant).map_err(io)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
    }
    File::open(directory).map_err(io)?.sync_all().map_err(io)?;
    Ok(domain)
}

pub(super) fn prepared_domain(directory: &Path, allocation: u64) -> Result<Value, String> {
    let domain = recorded_domain(directory, allocation)?;
    let namespace = text(&domain, "namespace")?;
    let generation = number(&domain, "generation")?.to_string();
    let groups = Path::new("/sys/fs/cgroup/system.slice");
    protected_directory(groups)?;
    let group = groups.join(format!(
        "fsm-containment-{namespace}-{generation}-{allocation}.service"
    ));
    let observed = fs::symlink_metadata(group).map_err(io)?;
    if !observed.is_dir()
        || observed.uid() != 0
        || observed.mode() & 0o022 != 0
        || domain.get("cgroup") != Some(&identity(&observed))
    {
        return Err("closing native domain identity differs".into());
    }
    Ok(domain)
}

/// Recorded authority identity remains checkable after native domain retirement.
pub(super) fn recorded_domain(directory: &Path, allocation: u64) -> Result<Value, String> {
    protected_directory(directory)?;
    let prepared = read_value(&directory.join(format!("prepared-{allocation}.json")), true)?;
    closed(&prepared, &["format", "phase", "domain"])?;
    if text(&prepared, "format")? != "fsm.native-prepared/1"
        || text(&prepared, "phase")? != "prepared"
    {
        return Err("closing requires a protected prepared allocation".into());
    }
    let domain = NativeDomain::from_value(prepared.get("domain").ok_or("domain missing")?)
        .map_err(|error| error.to_string())?
        .to_value();
    let namespace = text(&domain, "namespace")?;
    let generation = number(&domain, "generation")?.to_string();
    if number(&domain, "allocation")? != allocation
        || authority_path(namespace, &generation)? != directory
        || domain.get("authority") != Some(&identity(&fs::symlink_metadata(directory).map_err(io)?))
        || fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map_err(io)?
            .trim()
            != text(&domain, "boot")?
    {
        return Err("closing domain authority or route differs".into());
    }
    Ok(domain)
}
