//! Opaque store proofs from protected native receipts, never PID observations.

use std::path::Path;

use fsm_core::hashes::domain_hash;
use fsm_core::json::Value;
use fsm_core::record::execution::{Claim, Closure, NativeDomain};
use fsm_core::sha256::to_hex;

use super::ErrorObj;

/// A protected root-issued receipt binding closure to one durable claim record.
#[derive(Debug, Clone)]
pub struct VerifiedClosure {
    pub(super) closure: Closure,
    pub(super) journal_claim: String,
}

impl VerifiedClosure {
    /// Compare authenticated receipt identity without asserting current ownership.
    ///
    /// The stopped transition still rechecks ownership under the writer lease;
    /// this predicate alone authorizes no settlement, retry or capacity release.
    pub fn matches_claim(&self, claim: &Claim, journal_claim: &str) -> bool {
        let closure = self.closure.to_value();
        self.journal_claim == journal_claim
            && closure.get("run_id") == Some(&Value::Num(claim.run_id().to_string()))
            && closure.get("domain") == Some(&claim.domain().to_value())
    }

    /// Read bounded protected native evidence; unsupported platforms refuse.
    pub fn read(path: &Path) -> Result<Self, ErrorObj> {
        let material = read_protected(path)?;
        closed(&material, &["format", "domain", "run_id", "journal_claim"])?;
        if string(&material, "format")? != "fsm.native-closure/1" {
            return Err(invalid("unknown closure receipt format"));
        }
        let journal_claim = string(&material, "journal_claim")?;
        if !digest(journal_claim) {
            return Err(invalid("invalid claim record hash"));
        }
        let run_id = material
            .get("run_id")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse::<u64>().ok())
            .ok_or_else(|| invalid("invalid receipt run_id"))?;
        let domain = NativeDomain::from_value(
            material
                .get("domain")
                .ok_or_else(|| invalid("missing receipt domain"))?,
        )
        .map_err(|error| invalid(error.to_string()))?;
        let receipt = format!(
            "sha256:{}",
            to_hex(&domain_hash("fsm:native-closure:1", &material))
        );
        Ok(Self {
            closure: Closure::new(run_id, domain, receipt)
                .map_err(|error| invalid(error.to_string()))?,
            journal_claim: journal_claim.into(),
        })
    }
}

/// Protected legacy environment closure bound to the exact admission prefix.
#[derive(Debug, Clone)]
pub struct VerifiedQuiescence {
    pub(super) value: Value,
    pub(super) previous_head: String,
}

impl VerifiedQuiescence {
    /// Read a native-issued legacy witness; an operator assertion cannot produce one.
    pub fn read(path: &Path) -> Result<Self, ErrorObj> {
        let material = read_protected(path)?;
        closed(&material, &["format", "domain", "previous_head"])?;
        if string(&material, "format")? != "fsm.native-quiescence/1" {
            return Err(invalid("unknown quiescence receipt format"));
        }
        let previous_head = string(&material, "previous_head")?;
        if !digest(previous_head) {
            return Err(invalid("invalid legacy journal head"));
        }
        let domain = NativeDomain::from_value(
            material
                .get("domain")
                .ok_or_else(|| invalid("missing receipt domain"))?,
        )
        .map_err(|error| invalid(error.to_string()))?;
        let receipt = format!(
            "sha256:{}",
            to_hex(&domain_hash("fsm:native-quiescence:1", &material))
        );
        let value = Value::Obj(std::collections::BTreeMap::from([
            ("domain".into(), domain.to_value()),
            ("receipt".into(), Value::Str(receipt)),
            ("previous_head".into(), Value::Str(previous_head.into())),
        ]));
        Ok(Self {
            value,
            previous_head: previous_head.into(),
        })
    }
}

fn invalid(message: impl Into<String>) -> ErrorObj {
    ErrorObj::new("store/execution_evidence", message).hint("use the provisioned native authority's protected matching closure receipt; preserve unresolved ownership when evidence is missing")
}

fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|suffix| {
        suffix.len() == 64
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, ErrorObj> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("missing receipt {field}")))
}

