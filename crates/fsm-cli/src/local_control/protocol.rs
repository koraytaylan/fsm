//! Closed local-control payloads; metadata waiting never reads or opens Store.
use fsm_core::json::Value;
use fsm_execute::{
    error::ExecError,
    service::{ExecutorControl, ExecutorPhase, ShutdownMode, ShutdownReport, ShutdownRequest},
};
use std::collections::BTreeMap;

pub(super) const REQUEST_CAP: usize = 1024;
pub(super) const RESPONSE_CAP: usize = 128 * 1024;

// Constructed by the endpoint publisher from its original driver, protected
// random incarnation and actual physical writer identity; never a PID.
#[derive(Clone)]
pub(super) struct ControlIdentity {
    pub(super) incarnation: String,
    pub(super) store_device: u64,
    pub(super) store_inode: u64,
}

pub(super) fn apply_request(
    control: &ExecutorControl,
    identity: &ControlIdentity,
    value: &Value,
) -> Result<ShutdownRequest, ExecError> {
    let malformed = || ExecError::new("exec/config", "local control request schema differs");
    let fields = value.as_obj().ok_or_else(malformed)?;
    if fields.len() != 6
        || value.get("format").and_then(Value::as_str) != Some("fsm.executor-control/1")
    {
        return Err(malformed());
    }
    if value.get("incarnation").and_then(Value::as_str) != Some(identity.incarnation.as_str())
        || value
            .get("store_device")
            .and_then(Value::as_num)
            .and_then(|value| value.parse::<u64>().ok())
            != Some(identity.store_device)
        || value
            .get("store_inode")
            .and_then(Value::as_num)
            .and_then(|value| value.parse::<u64>().ok())
            != Some(identity.store_inode)
    {
        return Err(ExecError::new(
            "exec/mode",
            "local control original incarnation differs",
        ));
    }
    let mode = match value.get("mode").and_then(Value::as_str) {
        Some("drain") => ShutdownMode::Drain,
        Some("abort") => ShutdownMode::Abort,
        _ => return Err(malformed()),
    };
    let timeout_ms = value
        .get("timeout_ms")
        .and_then(Value::as_num)
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(malformed)?;
    // The actual control validates finite bounds before fence closure and
    // preserves the first request deadline/mode; no replacement state here.
    control.stop(mode, timeout_ms)
}

pub(super) fn request_value(
    identity: &ControlIdentity,
    mode: ShutdownMode,
    timeout_ms: i64,
) -> Value {
    Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str("fsm.executor-control/1".into())),
        (
            "incarnation".into(),
            Value::Str(identity.incarnation.clone()),
        ),
        (
            "store_device".into(),
            Value::Num(identity.store_device.to_string()),
        ),
        (
            "store_inode".into(),
            Value::Num(identity.store_inode.to_string()),
        ),
        (
            "mode".into(),
            Value::Str(
                match mode {
                    ShutdownMode::Drain => "drain",
                    ShutdownMode::Abort => "abort",
                }
                .into(),
            ),
        ),
        ("timeout_ms".into(), Value::Num(timeout_ms.to_string())),
    ]))
}

pub(super) fn report_value(identity: &ControlIdentity, report: &ShutdownReport) -> Value {
    let phase = match report.phase {
        ExecutorPhase::Running => "running",
        ExecutorPhase::Draining => "draining",
        ExecutorPhase::Stopping => "stopping",
        ExecutorPhase::Stopped => "stopped",
        ExecutorPhase::Uncertain => "uncertain",
    };
    Value::Obj(BTreeMap::from([
        (
            "format".into(),
            Value::Str("fsm.executor-control-report/1".into()),
        ),
        (
            "incarnation".into(),
            Value::Str(identity.incarnation.clone()),
        ),
        (
            "store_device".into(),
            Value::Num(identity.store_device.to_string()),
        ),
        (
            "store_inode".into(),
            Value::Num(identity.store_inode.to_string()),
        ),
        ("phase".into(), Value::Str(phase.into())),
        (
            "admission_closed".into(),
            Value::Bool(report.admission_closed),
        ),
        ("timed_out".into(), Value::Bool(report.timed_out)),
        (
            "unresolved_run_ids".into(),
            Value::Arr(
                report
                    .unresolved_run_ids
                    .iter()
                    .map(|run_id| Value::Num(run_id.to_string()))
                    .collect(),
            ),
        ),
        (
            "unclaimed_reservations".into(),
            report
                .unclaimed_reservations
                .map_or(Value::Null, |count| Value::Num(count.to_string())),
        ),
        (
            "inventory_complete".into(),
            Value::Bool(report.inventory_complete),
        ),
        (
            "helpers_retired".into(),
            Value::Bool(report.helpers_retired),
        ),
        (
            "writer_released".into(),
            Value::Bool(report.writer_released),
        ),
    ]))
}

pub(super) fn identity_value(identity: &ControlIdentity) -> Value {
    Value::Obj(BTreeMap::from([
        (
            "format".into(),
            Value::Str("fsm.executor-control-endpoint/1".into()),
        ),
        (
            "incarnation".into(),
            Value::Str(identity.incarnation.clone()),
        ),
        (
            "store_device".into(),
            Value::Num(identity.store_device.to_string()),
        ),
        (
            "store_inode".into(),
            Value::Num(identity.store_inode.to_string()),
        ),
    ]))
}

