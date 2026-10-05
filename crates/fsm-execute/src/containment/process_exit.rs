//! Invocation-matched root status is a candidate, never tree closure proof.

use super::super::{manager, number, text};
use fsm_core::json::Value;
use std::collections::BTreeMap;
use std::time::Instant;

pub(super) fn observe(
    domain: &Value,
    gate: &Value,
    deadline: Instant,
) -> Result<Option<i32>, String> {
    let unit = format!(
        "fsm-containment-{}-{}-{}.service",
        text(domain, "namespace")?,
        number(domain, "generation")?,
        number(domain, "allocation")?
    );
    let properties = manager::properties_before(
        &unit,
        &[
            "InvocationID",
            "ExecMainPID",
            "ExecMainCode",
            "ExecMainStatus",
        ],
        deadline,
    )?;
    decode(
        &properties,
        number(gate, "pid")?,
        text(gate, "invocation_id")?,
    )
}

fn decode(
    properties: &BTreeMap<String, String>,
    pid: u64,
    invocation: &str,
) -> Result<Option<i32>, String> {
    if pid == 0
        || pid > u64::from(u32::MAX)
        || invocation.len() != 32
        || invocation.bytes().all(|byte| byte == b'0')
        || !invocation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || properties.len() != 4
        || properties.get("InvocationID").map(String::as_str) != Some(invocation)
        || properties.get("ExecMainPID") != Some(&pid.to_string())
    {
        return Err("manager root status identity differs".into());
    }
    let integer = |key: &str| -> Result<u32, String> {
        let raw = properties
            .get(key)
            .ok_or("manager root status field missing")?;
        let value = raw
            .parse::<u32>()
            .map_err(|_| "manager root status integer invalid")?;
        if raw != &value.to_string() {
            return Err("manager root status integer noncanonical".into());
        }
        Ok(value)
    };
    let code = integer("ExecMainCode")?;
    let status = integer("ExecMainStatus")?;
    match (code, status) {
        (0, 0) => Ok(None),
        (1, 0..=255) => Ok(Some(
            i32::try_from(status).map_err(|_| "manager exit status invalid")?,
        )),
        (2 | 3, 1..=64) => Ok(Some(-1)),
        _ => Err("manager root status is not a recognized exit observation".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(code: &str, status: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("InvocationID".into(), "a".repeat(32)),
            ("ExecMainPID".into(), "42".into()),
            ("ExecMainCode".into(), code.into()),
            ("ExecMainStatus".into(), status.into()),
        ])
    }

    #[test]
    fn root_status_requires_matching_invocation_and_canonical_exit_fields() {
        let invocation = "a".repeat(32);
        assert_eq!(decode(&fields("0", "0"), 42, &invocation).unwrap(), None);
        for status in ["0", "17", "255"] {
            assert_eq!(
                decode(&fields("1", status), 42, &invocation).unwrap(),
                Some(status.parse().unwrap())
            );
        }
        for code in ["2", "3"] {
            assert_eq!(
                decode(&fields(code, "9"), 42, &invocation).unwrap(),
                Some(-1)
            );
        }
        for (code, status) in [
            ("1", "256"),
            ("01", "0"),
            ("1", "00"),
            ("0", "1"),
            ("2", "0"),
            ("3", "65"),
            ("4", "9"),
            ("1", "-1"),
        ] {
            assert!(decode(&fields(code, status), 42, &invocation).is_err());
        }
        assert!(decode(&fields("1", "0"), 43, &invocation).is_err());
        assert!(decode(&fields("1", "0"), 0, &invocation).is_err());
        assert!(decode(&fields("1", "0"), 42, &"b".repeat(32)).is_err());
        let mut excess = fields("1", "0");
        excess.insert("unknown".into(), "0".into());
        assert!(decode(&excess, 42, &invocation).is_err());
        for key in [
            "InvocationID",
            "ExecMainPID",
            "ExecMainCode",
            "ExecMainStatus",
        ] {
            let mut partial = fields("1", "0");
            partial.remove(key);
            assert!(decode(&partial, 42, &invocation).is_err());
        }
    }
}
