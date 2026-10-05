//! Protected entry: authorization and actual enrollment precede user code.

use super::{authority_path, closed, identity, io, number, protected_directory, read_value, text};
use fsm_core::json::Value;
use fsm_core::record::execution::Claim;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

pub(super) fn run(arguments: &[OsString]) -> Result<(), String> {
    if fs::metadata("/proc/self").map_err(io)?.uid() == 0 {
        return Err("entry requires an isolated unprivileged handler identity".into());
    }
    if arguments.len() != 3 {
        return Err("gate requires namespace, generation and allocation".into());
    }
    let namespace = arguments[0].to_str().ok_or("invalid namespace")?;
    let generation = arguments[1].to_str().ok_or("invalid generation")?;
    let allocation_text = arguments[2].to_str().ok_or("invalid allocation")?;
    let allocation = allocation_text
        .parse::<u64>()
        .map_err(|_| "invalid allocation")?;
    if allocation == 0 || allocation_text != allocation.to_string() {
        return Err("noncanonical entry allocation".into());
    }
    let directory = authority_path(namespace, generation)?;
    protected_directory(&directory)?;
    let grant_path = directory.join(format!("entry-{allocation}.json"));
    let grant = read_value(&grant_path, true)?;
    let (claim, argv) = decode(&grant)?;
    let domain = claim.domain().to_value();
    if text(&domain, "namespace")? != namespace
        || number(&domain, "generation")?.to_string() != generation
        || number(&domain, "allocation")? != allocation
        || domain.get("authority")
            != Some(&identity(&fs::symlink_metadata(&directory).map_err(io)?))
    {
        return Err("entry authority or route differs".into());
    }
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(io)?;
    let unit = format!("fsm-containment-{namespace}-{generation}-{allocation}.service");
    let group = Path::new("/sys/fs/cgroup/system.slice").join(&unit);
    let membership = fs::read_to_string("/proc/self/cgroup").map_err(io)?;
    let observed = fs::symlink_metadata(group).map_err(io)?;
    if boot.trim() != text(&domain, "boot")?
        || membership != format!("0::/system.slice/{unit}\n")
        || !observed.is_dir()
        || observed.uid() != 0
        || observed.mode() & 0o022 != 0
        || domain.get("cgroup") != Some(&identity(&observed))
    {
        return Err("entry is not enrolled in its authorized native domain".into());
    }
    // Re-read the pathname before exec so revocation/replacement observed
    // here refuses; closing must also kill/fence this already enrolled gate
    // to cover the final validation-to-exec race.
    if read_value(&grant_path, true)? != grant {
        return Err("entry grant changed before execution".into());
    }
    let error = Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(format!("authorized handler exec failed: {error}"))
}

pub(super) fn decode(grant: &Value) -> Result<(Claim, Vec<String>), String> {
    closed(grant, &["format", "claim", "journal_claim", "argv"])?;
    if text(grant, "format")? != "fsm.native-entry/1" {
        return Err("unknown entry format".into());
    }
    let original = text(grant, "journal_claim")?
        .strip_prefix("sha256:")
        .ok_or("invalid original claim hash")?;
    if original.len() != 64
        || !original
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("invalid original claim hash".into());
    }
    let claim = Claim::from_value(grant.get("claim").ok_or("claim missing")?)
        .map_err(|error| error.to_string())?;
    let values = grant
        .get("argv")
        .and_then(Value::as_arr)
        .ok_or("entry argv is not an array")?;
    if values.is_empty() || values.len() > 256 {
        return Err("entry argv count exceeds bound".into());
    }
    let argv = values
        .iter()
        .map(|value| {
            let argument = value.as_str().ok_or("entry argument is not a string")?;
            if argument.contains('\0') {
                return Err("entry argument contains NUL".into());
            }
            Ok(argument.to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    if !Path::new(&argv[0]).is_absolute() {
        return Err("entry command is not absolute".into());
    }
    Ok((claim, argv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::json::{JsonLimits, parse};
    use fsm_core::record::execution::{FileIdentity, NativeDomain, RetryPolicy};
    use std::collections::BTreeMap;

    fn grant() -> Value {
        let domain = NativeDomain::new(
            "a".repeat(32),
            1,
            "11111111-1111-1111-1111-111111111111".into(),
            FileIdentity::new(1, 2).unwrap(),
            FileIdentity::new(1, 3).unwrap(),
            1,
        )
        .unwrap();
        let claim = super::super::object([
            ("run_id", Value::Num("1".into())),
            ("instance_id", Value::Str("instance".into())),
            ("effect_id", Value::Str("effect".into())),
            ("attempt", Value::Num("1".into())),
            (
                "handler_fingerprint",
                Value::Str(format!("sha256:{}", "a".repeat(64))),
            ),
            (
                "retry",
                RetryPolicy::new(1, 10, 10, Vec::new()).unwrap().to_value(),
            ),
            ("domain", domain.to_value()),
        ]);
        super::super::object([
            ("format", Value::Str("fsm.native-entry/1".into())),
            ("claim", claim),
            (
                "journal_claim",
                Value::Str(format!("sha256:{}", "b".repeat(64))),
            ),
            ("argv", Value::Arr(vec![Value::Str("/bin/true".into())])),
        ])
    }

    #[test]
    fn grant_shape_accepts_absolute_command_and_refuses_command_and_hash_aliases() {
        let valid = grant();
        assert_eq!(decode(&valid).unwrap().1, ["/bin/true"]);
        // Shape-only values authenticate no root publisher or enrollment.
        for (field, value) in [
            ("argv", Value::Arr(Vec::new())),
            ("argv", Value::Arr(vec![Value::Str("true".into())])),
            ("argv", Value::Arr(vec![Value::Str("/bin/true\0".into())])),
            ("argv", Value::Arr(vec![Value::Null])),
            (
                "argv",
                Value::Arr(vec![Value::Str("/bin/true".into()); 257]),
            ),
            (
                "journal_claim",
                Value::Str(format!("sha256:{}", "B".repeat(64))),
            ),
            ("journal_claim", Value::Str("sha256:abc".into())),
        ] {
            let mut changed = valid.clone();
            if let Value::Obj(fields) = &mut changed {
                fields.insert(field.into(), value);
            }
            assert!(decode(&changed).is_err());
        }
        let mut fields = valid.as_obj().unwrap().clone();
        fields.insert("unexpected".into(), Value::Obj(BTreeMap::new()));
        assert!(decode(&Value::Obj(fields)).is_err());
    }

    #[test]
    fn entry_grants_refuse_unknown_fields_and_missing_authorization() {
        for source in [
            b"null".as_slice(),
            b"{}",
            b"{\"format\":\"fsm.native-entry/1\",\"argv\":[\"/bin/true\"]}",
        ] {
            assert!(decode(&parse(source, &JsonLimits::DEFAULT).unwrap()).is_err());
        }
    }
}
