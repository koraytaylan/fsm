//! Derive entry-grant access from the actual installed, enrolled gate.

use super::{identity, io, manager, number, protected_directory, text};
use fsm_core::json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

pub(super) const EXECUTABLE: &str = "/usr/libexec/fsm-containment-authority";
const KEYS: &[&str] = &[
    "ActiveState",
    "SubState",
    "MainPID",
    "DynamicUser",
    "ControlGroup",
    "Delegate",
    "ProtectControlGroups",
    "Restart",
    "InvocationID",
    "ExitType",
    "KillMode",
    "KillSignal",
];

#[cfg(test)]
pub(super) fn group(domain: &Value) -> Result<u32, String> {
    inspect(domain, Instant::now() + Duration::from_secs(4)).map(|gate| gate.group)
}

pub(super) struct GateIdentity {
    pub(super) pid: u32,
    pub(super) group: u32,
    pub(super) invocation: String,
}

impl GateIdentity {
    pub(super) fn to_value(&self) -> Value {
        super::object([
            ("pid", Value::Num(self.pid.to_string())),
            ("group_id", Value::Num(self.group.to_string())),
            ("invocation_id", Value::Str(self.invocation.clone())),
        ])
    }
}

pub(super) fn inspect(domain: &Value, deadline: Instant) -> Result<GateIdentity, String> {
    let namespace = text(domain, "namespace")?;
    let generation = number(domain, "generation")?.to_string();
    let allocation = number(domain, "allocation")?.to_string();
    let unit = format!("fsm-containment-{namespace}-{generation}-{allocation}.service");
    let membership = format!("0::/system.slice/{unit}\n");
    let properties = manager::properties_before(&unit, KEYS, deadline)?;
    let pid = main_pid(&properties, &unit)?;
    let process = PathBuf::from(format!("/proc/{pid}"));
    let executable = Path::new(EXECUTABLE);
    let installed = installed()?;
    let before = fs::symlink_metadata(&process).map_err(io)?;
    let expected = [EXECUTABLE, "gate", namespace, &generation, &allocation].join("\0") + "\0";
    let observe = || -> Result<(u32, u32), String> {
        if bounded(&process.join("cgroup"))? != membership.as_bytes()
            || bounded(&process.join("cmdline"))? != expected.as_bytes()
            || identity(&fs::metadata(process.join("exe")).map_err(io)?) != identity(&installed)
        {
            return Err("manager main process is not the routed installed gate".into());
        }
        credentials(&bounded(&process.join("status"))?)
    };
    let credentials = observe()?;
    if before.uid() != credentials.0
        || manager::properties_before(&unit, KEYS, deadline)? != properties
        || observe()? != credentials
    {
        return Err("enrolled gate changed during verification".into());
    }
    let after = fs::symlink_metadata(&process).map_err(io)?;
    let installed_after = fs::symlink_metadata(executable).map_err(io)?;
    let group = Path::new("/sys/fs/cgroup/system.slice").join(unit);
    let observed_group = fs::symlink_metadata(group).map_err(io)?;
    if identity(&after) != identity(&before)
        || after.uid() != before.uid()
        || identity(&installed_after) != identity(&installed)
        || installed_after.uid() != installed.uid()
        || installed_after.mode() != installed.mode()
        || !observed_group.is_dir()
        || observed_group.uid() != 0
        || observed_group.mode() & 0o022 != 0
        || domain.get("cgroup") != Some(&identity(&observed_group))
        || Instant::now() >= deadline
    {
        return Err("enrolled gate or native identity changed".into());
    }
    Ok(GateIdentity {
        pid,
        group: credentials.1,
        invocation: properties["InvocationID"].clone(),
    })
}

pub(super) fn installed() -> Result<fs::Metadata, String> {
    let executable = Path::new(EXECUTABLE);
    protected_directory(executable.parent().ok_or("gate executable has no parent")?)?;
    let metadata = fs::symlink_metadata(executable).map_err(io)?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o111 == 0
    {
        return Err("installed gate executable is not root protected".into());
    }
    Ok(metadata)
}

fn bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(super::NOFOLLOW_NONBLOCK)
        .open(path)
        .map_err(io)?;
    if !file.metadata().map_err(io)?.is_file() {
        return Err("gate observation is not a regular proc file".into());
    }
    let mut bytes = Vec::new();
    file.take(8193).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() > 8192 {
        return Err("gate observation exceeds bound".into());
    }
    Ok(bytes)
}

fn main_pid(properties: &BTreeMap<String, String>, unit: &str) -> Result<u32, String> {
    for (key, expected) in [
        ("ActiveState", "active"),
        ("SubState", "running"),
        ("DynamicUser", "yes"),
        ("Delegate", "no"),
        ("ProtectControlGroups", "yes"),
        ("Restart", "no"),
        ("ExitType", "cgroup"),
        ("KillMode", "control-group"),
        ("KillSignal", "9"),
        ("ControlGroup", &format!("/system.slice/{unit}")),
    ] {
        if properties.get(key).map(String::as_str) != Some(expected) {
            return Err("manager gate policy or membership differs".into());
        }
    }
    let invocation = properties
        .get("InvocationID")
        .ok_or("gate invocation missing")?;
    if invocation.len() != 32
        || invocation.bytes().all(|byte| byte == b'0')
        || !invocation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid gate invocation identity".into());
    }
    let raw = properties.get("MainPID").ok_or("gate main pid missing")?;
    let pid = raw.parse::<u32>().map_err(|_| "invalid gate main pid")?;
    if pid == 0 || raw != &pid.to_string() {
        return Err("noncanonical gate main pid".into());
    }
    Ok(pid)
}

