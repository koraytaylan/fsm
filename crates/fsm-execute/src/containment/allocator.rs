//! Durable native allocation without executing handler code.

use super::{
    closed, identity, io, number, object, protected_directory, publish_once, read_value, text,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::record::execution::{FileIdentity, NativeDomain};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

const GROUPS: &str = "/sys/fs/cgroup/system.slice";
const MAX_ALLOCATIONS: u64 = 4096;

fn origin(directory: &Path) -> Result<Value, String> {
    let generation = directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("authority-"))
        .ok_or("invalid authority path")?;
    let namespace = directory
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or("invalid namespace path")?;
    if super::authority_path(namespace, generation)? != directory {
        return Err("authority path differs".into());
    }
    Ok(object([
        (
            "format",
            Value::Str("fsm.native-allocation-counter/1".into()),
        ),
        ("namespace", Value::Str(namespace.into())),
        ("generation", Value::Num(generation.into())),
        (
            "authority",
            identity(&fs::symlink_metadata(directory).map_err(io)?),
        ),
        (
            "boot",
            Value::Str(
                fs::read_to_string("/proc/sys/kernel/random/boot_id")
                    .map_err(io)?
                    .trim()
                    .into(),
            ),
        ),
        ("last_allocation", Value::Num("0".into())),
    ]))
}

pub(super) fn initialize(directory: &Path) -> Result<(), String> {
    publish_once(&directory.join("counter.json"), &origin(directory)?)
}

fn counter(value: &Value, expected: &Value) -> Result<u64, String> {
    closed(
        value,
        &[
            "format",
            "namespace",
            "generation",
            "authority",
            "boot",
            "last_allocation",
        ],
    )?;
    for field in ["format", "namespace", "generation", "authority", "boot"] {
        if value.get(field) != expected.get(field) {
            return Err("allocation authority differs".into());
        }
    }
    let last = number(value, "last_allocation")?;
    if last > MAX_ALLOCATIONS {
        return Err("allocation inventory exceeds bound".into());
    }
    Ok(last)
}

fn prefix(origin: &Value) -> Result<String, String> {
    Ok(format!(
        "fsm-containment-{}-{}-",
        text(origin, "namespace")?,
        number(origin, "generation")?
    ))
}

fn cgroup(origin: &Value, allocation: u64) -> Result<PathBuf, String> {
    Ok(Path::new(GROUPS).join(format!("{}{allocation}.service", prefix(origin)?)))
}

fn intent(origin: &Value, allocation: u64) -> Value {
    let mut value = origin.clone();
    if let Value::Obj(fields) = &mut value {
        fields.remove("last_allocation");
        fields.insert(
            "format".into(),
            Value::Str("fsm.native-allocation-intent/1".into()),
        );
        fields.insert("allocation".into(), Value::Num(allocation.to_string()));
    }
    value
}

fn inventory(directory: &Path, origin: &Value, last: u64) -> Result<(), String> {
    let mut intents = 0;
    for (index, entry) in fs::read_dir(directory).map_err(io)?.enumerate() {
        if index >= 32768 {
            return Err("authority directory inventory exceeds bound".into());
        }
        let name = entry.map_err(io)?.file_name();
        let Some(name) = name
            .to_str()
            .and_then(|name| name.strip_prefix("allocation-"))
        else {
            continue;
        };
        let number_text = name
            .strip_suffix(".json")
            .ok_or("unknown allocation record")?;
        let allocation = number_text
            .parse::<u64>()
            .map_err(|_| "invalid allocation record")?;
        if allocation == 0 || allocation > last || number_text != allocation.to_string() {
            return Err("allocation counter rollback or alias".into());
        }
        if read_value(
            &directory.join(format!("allocation-{allocation}.json")),
            true,
        )? != intent(origin, allocation)
        {
            return Err("allocation intent differs".into());
        }
        intents += 1;
    }
    if intents != last {
        return Err("allocation intent inventory differs".into());
    }
    for allocation in 1..=last {
        let prepared = read_value(&directory.join(format!("prepared-{allocation}.json")), true)?;
        closed(&prepared, &["format", "phase", "domain"])?;
        let domain = NativeDomain::from_value(prepared.get("domain").ok_or("domain missing")?)
            .map_err(|error| error.to_string())?
            .to_value();
        if text(&prepared, "format")? != "fsm.native-prepared/1"
            || text(&prepared, "phase")? != "prepared"
            || number(&domain, "allocation")? != allocation
            || ["namespace", "generation", "boot", "authority"]
                .iter()
                .any(|field| domain.get(field) != origin.get(field))
        {
            return Err("prepared allocation identity differs".into());
        }
        let path = cgroup(origin, allocation)?;
        let tombstone = directory.join(format!("closed-{allocation}.json"));
        if tombstone.exists() {
            let closed = read_value(&tombstone, true)?;
            if closed
                != object([
                    ("format", Value::Str("fsm.native-domain-closed/1".into())),
                    ("domain", domain),
                ])
                || path.exists()
            {
                return Err("closed native domain reappeared or differs".into());
            }
        } else {
            let metadata = fs::symlink_metadata(&path).map_err(io)?;
            if !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.mode() & 0o022 != 0
                || domain.get("cgroup") != Some(&identity(&metadata))
            {
                return Err("known native domain lost identity".into());
            }
        }
    }
    let prefix = prefix(origin)?;
    for (index, entry) in fs::read_dir(GROUPS).map_err(io)?.enumerate() {
        if index >= 32768 {
            return Err("native directory inventory exceeds bound".into());
        }
        let name = entry.map_err(io)?.file_name();
        let Some(suffix) = name.to_str().and_then(|name| name.strip_prefix(&prefix)) else {
            continue;
        };
        let number_text = suffix
            .strip_suffix(".service")
            .ok_or("unknown native domain")?;
        let allocation = number_text
            .parse::<u64>()
            .map_err(|_| "unknown native domain")?;
        if allocation == 0 || allocation > last || number_text != allocation.to_string() {
            return Err("unknown native domain refuses allocation".into());
        }
    }
    Ok(())
}

