//! Canonical JSON (FSM-CJSON) helpers used by every hash in the system.

use crate::json::{JsonError, JsonLimits, Value, parse, write_canonical};

/// Serialize `v` to FSM-CJSON bytes (single line, sorted keys, minimal escapes).
pub fn canon_bytes(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_canonical(v, &mut out);
    out
}

/// Parse `bytes` and report whether they already equal their canonical form.
///
/// Invalid JSON is returned as `Err`, not `Ok(false)`.
pub fn is_canonical(bytes: &[u8], limits: &JsonLimits) -> Result<bool, JsonError> {
    let v = parse(bytes, limits)?;
    Ok(canon_bytes(&v) == bytes)
}
