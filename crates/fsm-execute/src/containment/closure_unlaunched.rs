//! Closure of a bound allocation with no durable launch submission.

use super::*;

pub(super) fn complete(
    directory: &Path,
    allocation: u64,
    domain: &Value,
    binding: &Value,
    claim: &Claim,
    journal_claim: &str,
) -> Result<(), String> {
    no_submission(directory, allocation)?;
    let unit = format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(domain, "namespace")?,
        number(domain, "generation")?
    );
    let groups = Path::new("/sys/fs/cgroup/system.slice");
    protected_directory(groups)?;
    let group = groups.join(&unit);
    let closing = object([
        ("format", Value::Str("fsm.native-closing/1".into())),
        ("domain", domain.clone()),
    ]);
    let closing_path = directory.join(format!("closing-{allocation}.json"));
    let tombstone = object([
        ("format", Value::Str("fsm.native-domain-closed/1".into())),
        ("domain", domain.clone()),
    ]);
    let tombstone_path = directory.join(format!("closed-{allocation}.json"));
    if absent(&group)? {
        if read_value(&closing_path, true)? != closing {
            return Err("unlaunched absent domain lacks durable revocation".into());
        }
        revoked(directory, allocation)?;
    } else if closing::revoke_locked(directory, allocation)? != *domain {
        return Err("unlaunched revocation domain differs".into());
    }
    sync(&closing_path)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    if !manager::retired(&unit, deadline)? {
        return Err("unlaunched manager still owns unit or job".into());
    }
    if !absent(&group)? {
        let observed = fs::symlink_metadata(&group).map_err(io)?;
        if !observed.is_dir()
            || observed.uid() != 0
            || observed.mode() & 0o022 != 0
            || domain.get("cgroup") != Some(&super::super::identity(&observed))
            || !fs::read_to_string(group.join("cgroup.events"))
                .map_err(io)?
                .lines()
                .any(|line| line == "populated 0")
        {
            return Err("unlaunched domain identity or emptiness differs".into());
        }
        fs::remove_dir(&group).map_err(io)?;
    }
    if !manager::retired(&unit, deadline)? || !absent(&group)? {
        return Err("unlaunched native retirement differs".into());
    }
    super::super::exec_status::retire(directory, allocation)?;
    no_submission(directory, allocation)?;
    validate_records(
        directory,
        &[
            (format!("binding-{allocation}.json"), binding.clone()),
            (format!("closing-{allocation}.json"), closing),
        ],
    )?;
    revoked(directory, allocation)?;
    if closing::recorded_domain(directory, allocation)? != *domain {
        return Err("unlaunched recorded identity changed".into());
    }
    publish_or_sync(&tombstone_path, &tombstone)?;
    immutable(
        &directory.join(format!("closure-{allocation}-{}.json", claim.run_id())),
        &object([
            ("format", Value::Str("fsm.native-closure/1".into())),
            ("domain", domain.clone()),
            ("run_id", Value::Num(claim.run_id().to_string())),
            ("journal_claim", Value::Str(journal_claim.into())),
        ]),
    )
}

fn no_submission(directory: &Path, allocation: u64) -> Result<(), String> {
    for prefix in ["launch", "handoff", "manager-stopped", "manager-retired"] {
        for suffix in ["json", "json.pending"] {
            if !absent(&directory.join(format!("{prefix}-{allocation}.{suffix}")))? {
                return Err("unlaunched closure carries submission material".into());
            }
        }
    }
    Ok(())
}