fn closed(value: &Value, fields: &[&str]) -> Result<(), ErrorObj> {
    let object = value
        .as_obj()
        .ok_or_else(|| invalid("receipt is not an object"))?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(invalid("receipt fields are not closed"));
    }
    Ok(())
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn read_protected(_path: &Path) -> Result<Value, ErrorObj> {
    Err(invalid(
        "native evidence requires the provisioned Linux/systemd backend",
    ))
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn read_protected(path: &Path) -> Result<Value, ErrorObj> {
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    const MAX_RECEIPT: usize = 8 * 1024;
    // Linux x86-64/AArch64 UAPI flags: bounded nonblocking open, no symlink follow.
    const NONBLOCK: i32 = 0o4000;
    const NOFOLLOW: i32 = 0o400000;
    if path.as_os_str().len() > 4096
        || !path.starts_with("/var/lib/fsm-containment")
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(invalid(
            "receipt path is outside the protected native authority",
        ));
    }
    validate_ancestors(
        path.parent()
            .ok_or_else(|| invalid("missing authority directory"))?,
    )?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(NONBLOCK | NOFOLLOW)
        .open(path)
        .map_err(|error| invalid(error.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|error| invalid(error.to_string()))?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o222 != 0
        || metadata.len() > MAX_RECEIPT as u64
    {
        return Err(invalid(
            "receipt must be a bounded root-owned non-writable regular file",
        ));
    }
    let mut bytes = Vec::with_capacity(MAX_RECEIPT + 1);
    file.by_ref()
        .take((MAX_RECEIPT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid(error.to_string()))?;
    if bytes.len() > MAX_RECEIPT {
        return Err(invalid("receipt exceeds native byte bound"));
    }
    let after = file
        .metadata()
        .map_err(|error| invalid(error.to_string()))?;
    if after.uid() != 0
        || after.mode() & 0o222 != 0
        || after.len() != metadata.len()
        || after.dev() != metadata.dev()
        || after.ino() != metadata.ino()
    {
        return Err(invalid("protected receipt changed during read"));
    }
    let content = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
    let value = fsm_core::json::parse(
        content,
        &fsm_core::json::JsonLimits {
            max_depth: 64,
            max_bytes: MAX_RECEIPT,
        },
    )
    .map_err(|error| invalid(error.message))?;
    if fsm_core::canon::canon_bytes(&value) != content {
        return Err(invalid("native receipt is not canonical"));
    }
    validate_location(path, &value)?;
    Ok(value)
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn validate_location(path: &Path, value: &Value) -> Result<(), ErrorObj> {
    use std::os::unix::fs::MetadataExt;
    let domain = NativeDomain::from_value(
        value
            .get("domain")
            .ok_or_else(|| invalid("missing native domain"))?,
    )
    .map_err(|error| invalid(error.to_string()))?
    .to_value();
    let namespace = string(&domain, "namespace")?;
    let generation = string_number(&domain, "generation")?;
    let allocation = string_number(&domain, "allocation")?;
    let filename = match string(value, "format")? {
        "fsm.native-closure/1" => format!(
            "closure-{allocation}-{}.json",
            string_number(value, "run_id")?
        ),
        "fsm.native-quiescence/1" => {
            let head = string(value, "previous_head")?;
            if !digest(head) {
                return Err(invalid("invalid legacy prefix"));
            }
            format!("quiescence-{allocation}-{}.json", &head[7..])
        }
        _ => return Err(invalid("unknown native receipt format")),
    };
    let authority_path = Path::new("/var/lib/fsm-containment")
        .join(namespace)
        .join(format!("authority-{generation}"));
    if path != authority_path.join(filename) {
        return Err(invalid("receipt path disagrees with its native identity"));
    }
    validate_ancestors(&authority_path)?;
    let authority = domain
        .get("authority")
        .ok_or_else(|| invalid("missing authority identity"))?;
    let metadata =
        std::fs::symlink_metadata(&authority_path).map_err(|error| invalid(error.to_string()))?;
    if string_number(authority, "device")? != metadata.dev()
        || string_number(authority, "inode")? != metadata.ino()
    {
        return Err(invalid("native authority directory identity mismatch"));
    }
    Ok(())
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn validate_ancestors(directory: &Path) -> Result<(), ErrorObj> {
    use std::os::unix::fs::MetadataExt;
    for ancestor in directory.ancestors() {
        let metadata =
            std::fs::symlink_metadata(ancestor).map_err(|error| invalid(error.to_string()))?;
        if !metadata.is_dir() || metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(invalid("native authority ancestor is not protected"));
        }
    }
    Ok(())
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn string_number(value: &Value, field: &str) -> Result<u64, ErrorObj> {
    value
        .get(field)
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .ok_or_else(|| invalid(format!("invalid native {field}")))
}
