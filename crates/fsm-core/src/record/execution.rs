//! Pure identity and retry values for SPEC's reserved claim-era contract.
//!
//! These values describe evidence; constructing one does not authenticate a
//! native supervisor or prove closure. Store mutators must perform that check.

use std::collections::BTreeMap;
use std::fmt;

use crate::json::Value;

mod bounded;
mod handoff;
mod ownership;
mod ownership_values;

pub use handoff::AcknowledgedHandoff;
pub use ownership::{Admission, ExecutionState, PendingEffect, Settlement};
pub use ownership_values::{Claim, Closure, Stopped, StoppedOutcome};

/// A malformed field in a claim-era value, without platform I/O or error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeError(pub &'static str);

impl fmt::Display for ShapeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid execution field: {}", self.0)
    }
}

impl std::error::Error for ShapeError {}

/// Native file identity; a path or PID cannot replace it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    device: u64,
    inode: u64,
}

impl FileIdentity {
    /// Construct an identity with a positive inode; device zero is valid.
    pub fn new(device: u64, inode: u64) -> Result<Self, ShapeError> {
        if inode == 0 {
            return Err(ShapeError("inode"));
        }
        Ok(Self { device, inode })
    }

    /// Return the native device identifier.
    pub fn device(self) -> u64 {
        self.device
    }

    /// Return the positive native inode identifier.
    pub fn inode(self) -> u64 {
        self.inode
    }

    fn value(self) -> Value {
        object([
            ("device", number(self.device)),
            ("inode", number(self.inode)),
        ])
    }

    fn decode(value: &Value) -> Result<Self, ShapeError> {
        closed(value, &["device", "inode"])?;
        Self::new(unsigned(value, "device")?, unsigned(value, "inode")?)
    }
}

/// Full identity of the initially selected Linux/systemd containment domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDomain {
    namespace: String,
    allocation: u64,
    boot: String,
    cgroup: FileIdentity,
    authority: FileIdentity,
    generation: u64,
}

impl NativeDomain {
    /// Backend discriminator for the selected containment implementation.
    pub const BACKEND: &str = "linux-systemd/1";

    /// Validate the complete domain identity without authenticating it.
    pub fn new(
        namespace: String,
        allocation: u64,
        boot: String,
        cgroup: FileIdentity,
        authority: FileIdentity,
        generation: u64,
    ) -> Result<Self, ShapeError> {
        if !hex(&namespace, 32) {
            return Err(ShapeError("namespace"));
        }
        if !boot_id(&boot) {
            return Err(ShapeError("boot"));
        }
        if allocation == 0 || generation == 0 {
            return Err(ShapeError("native_counter"));
        }
        Ok(Self {
            namespace,
            allocation,
            boot,
            cgroup,
            authority,
            generation,
        })
    }

    /// Encode the closed canonical value described in SPEC.
    pub fn to_value(&self) -> Value {
        object([
            ("backend", Value::Str(Self::BACKEND.into())),
            ("namespace", Value::Str(self.namespace.clone())),
            ("allocation", number(self.allocation)),
            ("boot", Value::Str(self.boot.clone())),
            ("cgroup", self.cgroup.value()),
            ("authority", self.authority.value()),
            ("generation", number(self.generation)),
        ])
    }

    /// Decode a closed value, rejecting unknown fields and invalid identities.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        closed(
            value,
            &[
                "backend",
                "namespace",
                "allocation",
                "boot",
                "cgroup",
                "authority",
                "generation",
            ],
        )?;
        if text(value, "backend")? != Self::BACKEND {
            return Err(ShapeError("backend"));
        }
        let namespace = text(value, "namespace")?;
        let boot = text(value, "boot")?;
        // Validate attacker-controlled lengths before cloning their strings.
        if !hex(namespace, 32) || !boot_id(boot) {
            return Err(ShapeError("native_identity"));
        }
        Self::new(
            namespace.into(),
            unsigned(value, "allocation")?,
            boot.into(),
            FileIdentity::decode(value.get("cgroup").ok_or(ShapeError("cgroup"))?)?,
            FileIdentity::decode(value.get("authority").ok_or(ShapeError("authority"))?)?,
            unsigned(value, "generation")?,
        )
    }
}

/// Existing executor failure classes, ordered by their canonical names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FailureClass {
    McpError,
    NonzeroExit,
    Spawn,
    Timeout,
}

