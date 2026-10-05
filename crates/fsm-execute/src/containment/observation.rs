//! Read-only bounded native progress; population is never a closure proof.

use super::{NOFOLLOW_NONBLOCK, closing, io, number, object, read_value, text};
use fsm_core::json::Value;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub(super) fn read(directory: &Path, allocation: u64) -> Result<Value, String> {
    let domain = closing::prepared_domain(directory, allocation)?;
    let before = phase(directory, allocation, &domain)?;
    let group = Path::new("/sys/fs/cgroup/system.slice").join(format!(
        "fsm-containment-{}-{}-{allocation}.service",
        text(&domain, "namespace")?,
        number(&domain, "generation")?
    ));
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW_NONBLOCK)
        .open(group.join("cgroup.events"))
        .map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || metadata.dev()
            != number(
                domain.get("cgroup").ok_or("cgroup identity missing")?,
                "device",
            )?
    {
        return Err("native observation control is not protected".into());
    }
    let mut bytes = Vec::with_capacity(4097);
    file.take(4097).read_to_end(&mut bytes).map_err(io)?;
    if bytes.len() > 4096 {
        return Err("native observation exceeds bound".into());
    }
    let (populated, frozen) = events(&bytes)?;
    if closing::prepared_domain(directory, allocation)? != domain
        || phase(directory, allocation, &domain)? != before
    {
        return Err("native observation identity or phase changed".into());
    }
    Ok(object([
        ("format", Value::Str("fsm.native-observation/1".into())),
        ("domain", domain),
        ("closing", Value::Bool(before)),
        ("populated", Value::Bool(populated)),
        ("frozen", Value::Bool(frozen)),
    ]))
}

fn phase(directory: &Path, allocation: u64, domain: &Value) -> Result<bool, String> {
    match fs::symlink_metadata(directory.join(format!("closed-{allocation}.json"))) {
        Ok(_) => return Err("native observation live domain carries closed evidence".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    let path = directory.join(format!("closing-{allocation}.json"));
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            if read_value(&path, true)?
                != object([
                    ("format", Value::Str("fsm.native-closing/1".into())),
                    ("domain", domain.clone()),
                ])
            {
                return Err("native observation closing material differs".into());
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io(error)),
    }
}

fn events(bytes: &[u8]) -> Result<(bool, bool), String> {
    let value = std::str::from_utf8(bytes).map_err(|_| "invalid native events encoding")?;
    let mut fields = BTreeMap::new();
    for line in value.lines() {
        let (key, raw) = line.split_once(' ').ok_or("invalid native event field")?;
        let boolean = match raw {
            "0" => false,
            "1" => true,
            _ => return Err("invalid native event boolean".into()),
        };
        if fields.insert(key, boolean).is_some() {
            return Err("duplicate native event field".into());
        }
    }
    if fields.len() != 2 {
        return Err("native event fields differ".into());
    }
    Ok((
        *fields.get("populated").ok_or("population missing")?,
        *fields.get("frozen").ok_or("freeze state missing")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_events_are_exact_boolean_samples_not_closure_proofs() {
        assert_eq!(events(b"populated 1\nfrozen 0\n").unwrap(), (true, false));
        assert_eq!(events(b"frozen 1\npopulated 0\n").unwrap(), (false, true));
        for bytes in [
            b"populated 0\n".as_slice(),
            b"populated 0\nfrozen 00\n",
            b"populated 2\nfrozen 0\n",
            b"populated 0\npopulated 1\n",
            b"populated 0\nfrozen 0\nunknown 0\n",
            b"invalid",
            b"\xff",
        ] {
            assert!(events(bytes).is_err());
        }
    }
}
