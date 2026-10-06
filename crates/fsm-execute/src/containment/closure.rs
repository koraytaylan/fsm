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

#[path = "closure_unlaunched.rs"]
mod unlaunched;

#[path = "closure_prepared.rs"]
mod prepared;

pub(super) fn complete(directory: &Path, allocation: u64) -> Result<(), String> {
    protected_directory(directory)?;
    let _lock = authority_lock(directory)?;
    let domain = closing::recorded_domain(directory, allocation)?;
    if absent(&directory.join(format!("binding-{allocation}.json")))? {
        return prepared::complete(directory, allocation, &domain);
    }
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
    if absent(&directory.join(format!("launch-{allocation}.json")))? {
        return unlaunched::complete(
            directory,
            allocation,
            &domain,
            &binding,
            &claim,
            journal_claim,
        );
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
    let mut records = vec![
        (format!("launch-{allocation}.json"), intent),
        (format!("handoff-{allocation}.json"), handoff),
        (format!("binding-{allocation}.json"), binding.clone()),
    ];
    validate_records(directory, &records)?;
    let stopped_name = format!("manager-stopped-{allocation}.json");
    let matched_stop = !absent(&directory.join(&stopped_name))?;
    if matched_stop {
        records.push((stopped_name, stopped.clone()));
        validate_records(directory, &records)?;
    }
    let tombstone = object([
        ("format", Value::Str("fsm.native-domain-closed/1".into())),
        ("domain", domain.clone()),
    ]);
    let tombstone_path = directory.join(format!("closed-{allocation}.json"));
    if absent(&tombstone_path)? {
        if closing::revoke_after_handoff(directory, allocation)? != domain {
            return Err("closure revocation domain differs".into());
        }
    } else if read_value(&tombstone_path, true)? != tombstone {
        return Err("closure tombstone differs".into());
    }
    records.push((format!("closing-{allocation}.json"), closing));
    validate_records(directory, &records)?;
    sync(&directory.join(format!("closing-{allocation}.json")))?;
    revoked(directory, allocation)?;
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
        if manager::retired(&unit, deadline)? {
            remove_empty(directory, allocation, &domain, &unit, &group, deadline)?;
            if manager::retired(&unit, deadline)? && absent(&group)? {
                break;
            }
        }
        if Instant::now() >= deadline {
            return Err("closure native retirement deadline".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    super::exec_status::retire(directory, allocation)?;
    validate_records(directory, &records)?;
    revoked(directory, allocation)?;
    if closing::recorded_domain(directory, allocation)? != domain || !absent(&group)? {
        return Err("closure native identity changed".into());
    }
    if !matched_stop {
        let mut retired = stopped;
        let Value::Obj(fields) = &mut retired else {
            return Err("retirement material invalid".into());
        };
        fields.insert(
            "format".into(),
            Value::Str("fsm.native-manager-retired/1".into()),
        );
        publish_or_sync(
            &directory.join(format!("manager-retired-{allocation}.json")),
            &retired,
        )?;
    }
    publish_or_sync(&tombstone_path, &tombstone)?;
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

/// Caller holds the authority lock and has already durably revoked entry.
pub(super) fn remove_empty(
    directory: &Path,
    allocation: u64,
    domain: &Value,
    unit: &str,
    group: &Path,
    deadline: Instant,
) -> Result<(), String> {
    if absent(group)? {
        return Ok(());
    }
    let sample = super::observation::read(directory, allocation)?;
    if sample.get("domain") != Some(domain)
        || sample.get("closing") != Some(&Value::Bool(true))
        || sample.get("populated") != Some(&Value::Bool(false))
        || closing::prepared_domain(directory, allocation)? != *domain
        || !manager::retired(unit, deadline)?
    {
        return Err("closure residual native domain is uncertain".into());
    }
    // The kernel refuses a populated group or one with child cgroups; never
    // recursively remove unknown domains or substitute emptiness for absence.
    fs::remove_dir(group).map_err(io)
}

fn validate_records(directory: &Path, records: &[(String, Value)]) -> Result<(), String> {
    for (name, expected) in records {
        if read_value(&directory.join(name), true)? != *expected {
            return Err("closure protected material differs".into());
        }
    }
    Ok(())
}

fn revoked(directory: &Path, allocation: u64) -> Result<(), String> {
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
    let pending = path.with_extension("json.pending");
    if !absent(path)? {
        let metadata = fs::symlink_metadata(path).map_err(io)?;
        if metadata.mode() & 0o222 != 0 || read_value(path, true)? != *material {
            return Err("closure receipt differs or is incomplete".into());
        }
        if !absent(&pending)? {
            let staged = fs::symlink_metadata(&pending).map_err(io)?;
            if !staged.is_file()
                || staged.uid() != 0
                || (staged.dev(), staged.ino()) != (metadata.dev(), metadata.ino())
            {
                return Err("closure pending receipt differs from final publication".into());
            }
            sync(&pending)?;
            fs::remove_file(&pending).map_err(io)?;
        }
        return sync(path);
    }
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