fn credentials(bytes: &[u8]) -> Result<(u32, u32), String> {
    let source = std::str::from_utf8(bytes).map_err(|_| "invalid gate status encoding")?;
    let keys = [
        "Uid",
        "Gid",
        "Groups",
        "CapInh",
        "CapPrm",
        "CapEff",
        "CapBnd",
        "CapAmb",
        "NoNewPrivs",
    ];
    let mut fields = BTreeMap::new();
    for line in source.lines() {
        let (key, value) = line.split_once(':').ok_or("invalid gate status field")?;
        if keys.contains(&key) && fields.insert(key, value.trim()).is_some() {
            return Err("duplicate gate security field".into());
        }
    }
    if fields.len() != keys.len() || fields.get("NoNewPrivs") != Some(&"1") {
        return Err("gate security fields missing or privilege escalation allowed".into());
    }
    for key in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
        let value = fields[key];
        if value.len() != 16 || value.bytes().any(|byte| byte != b'0') {
            return Err("enrolled gate retains capabilities".into());
        }
    }
    let uid = credential_ids(fields["Uid"])?;
    let gid = credential_ids(fields["Gid"])?;
    if uid != gid
        || fields["Groups"]
            .split_whitespace()
            .any(|value| value != gid.to_string())
    {
        return Err("gate identity pair or supplementary groups differ".into());
    }
    Ok((uid, gid))
}

fn credential_ids(source: &str) -> Result<u32, String> {
    let mut ids = source.split_whitespace();
    let raw = ids.next().ok_or("gate credential missing")?;
    let id = raw.parse::<u32>().map_err(|_| "invalid gate credential")?;
    if !(61184..=65519).contains(&id)
        || raw != id.to_string()
        || ids.clone().count() != 3
        || ids.any(|value| value != raw)
    {
        return Err("gate credentials are not one isolated dynamic identity".into());
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> String {
        "Uid:\t61184\t61184\t61184\t61184\nGid:\t61184\t61184\t61184\t61184\nGroups:\t\nCapInh:\t0000000000000000\nCapPrm:\t0000000000000000\nCapEff:\t0000000000000000\nCapBnd:\t0000000000000000\nCapAmb:\t0000000000000000\nNoNewPrivs:\t1\n".into()
    }

    #[test]
    fn gate_credentials_require_all_isolation_fields() {
        let valid = status();
        assert_eq!(credentials(valid.as_bytes()).unwrap(), (61184, 61184));
        for changed in [
            valid.replace("61184", "0"),
            valid.replace("61184", "061184"),
            valid.replace("61184", "65520"),
            valid.replace("Groups:\t\n", "Groups:\t0\n"),
            valid.replace("NoNewPrivs:\t1", "NoNewPrivs:\t0"),
            valid.replace("CapBnd:\t0000000000000000", "CapBnd:\t0000000000000001"),
            valid.replace("Gid:\t61184", "Gid:\t61185"),
            valid.replace("CapAmb:\t0000000000000000\n", ""),
            valid.clone() + "Uid:\t61184\t61184\t61184\t61184\n",
        ] {
            assert!(credentials(changed.as_bytes()).is_err());
        }
        assert!(credentials(b"\xff").is_err());
    }

    #[test]
    fn manager_gate_policy_requires_live_pid_and_invocation() {
        let unit = "fsm-containment-test.service";
        let valid = BTreeMap::from([
            ("ActiveState".into(), "active".into()),
            ("SubState".into(), "running".into()),
            ("DynamicUser".into(), "yes".into()),
            ("Delegate".into(), "no".into()),
            ("ProtectControlGroups".into(), "yes".into()),
            ("Restart".into(), "no".into()),
            ("ExitType".into(), "cgroup".into()),
            ("KillMode".into(), "control-group".into()),
            ("KillSignal".into(), "9".into()),
            ("ControlGroup".into(), format!("/system.slice/{unit}")),
            ("MainPID".into(), "123".into()),
            ("InvocationID".into(), "a".repeat(32)),
        ]);
        assert_eq!(main_pid(&valid, unit).unwrap(), 123);
        for (key, value) in [
            ("ActiveState", "inactive"),
            ("SubState", "exited"),
            ("DynamicUser", "no"),
            ("Delegate", "yes"),
            ("ProtectControlGroups", "no"),
            ("Restart", "always"),
            ("ExitType", "main"),
            ("KillMode", "process"),
            ("KillSignal", "15"),
            ("ControlGroup", "/other"),
            ("MainPID", "0"),
            ("MainPID", "0123"),
            ("MainPID", "4294967296"),
            ("InvocationID", "00000000000000000000000000000000"),
            ("InvocationID", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        ] {
            let mut changed = valid.clone();
            changed.insert(key.into(), value.into());
            assert!(main_pid(&changed, unit).is_err());
        }
        for key in KEYS {
            let mut changed = valid.clone();
            changed.remove(*key);
            assert!(main_pid(&changed, unit).is_err());
        }
    }
}
