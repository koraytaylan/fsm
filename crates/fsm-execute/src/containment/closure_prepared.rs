//! Retire an unbound prepared allocation without issuing execution evidence.

use super::*;

/// The caller retains the authority lock; absent binding is never a receipt.
pub(super) fn complete(
    directory: &Path,
    allocation: u64,
    domain: &Value,
    deadline: Instant,
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
    if absent(&group)? {
        if read_value(&closing_path, true)? != closing {
            return Err("prepared absent domain lacks durable revocation".into());
        }
        revoked(directory, allocation)?;
    } else if closing::revoke_locked(directory, allocation)? != *domain {
        return Err("prepared revocation domain differs".into());
    }
    sync(&closing_path)?;
    if !manager::retired(&unit, deadline)? {
        return Err("prepared manager still owns unit or job".into());
    }
    remove_empty(directory, allocation, domain, &unit, &group, deadline)?;
    if !manager::retired(&unit, deadline)? || !absent(&group)? {
        return Err("prepared native retirement differs".into());
    }
    no_submission(directory, allocation)?;
    revoked(directory, allocation)?;
    if closing::recorded_domain(directory, allocation)? != *domain
        || read_value(&closing_path, true)? != closing
    {
        return Err("prepared recorded identity changed".into());
    }
    publish_or_sync(
        &directory.join(format!("closed-{allocation}.json")),
        &object([
            ("format", Value::Str("fsm.native-domain-closed/1".into())),
            ("domain", domain.clone()),
        ]),
    )
}

fn no_submission(directory: &Path, allocation: u64) -> Result<(), String> {
    for prefix in [
        "binding",
        "launch",
        "handoff",
        "manager-stopped",
        "manager-retired",
        "exec-status",
    ] {
        for suffix in ["json", "json.pending"] {
            if !absent(&directory.join(format!("{prefix}-{allocation}.{suffix}")))? {
                return Err("prepared cleanup carries binding or submission material".into());
            }
        }
    }
    Ok(())
}