pub(super) fn parse_identity(value: &Value) -> Option<ControlIdentity> {
    if value.as_obj()?.len() != 4
        || value.get("format")?.as_str()? != "fsm.executor-control-endpoint/1"
    {
        return None;
    }
    let incarnation = value.get("incarnation")?.as_str()?;
    if incarnation.len() != 64
        || !incarnation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(ControlIdentity {
        incarnation: incarnation.into(),
        store_device: value.get("store_device")?.as_num()?.parse().ok()?,
        store_inode: value.get("store_inode")?.as_num()?.parse().ok()?,
    })
}

pub(super) fn observation_value(identity: &ControlIdentity) -> Value {
    let Value::Obj(mut fields) = identity_value(identity) else {
        unreachable!("closed identity object")
    };
    fields.insert("format".into(), Value::Str("fsm.executor-observe/2".into()));
    Value::Obj(fields)
}

pub(super) fn validate_observation(
    identity: &ControlIdentity,
    value: &Value,
) -> Result<(), ExecError> {
    let mut legacy = observation_value(identity);
    if let Value::Obj(fields) = &mut legacy {
        fields.insert("format".into(), Value::Str("fsm.executor-observe/1".into()));
    }
    if value == &observation_value(identity) || value == &legacy {
        Ok(())
    } else {
        Err(ExecError::new(
            "exec/config",
            "observation schema or original identity differs",
        ))
    }
}

pub(super) const PREPARATION_PHASES: [&str; 10] = [
    "queued",
    "preparing",
    "prepared",
    "cleaning",
    "unknown_allocation",
    "uncertain_preparation",
    "uncertain_cleanup",
    "uncertain_domain",
    "claim_uncertain",
    "closed",
];

pub(super) fn observation_report_value(
    identity: &ControlIdentity,
    report: &ShutdownReport,
    phases: Option<[usize; 10]>,
) -> Value {
    let Value::Obj(mut fields) = report_value(identity, report) else {
        unreachable!()
    };
    fields.insert(
        "format".into(),
        Value::Str("fsm.executor-observation-report/1".into()),
    );
    let phases = phases.map_or(Value::Null, |counts| {
        Value::Obj(
            PREPARATION_PHASES
                .into_iter()
                .zip(counts)
                .map(|(name, count)| (name.into(), Value::Num(count.to_string())))
                .collect(),
        )
    });
    fields.insert("preparation_phases".into(), phases);
    Value::Obj(fields)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReportKind {
    Terminal,
    Observed,
}

pub(super) fn valid_report(value: &Value, identity: &ControlIdentity, kind: ReportKind) -> bool {
    let valid = || -> Option<()> {
        let (fields, format) = match kind {
            ReportKind::Terminal => (12, "fsm.executor-control-report/1"),
            ReportKind::Observed => (13, "fsm.executor-observation-report/1"),
        };
        if value.as_obj()?.len() != fields
            || value.get("format")?.as_str()? != format
            || value.get("incarnation")?.as_str()? != identity.incarnation
            || value.get("store_device")?.as_num()?.parse::<u64>().ok()? != identity.store_device
            || value.get("store_inode")?.as_num()?.parse::<u64>().ok()? != identity.store_inode
        {
            return None;
        }
        let phase = value.get("phase")?.as_str()?;
        if !matches!(
            phase,
            "running" | "draining" | "stopping" | "stopped" | "uncertain"
        ) || (kind == ReportKind::Terminal && !matches!(phase, "stopped" | "uncertain"))
        {
            return None;
        }
        for field in [
            "admission_closed",
            "timed_out",
            "inventory_complete",
            "helpers_retired",
            "writer_released",
        ] {
            if !matches!(value.get(field)?, Value::Bool(_)) {
                return None;
            }
        }
        if (kind == ReportKind::Terminal || phase == "stopped")
            && value.get("admission_closed")? != &Value::Bool(true)
        {
            return None;
        }
        let Value::Arr(ids) = value.get("unresolved_run_ids")? else {
            return None;
        };
        if ids.len() > 4096
            || ids.iter().any(|id| {
                id.as_num()
                    .and_then(|number| number.parse::<u64>().ok())
                    .is_none()
            })
        {
            return None;
        }
        let reservations = value.get("unclaimed_reservations")?;
        if reservations != &Value::Null && reservations.as_num()?.parse::<usize>().is_err() {
            return None;
        }
        if kind == ReportKind::Observed {
            let phases = value.get("preparation_phases")?;
            if phases != &Value::Null {
                if phases.as_obj()?.len() != PREPARATION_PHASES.len() {
                    return None;
                }
                let mut total = 0usize;
                for name in PREPARATION_PHASES {
                    total =
                        total.checked_add(phases.get(name)?.as_num()?.parse::<usize>().ok()?)?;
                    if total > 4096 {
                        return None;
                    }
                }
                if reservations.as_num()?.parse::<usize>().ok()? != total {
                    return None;
                }
            }
        }
        if phase == "stopped"
            && (!ids.is_empty()
                || reservations != &Value::Num("0".into())
                || ["inventory_complete", "helpers_retired", "writer_released"]
                    .iter()
                    .any(|key| value.get(key) != Some(&Value::Bool(true))))
        {
            return None;
        }
        Some(())
    };
    valid().is_some()
}
