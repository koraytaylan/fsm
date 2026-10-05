//! Matched manager stop after durable entry revocation; never closure proof.

use super::{closed, closing, manager, number, read_value, text};
use fsm_core::record::execution::Claim;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn request(directory: &Path, allocation: u64) -> Result<(), String> {
    let (domain, _lock) = closing::revoke(directory, allocation)?;
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
    let handoff = read_value(&directory.join(format!("handoff-{allocation}.json")), true)?;
    closed(&handoff, &["format", "binding", "gate"])?;
    if text(&handoff, "format")? != "fsm.native-launch-handoff/1"
        || handoff.get("binding") != Some(&binding)
    {
        return Err("manager stop handoff differs from binding".into());
    }
    closed(&binding, &["format", "claim", "journal_claim"])?;
    if text(&binding, "format")? != "fsm.native-claim-binding/1" {
        return Err("manager stop binding format differs".into());
    }
    let claim = Claim::from_value(binding.get("claim").ok_or("manager stop claim missing")?)
        .map_err(|error| error.to_string())?;
    if claim.domain().to_value() != domain {
        return Err("manager stop domain differs from handoff".into());
    }
    let gate = handoff.get("gate").ok_or("manager stop gate missing")?;
    closed(gate, &["pid", "group_id", "invocation_id"])?;
    let pid = number(gate, "pid")?;
    if pid == 0
        || pid > u64::from(u32::MAX)
        || !(61184..=65519).contains(&number(gate, "group_id")?)
    {
        return Err("manager stop gate diagnostics differ".into());
    }
    let invocation = text(gate, "invocation_id")?;
    if invocation.len() != 32
        || invocation.bytes().all(|byte| byte == b'0')
        || !invocation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("manager stop invocation invalid".into());
    }
    let unit = format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    let keys = [
        "InvocationID",
        "ControlGroup",
        "DynamicUser",
        "Delegate",
        "ProtectControlGroups",
        "KillMode",
        "KillSignal",
        "ExitType",
        "Restart",
    ];
    let properties = manager::properties_before(&unit, &keys, deadline)?;
    for (key, expected) in [
        ("InvocationID", invocation),
        ("ControlGroup", &format!("/system.slice/{unit}")),
        ("DynamicUser", "yes"),
        ("Delegate", "no"),
        ("ProtectControlGroups", "yes"),
        ("KillMode", "control-group"),
        ("KillSignal", "9"),
        ("ExitType", "cgroup"),
        ("Restart", "no"),
    ] {
        if properties.get(key).map(String::as_str) != Some(expected) {
            return Err("manager stop policy or invocation differs".into());
        }
    }
    if closing::prepared_domain(directory, allocation)? != domain {
        return Err("manager stop native identity changed".into());
    }
    manager::stop(&unit, deadline)
}