fn advance(directory: &Path, mut value: Value, next: u64) -> Result<(), String> {
    let Value::Obj(fields) = &mut value else {
        return Err("counter is not an object".into());
    };
    fields.insert("last_allocation".into(), Value::Num(next.to_string()));
    let temporary = directory.join("counter.json.pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(io)?;
    file.write_all(&canon_bytes(&value)).map_err(io)?;
    file.sync_all().map_err(io)?;
    fs::rename(temporary, directory.join("counter.json")).map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)
}

pub(super) fn prepare(directory: &Path) -> Result<Value, String> {
    protected_directory(directory)?;
    // A missing/delegated facility refuses before burning an allocation.
    protected_directory(Path::new(GROUPS))?;
    super::manager::require()?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(super::NOFOLLOW_NONBLOCK)
        .open(directory.join("LOCK"))
        .map_err(io)?;
    let metadata = lock.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err("authority lock is not protected".into());
    }
    lock.try_lock().map_err(|_| "authority busy")?;
    let origin = origin(directory)?;
    let registration = read_value(&directory.join("store.json"), true)?;
    closed(&registration, &["format", "path", "identity"])?;
    let store_metadata =
        fs::symlink_metadata(Path::new(text(&registration, "path")?)).map_err(io)?;
    if text(&registration, "format")? != "fsm.native-store-registration/1"
        || !store_metadata.is_dir()
        || registration.get("identity") != Some(&identity(&store_metadata))
    {
        return Err("registered store identity differs".into());
    }
    let counter_value = read_value(&directory.join("counter.json"), true)?;
    let last = counter(&counter_value, &origin)?;
    inventory(directory, &origin, last)?;
    let next = last
        .checked_add(1)
        .filter(|next| *next <= MAX_ALLOCATIONS)
        .ok_or("authority generation allocation limit")?;
    publish_once(
        &directory.join(format!("allocation-{next}.json")),
        &intent(&origin, next),
    )?;
    advance(directory, counter_value, next)?;
    let path = cgroup(&origin, next)?;
    fs::create_dir(&path).map_err(io)?;
    let metadata = fs::symlink_metadata(&path).map_err(io)?;
    if !metadata.is_dir()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || !fs::read_to_string(path.join("cgroup.events"))
            .map_err(io)?
            .lines()
            .any(|line| line == "populated 0")
    {
        return Err("new native domain is not protected and empty".into());
    }
    let authority = fs::symlink_metadata(directory).map_err(io)?;
    let domain = NativeDomain::new(
        text(&origin, "namespace")?.into(),
        next,
        text(&origin, "boot")?.into(),
        FileIdentity::new(metadata.dev(), metadata.ino()).map_err(|error| error.to_string())?,
        FileIdentity::new(authority.dev(), authority.ino()).map_err(|error| error.to_string())?,
        number(&origin, "generation")?,
    )
    .map_err(|error| error.to_string())?
    .to_value();
    publish_once(
        &directory.join(format!("prepared-{next}.json")),
        &object([
            ("format", Value::Str("fsm.native-prepared/1".into())),
            ("phase", Value::Str("prepared".into())),
            ("domain", domain.clone()),
        ]),
    )?;
    Ok(domain)
}

#[cfg(test)]
#[path = "allocator_native_tests.rs"]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_refuses_copied_authority_boot_change_alias_and_exhaustion() {
        let expected = object([
            (
                "format",
                Value::Str("fsm.native-allocation-counter/1".into()),
            ),
            ("namespace", Value::Str("a".repeat(32))),
            ("generation", Value::Num("1".into())),
            ("authority", Value::Null),
            ("boot", Value::Str("boot".into())),
            ("last_allocation", Value::Num("0".into())),
        ]);
        assert_eq!(counter(&expected, &expected).unwrap(), 0);
        let mut at_limit = expected.clone();
        if let Value::Obj(fields) = &mut at_limit {
            fields.insert("last_allocation".into(), Value::Num("4096".into()));
        }
        assert_eq!(counter(&at_limit, &expected).unwrap(), 4096);
        for (field, replacement) in [
            ("authority", Value::Str("copied".into())),
            ("boot", Value::Str("different".into())),
            ("last_allocation", Value::Num("4097".into())),
            ("last_allocation", Value::Str("1".into())),
        ] {
            let mut changed = expected.clone();
            if let Value::Obj(fields) = &mut changed {
                fields.insert(field.into(), replacement);
            }
            assert!(counter(&changed, &expected).is_err());
        }
        assert_eq!(
            cgroup(&expected, 2).unwrap(),
            Path::new(GROUPS).join(format!("fsm-containment-{}-1-2.service", "a".repeat(32)))
        );
    }
}
