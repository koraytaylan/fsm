//! Matched manager stop after durable entry revocation; never closure proof.

use super::{closed, closing, io, manager, number, object, publish_once, read_value, text};
use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::Claim;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

/// Attempt original-domain fencing while preserving any manager stop refusal;
/// neither a kernel kill submission nor this return value establishes closure.
pub(super) fn fence(directory: &Path, allocation: u64) -> Result<(), String> {
    let result = request(directory, allocation);
    if result.is_err() {
        let _ = super::termination::request(directory, allocation);
    }
    result
}

pub(super) fn request(directory: &Path, allocation: u64) -> Result<(), String> {
    super::protected_directory(directory).map_err(|error| stage("authority directory", error))?;
    let _lock = super::authority_lock(directory).map_err(|error| stage("authority lock", error))?;
    let domain = closing::recorded_domain(directory, allocation)
        .map_err(|error| stage("recorded domain", error))?;
    // A live original resource can always have admission revoked, even when
    // damaged handoff material prevents a matched manager operation or proof.
    let group_path = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    ));
    match fs::symlink_metadata(&group_path) {
        Ok(_) => {
            match closing::revoke_locked(directory, allocation) {
                Ok(observed) if observed == domain => {}
                Ok(_) => return Err("manager stop live native identity changed".into()),
                Err(error) => {
                    // The original group can retire between the metadata read
                    // and live revocation. Absence permits only continuation
                    // into the complete original-handoff checks below, never
                    // successful fencing or closure on its own. A surviving,
                    // replaced or unreadable group retains the original error.
                    match fs::symlink_metadata(&group_path) {
                        Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => {}
                        _ => return Err(stage("live domain revocation", error)),
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let completed = directory.join(format!("manager-stopped-{allocation}.json"));
    match fs::symlink_metadata(&completed) {
        Ok(_) => return Err("manager stop completion already exists or is uncertain".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let binding = read_value(&directory.join(format!("binding-{allocation}.json")), true)
        .map_err(|error| stage("original binding", error))?;
    let handoff = read_value(&directory.join(format!("handoff-{allocation}.json")), true)
        .map_err(|error| stage("original handoff", error))?;
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
    let material = object([
        ("format", Value::Str("fsm.native-manager-stopped/1".into())),
        ("domain", domain.clone()),
        ("binding", binding.clone()),
        ("gate", gate.clone()),
    ]);
    let bytes = canon_bytes(&material);
    if bytes.len() as u64 > super::MAX_RECORD {
        return Err("manager stop completion exceeds native byte bound".into());
    }
    parse(&bytes, &JsonLimits::DEFAULT)
        .map_err(|_| "manager stop completion exceeds native depth bound")?;
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
        "RemainAfterExit",
        "CollectMode",
    ];
    let properties = manager::properties_before(&unit, &keys, deadline)
        .map_err(|error| stage("original manager properties", error))?;
    for (key, expected) in [
        ("InvocationID", invocation),
        ("DynamicUser", "yes"),
        ("Delegate", "no"),
        ("ProtectControlGroups", "yes"),
        ("KillMode", "control-group"),
        ("KillSignal", "9"),
        ("ExitType", "cgroup"),
        ("Restart", "no"),
        ("RemainAfterExit", "yes"),
        ("CollectMode", "inactive"),
    ] {
        if properties.get(key).map(String::as_str) != Some(expected) {
            return Err("manager stop policy or invocation differs".into());
        }
    }
    let group = format!("/system.slice/{unit}");
    let observed = properties.get("ControlGroup").map(String::as_str);
    if observed != Some(group.as_str()) && observed != Some("") {
        return Err("manager stop control group differs".into());
    }
    if observed == Some("") {
        match fs::symlink_metadata(format!("/sys/fs/cgroup{group}")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("manager stop empty control group is uncertain".into()),
        }
    }
    if closing::revoke_after_handoff(directory, allocation)
        .map_err(|error| stage("handoff revocation", error))?
        != domain
    {
        return Err("manager stop native identity changed".into());
    }
    manager::stop(&unit, deadline).map_err(|error| stage("matched manager stop", error))?;
    manager::reset_original_failed(&unit, pid, invocation, deadline)
        .map_err(|error| stage("original failed-unit retirement", error))?;
    publish_once(&completed, &material).map_err(|error| stage("stop completion publication", error))
}

fn stage(operation: &str, error: String) -> String {
    format!("manager stop {operation}: {error}")
}
