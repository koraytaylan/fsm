//! Bounded discovery of one protected physical-store authority; never admission.

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use std::fs::{self, Metadata, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::Path;

const BASE: &str = "/var/lib/fsm-containment";
const LIMIT: usize = 4096;

fn identity(metadata: &Metadata) -> Value {
    Value::Obj(std::collections::BTreeMap::from([
        ("device".into(), Value::Num(metadata.dev().to_string())),
        ("inode".into(), Value::Num(metadata.ino().to_string())),
    ]))
}

fn snapshot(metadata: &Metadata) -> (u64, u64, u32, u32, u64) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.uid(),
        metadata.mode(),
        metadata.len(),
    )
}

fn directory(path: &Path) -> Result<Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(super::message)?;
    if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
        return Err("native discovery directory is not root protected".into());
    }
    Ok(metadata)
}

fn unchanged_directory(path: &Path, before: &Metadata) -> Result<(), String> {
    let after = directory(path)?;
    if identity(&after) != identity(before) || after.mode() != before.mode() {
        return Err("native discovery directory changed".into());
    }
    Ok(())
}

fn document(path: &Path) -> Result<Value, String> {
    let before = fs::symlink_metadata(path).map_err(super::message)?;
    if !before.is_file() || before.uid() != 0 || before.mode() & 0o7777 != 0o444 {
        return Err("native discovery document is not immutable root publication".into());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(0o400000 | 0o4000)
        .open(path)
        .map_err(super::message)?;
    if snapshot(&file.metadata().map_err(super::message)?) != snapshot(&before) {
        return Err("native discovery document changed before read".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(super::message)?;
    if bytes.len() > LIMIT {
        return Err("native discovery document exceeds bound".into());
    }
    if snapshot(&file.metadata().map_err(super::message)?) != snapshot(&before)
        || snapshot(&fs::symlink_metadata(path).map_err(super::message)?) != snapshot(&before)
    {
        return Err("native discovery document changed during read".into());
    }
    let value = parse(
        &bytes,
        &JsonLimits {
            max_depth: 64,
            max_bytes: LIMIT,
        },
    )
    .map_err(|_| "native discovery document JSON invalid")?;
    if canon_bytes(&value) != bytes {
        return Err("native discovery document is not canonical".into());
    }
    Ok(value)
}

fn closed(value: &Value, names: &[&str]) -> Result<(), String> {
    let fields = value.as_obj().ok_or("native discovery object required")?;
    if fields.len() != names.len() || names.iter().any(|name| !fields.contains_key(*name)) {
        return Err("native discovery object fields differ".into());
    }
    Ok(())
}

fn number(value: &Value, name: &str) -> Result<u64, String> {
    let encoded = value
        .get(name)
        .and_then(Value::as_num)
        .ok_or("native discovery number missing")?;
    let number = encoded
        .parse::<u64>()
        .map_err(|_| "native discovery number invalid")?;
    if number.to_string() != encoded {
        return Err("native discovery number is not canonical".into());
    }
    Ok(number)
}

fn charge(entries: &mut usize) -> Result<(), String> {
    if *entries == LIMIT {
        return Err("native discovery inventory exceeds bound".into());
    }
    *entries += 1;
    Ok(())
}

fn namespace(name: &str) -> bool {
    name.len() == 32
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn generation(name: &str) -> Result<u64, String> {
    let encoded = name
        .strip_prefix("authority-")
        .ok_or("native discovery generation name invalid")?;
    let generation = encoded
        .parse::<u64>()
        .map_err(|_| "native discovery generation invalid")?;
    if generation == 0 || generation.to_string() != encoded {
        return Err("native discovery generation is not canonical positive decimal".into());
    }
    Ok(generation)
}

fn route(authority: &Path) -> Result<(), String> {
    let before = directory(authority)?;
    let broker = authority.join("broker");
    let broker_before = directory(&broker)?;
    let route = document(&broker.join("route.json"))?;
    closed(&route, &["format", "configuration", "epoch", "socket"])?;
    let config = route
        .get("configuration")
        .ok_or("native discovery configuration missing")?;
    closed(config, &["format", "authority", "operator", "boot"])?;
    let uid = number(config, "operator")?;
    let epoch = number(&route, "epoch")?;
    let actual_uid = fs::metadata("/proc/self").map_err(super::message)?.uid();
    if route.get("format").and_then(Value::as_str) != Some("fsm.native-broker-route/1")
        || config.get("format").and_then(Value::as_str) != Some("fsm.native-broker-config/1")
        || uid == 0
        || uid >= u64::from(u32::MAX)
        || (61184..=65519).contains(&uid)
        || uid != u64::from(actual_uid)
        || epoch == 0
        || epoch > 4096
        || config.get("authority") != Some(&identity(&before))
        || config.get("boot").and_then(Value::as_str)
            != Some(
                fs::read_to_string("/proc/sys/kernel/random/boot_id")
                    .map_err(super::message)?
                    .trim(),
            )
    {
        return Err("native discovery operator, boot or authority differs".into());
    }
    let socket = fs::symlink_metadata(broker.join(format!("s-{epoch}"))).map_err(super::message)?;
    if !socket.file_type().is_socket()
        || u64::from(socket.uid()) != uid
        || socket.mode() & 0o7777 != 0o600
        || route.get("socket") != Some(&identity(&socket))
    {
        return Err("native discovery socket identity or access differs".into());
    }
    unchanged_directory(&broker, &broker_before)?;
    unchanged_directory(authority, &before)
}

pub(super) fn discover(store: &Path) -> Result<(String, u64), String> {
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return Err("native discovery platform unsupported".into());
    }
    let store_before = fs::symlink_metadata(store).map_err(super::message)?;
    if !store_before.is_dir() {
        return Err("native discovery store is not a directory".into());
    }
    let base = Path::new(BASE);
    for ancestor in base.ancestors() {
        directory(ancestor)?;
    }
    let base_before = directory(base)?;
    let mut entries = 0;
    let mut selected = None;
    for entry in fs::read_dir(base).map_err(super::message)? {
        charge(&mut entries)?;
        let entry = entry.map_err(super::message)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "native discovery namespace encoding invalid")?;
        if !namespace(&name) {
            return Err("native discovery namespace name invalid".into());
        }
        let namespace_path = entry.path();
        let namespace_before = directory(&namespace_path)?;
        for entry in fs::read_dir(&namespace_path).map_err(super::message)? {
            charge(&mut entries)?;
            let entry = entry.map_err(super::message)?;
            let name_generation = entry
                .file_name()
                .into_string()
                .map_err(|_| "native discovery generation encoding invalid")?;
            // Namespace siblings may be operator stores; charge their inventory
            // entry without treating them as an authority registration.
            if !name_generation.starts_with("authority-") {
                continue;
            }
            let generation = generation(&name_generation)?;
            let authority = entry.path();
            let before = directory(&authority)?;
            let registration = document(&authority.join("store-identity.json"))?;
            closed(&registration, &["format", "identity"])?;
            if registration.get("format").and_then(Value::as_str)
                != Some("fsm.native-store-identity/1")
            {
                return Err("native discovery store identity format invalid".into());
            }
            let physical = registration
                .get("identity")
                .ok_or("native discovery physical identity missing")?;
            closed(physical, &["device", "inode"])?;
            number(physical, "device")?;
            number(physical, "inode")?;
            if physical == &identity(&store_before) {
                if selected.is_some() {
                    return Err("native discovery store registration is ambiguous".into());
                }
                selected = Some((name.clone(), generation));
            }
            unchanged_directory(&authority, &before)?;
        }
        unchanged_directory(&namespace_path, &namespace_before)?;
    }
    unchanged_directory(base, &base_before)?;
    let selected = selected.ok_or("native discovery store registration missing")?;
    let authority = base
        .join(&selected.0)
        .join(format!("authority-{}", selected.1));
    route(&authority)?;
    let registration = document(&authority.join("store-identity.json"))?;
    closed(&registration, &["format", "identity"])?;
    if registration.get("format").and_then(Value::as_str) != Some("fsm.native-store-identity/1")
        || registration.get("identity") != Some(&identity(&store_before))
    {
        return Err("native discovery selected store identity changed".into());
    }
    for ancestor in authority.ancestors() {
        directory(ancestor)?;
    }
    if identity(&fs::symlink_metadata(store).map_err(super::message)?) != identity(&store_before) {
        return Err("native discovery physical store changed".into());
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_charges_exact_limit_and_refuses_next_entry() {
        let mut entries = 0;
        for _ in 0..LIMIT {
            charge(&mut entries).unwrap();
        }
        assert_eq!(entries, LIMIT);
        assert_eq!(
            charge(&mut entries).unwrap_err(),
            "native discovery inventory exceeds bound"
        );
        assert_eq!(entries, LIMIT);
    }

    #[test]
    fn route_names_require_canonical_namespace_and_positive_generation() {
        assert!(namespace("0123456789abcdef0123456789abcdef"));
        for invalid in [
            "",
            "0123456789ABCDEF0123456789abcdef",
            "../0123456789abcdef0123456789abcdef",
        ] {
            assert!(!namespace(invalid));
        }
        assert_eq!(generation("authority-1").unwrap(), 1);
        assert_eq!(
            generation("authority-18446744073709551615").unwrap(),
            u64::MAX
        );
        for invalid in [
            "authority-0",
            "authority-01",
            "authority-+1",
            "authority-18446744073709551616",
            "other-1",
        ] {
            assert!(generation(invalid).is_err());
        }
    }
}