impl FailureClass {
    /// Return the canonical failure class name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::McpError => "mcp_error",
            Self::NonzeroExit => "nonzero_exit",
            Self::Spawn => "spawn",
            Self::Timeout => "timeout",
        }
    }

    fn parse(text: &str) -> Result<Self, ShapeError> {
        match text {
            "mcp_error" => Ok(Self::McpError),
            "nonzero_exit" => Ok(Self::NonzeroExit),
            "spawn" => Ok(Self::Spawn),
            "timeout" => Ok(Self::Timeout),
            _ => Err(ShapeError("failure_class")),
        }
    }
}

/// Immutable retry policy; failed attempts and run identities remain separate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    attempts: u32,
    backoff_ms: i64,
    max_backoff_ms: i64,
    on: Vec<FailureClass>,
}

impl RetryPolicy {
    /// Validate bounds and normalize at most four supplied failure classes.
    pub fn new(
        attempts: u32,
        backoff_ms: i64,
        max_backoff_ms: i64,
        mut on: Vec<FailureClass>,
    ) -> Result<Self, ShapeError> {
        if !(1..=16).contains(&attempts) {
            return Err(ShapeError("attempts"));
        }
        if backoff_ms <= 0 || max_backoff_ms < backoff_ms {
            return Err(ShapeError("backoff_ms"));
        }
        if on.len() > 4 {
            return Err(ShapeError("on"));
        }
        on.sort_unstable();
        on.dedup();
        Ok(Self {
            attempts,
            backoff_ms,
            max_backoff_ms,
            on,
        })
    }

    /// Encode the closed canonical value described in SPEC.
    pub fn to_value(&self) -> Value {
        object([
            ("attempts", number(u64::from(self.attempts))),
            ("backoff_ms", Value::Num(self.backoff_ms.to_string())),
            (
                "max_backoff_ms",
                Value::Num(self.max_backoff_ms.to_string()),
            ),
            (
                "on",
                Value::Arr(
                    self.on
                        .iter()
                        .map(|c| Value::Str(c.as_str().into()))
                        .collect(),
                ),
            ),
        ])
    }

    /// Decode a closed value, rejecting unknown fields and invalid identities.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        closed(value, &["attempts", "backoff_ms", "max_backoff_ms", "on"])?;
        let entries = value
            .get("on")
            .and_then(Value::as_arr)
            .ok_or(ShapeError("on"))?;
        if entries.len() > 4 {
            return Err(ShapeError("on"));
        }
        let classes: Vec<_> = entries
            .iter()
            .map(|v| FailureClass::parse(v.as_str().ok_or(ShapeError("on"))?))
            .collect::<Result<_, _>>()?;
        // Persisted policies must already be canonical, not silently repaired.
        if classes.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ShapeError("on"));
        }
        Self::new(
            u32::try_from(unsigned(value, "attempts")?).map_err(|_| ShapeError("attempts"))?,
            signed(value, "backoff_ms")?,
            signed(value, "max_backoff_ms")?,
            classes,
        )
    }

    /// Exact existing saturating backoff after a positive failed count.
    pub fn ready_at(&self, failed_count: u32, last_timestamp: i64) -> Result<i64, ShapeError> {
        if !(1..=16).contains(&failed_count) {
            return Err(ShapeError("failed_count"));
        }
        let factor = 1_i64 << (failed_count - 1);
        let delay = self
            .backoff_ms
            .saturating_mul(factor)
            .min(self.max_backoff_ms);
        Ok(last_timestamp.saturating_add(delay))
    }

    /// Check the exact retry deadline, class, and remaining attempt allowance.
    pub fn admits_retry(
        &self,
        failed_count: u32,
        class: FailureClass,
        last_timestamp: i64,
        now: i64,
    ) -> bool {
        failed_count > 0
            && failed_count < self.attempts
            && self.on.contains(&class)
            && self
                .ready_at(failed_count, last_timestamp)
                .is_ok_and(|due| now >= due)
    }
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn number(value: u64) -> Value {
    Value::Num(value.to_string())
}

fn closed(value: &Value, fields: &[&str]) -> Result<(), ShapeError> {
    let object = value.as_obj().ok_or(ShapeError("object"))?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(ShapeError("fields"));
    }
    Ok(())
}

fn text<'a>(value: &'a Value, field: &'static str) -> Result<&'a str, ShapeError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or(ShapeError(field))
}

fn unsigned(value: &Value, field: &'static str) -> Result<u64, ShapeError> {
    value
        .get(field)
        .and_then(Value::as_num)
        .and_then(|v| v.parse().ok())
        .ok_or(ShapeError(field))
}

fn signed(value: &Value, field: &'static str) -> Result<i64, ShapeError> {
    value
        .get(field)
        .and_then(Value::as_num)
        .and_then(|v| v.parse().ok())
        .ok_or(ShapeError(field))
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn boot_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
