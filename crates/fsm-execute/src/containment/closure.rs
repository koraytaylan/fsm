//! Matched completed-submission retirement and immutable native receipts.

use super::{
    NOFOLLOW_NONBLOCK, authority_lock, closed, closing, io, manager, number, object,
    protected_directory, publish_once, read_value, text,
};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn complete(directory: &Path, allocation: u64) -> Result<(), String> {
    protected_directory(directory)?;
    let _lock = authority_lock(directory)?;
    let domain = closing::recorded_domain(directory, allocation)?;
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    closed(&binding, &["format", "claim", "journal_claim"])?;
    if text(&binding, "format")? != "fsm.native-claim-binding/1" {
        return Err("closure binding format differs".into());
    }
    let claim = Claim::from_value(binding.get("claim").ok_or("closure claim missing")?)
        .map_err(|error| error.to_string())?;
    if claim.domain().to_value() != domain {
        return Err("closure binding domain differs".into());
    }
    let journal_claim = text(&binding, "journal_claim")?;
    if !journal_claim.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err("closure journal claim hash invalid".into());
    }
    let handoff = read_value(&directory.join(format!("handoff-{allocation}.json")), true)?;
    closed(&handoff, &["format", "binding", "gate"])?;
    if text(&handoff, "format")? != "fsm.native-launch-handoff/1"
        || handoff.get("binding") != Some(&binding)
    {
        return Err("closure handoff differs".into());
    }
    let stopped = object([
        ("format", Value::Str("fsm.native-manager-stopped/1".into())),
        ("domain", domain.clone()),
        ("binding", binding.clone()),
        (
            "gate",
            handoff.get("gate").ok_or("closure gate missing")?.clone(),
        ),
    ]);
    let intent = object([
        ("format", Value::Str("fsm.native-launch-intent/1".into())),
        ("binding", binding.clone()),
    ]);
    let closing = object([
        ("format", Value::Str("fsm.native-closing/1".into())),
        ("domain", domain.clone()),
    ]);
    let records = [
        (format!("manager-stopped-{allocation}.json"), stopped),
        (format!("launch-{allocation}.json"), intent),
        (format!("closing-{allocation}.json"), closing),
        (format!("handoff-{allocation}.json"), handoff),
        (format!("binding-{allocation}.json"), binding.clone()),
    ];
    validate_records(directory, allocation, &records)?;
    let unit = format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    );
    let groups = Path::new("/sys/fs/cgroup/system.slice");
    protected_directory(groups)?;
    let group = groups.join(&unit);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if manager::retired(&unit, deadline)? && absent(&group)? {
            break;
        }
        if Instant::now() >= deadline {
            return Err("closure native retirement deadline".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    validate_records(directory, allocation, &records)?;
    if closing::recorded_domain(directory, allocation)? != domain || !absent(&group)? {
        return Err("closure native identity changed".into());
    }
    let tombstone = object([
        ("format", Value::Str("fsm.native-domain-closed/1".into())),
        ("domain", domain.clone()),
    ]);
    publish_or_sync(
        &directory.join(format!("closed-{allocation}.json")),
        &tombstone,
    )?;
    let receipt = object([
        ("format", Value::Str("fsm.native-closure/1".into())),
        ("domain", domain),
        ("run_id", Value::Num(claim.run_id().to_string())),
        ("journal_claim", Value::Str(journal_claim.into())),
    ]);
    immutable(
        &directory.join(format!("closure-{allocation}-{}.json", claim.run_id())),
        &receipt,
    )
}

fn validate_records(
    directory: &Path,
    allocation: u64,
    records: &[(String, Value)],
) -> Result<(), String> {
    for (name, expected) in records {
        if read_value(&directory.join(name), true)? != *expected {
            return Err("closure protected material differs".into());
        }
    }
    for suffix in ["json", "json.pending"] {
        if !absent(&directory.join(format!("entry-{allocation}.{suffix}")))? {
            return Err("closure entry admission remains".into());
        }
    }
    Ok(())
}

fn absent(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(io(error)),
    }
}

fn sync(path: &Path) -> Result<(), String> {
    OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(path)
        .map_err(io)?
        .sync_all()
        .map_err(io)?;
    File::open(path.parent().ok_or("closure parent missing")?)
        .map_err(io)?
        .sync_all()
        .map_err(io)
}

fn publish_or_sync(path: &Path, material: &Value) -> Result<(), String> {
    if absent(path)? {
        publish_once(path, material)?;
    } else if read_value(path, true)? != *material {
        return Err("closure tombstone differs".into());
    }
    sync(path)
}

fn immutable(path: &Path, material: &Value) -> Result<(), String> {
    let bytes = canon_bytes(material);
    if bytes.len() as u64 > super::MAX_RECORD || parse(&bytes, &JsonLimits::DEFAULT).is_err() {
        return Err("closure receipt exceeds native limits".into());
    }
    if !absent(path)? {
        let metadata = fs::symlink_metadata(path).map_err(io)?;
        if metadata.mode() & 0o222 != 0 || read_value(path, true)? != *material {
            return Err("closure receipt differs or is incomplete".into());
        }
        return sync(path);
    }
    let pending = path.with_extension("json.pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&pending)
        .map_err(io)?;
    file.write_all(&bytes).map_err(io)?;
    file.set_permissions(fs::Permissions::from_mode(0o444))
        .map_err(io)?;
    file.sync_all().map_err(io)?;
    fs::hard_link(&pending, path).map_err(io)?;
    File::open(path.parent().ok_or("closure parent missing")?)
        .map_err(io)?
        .sync_all()
        .map_err(io)?;
    fs::remove_file(pending).map_err(io)?;
    File::open(path.parent().ok_or("closure parent missing")?)
        .map_err(io)?
        .sync_all()
        .map_err(io)
}
