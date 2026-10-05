//! Privileged immutable entry-grant publication after fresh claim validation.

use super::{closed, entry, io, number, object, protected_directory, read_value, validate_binding};
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, chown};
use std::path::Path;

pub(super) fn publish(directory: &Path, request: &Value) -> Result<(), String> {
    closed(request, &["grant", "group_id"])?;
    let group =
        u32::try_from(number(request, "group_id")?).map_err(|_| "invalid isolated group")?;
    if group == 0 {
        return Err("root group cannot authorize handler entry".into());
    }
    publish_grant(directory, request, Some(group))
}

pub(super) fn publish_enrolled(directory: &Path, request: &Value) -> Result<(), String> {
    closed(request, &["grant"])?;
    publish_grant(directory, request, None)
}

fn publish_grant(directory: &Path, request: &Value, selected: Option<u32>) -> Result<(), String> {
    protected_directory(directory)?;
    let grant = request.get("grant").ok_or("entry grant missing")?;
    let (claim, argv) = entry::decode(grant)?;
    let bytes = canon_bytes(grant);
    if bytes.len() as u64 > super::MAX_RECORD {
        return Err("entry grant exceeds bound".into());
    }
    let allocation = number(&claim.domain().to_value(), "allocation")?;
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    let expected = object([
        ("format", Value::Str("fsm.native-claim-binding/1".into())),
        ("claim", claim.to_value()),
        (
            "journal_claim",
            grant
                .get("journal_claim")
                .ok_or("claim hash missing")?
                .clone(),
        ),
    ]);
    if binding != expected {
        return Err("entry grant differs from protected claim binding".into());
    }
    // Keep the lock through visibility/durability so closing can serialize
    // revocation against this operation; the journal is freshly re-read.
    let (_, _lock) = validate_binding(directory, &binding, Some(&argv))?;
    let group = match selected {
        Some(group) => group,
        None => {
            let handoff_path = directory.join(format!("handoff-{allocation}.json"));
            let handoff = read_value(&handoff_path, true)?;
            closed(&handoff, &["format", "binding", "gate"])?;
            if super::text(&handoff, "format")? != "fsm.native-launch-handoff/1"
                || handoff.get("binding") != Some(&binding)
            {
                return Err("entry handoff differs from protected binding".into());
            }
            let gate = super::enrollment::inspect(
                &claim.domain().to_value(),
                std::time::Instant::now() + std::time::Duration::from_secs(4),
            )?;
            if handoff.get("gate") != Some(&gate.to_value()) {
                return Err("enrolled gate differs from protected handoff".into());
            }
            // Exact replay syncs handoff bytes exposed by a prior failed sync.
            OpenOptions::new()
                .read(true)
                .custom_flags(super::NOFOLLOW_NONBLOCK)
                .open(&handoff_path)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
            File::open(directory).map_err(io)?.sync_all().map_err(io)?;
            gate.group
        }
    };
    publish_file(directory, allocation, group, &bytes)
}

fn publish_file(directory: &Path, allocation: u64, group: u32, bytes: &[u8]) -> Result<(), String> {
    let temporary = directory.join(format!("entry-{allocation}.json.pending"));
    let target = directory.join(format!("entry-{allocation}.json"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    chown(&temporary, Some(0), Some(group)).map_err(io)?;
    file.set_permissions(fs::Permissions::from_mode(0o440))
        .map_err(io)?;
    file.sync_all().map_err(io)?;
    // hard_link is exclusive; rename would overwrite an existing grant.
    fs::hard_link(&temporary, &target).map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)?;
    fs::remove_file(&temporary).map_err(io)?;
    File::open(directory).map_err(io)?.sync_all().map_err(io)
}
