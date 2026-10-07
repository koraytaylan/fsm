//! Structural event validation with symbolic, clock-free store stamping.

use super::report::object;
use super::{CheckStatus, Finding, Limits, Report};
use crate::config::{Advance, HandlerTable};
use crate::error::ExecError;
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::machine::CompiledMachine;
use fsm_core::spec::TySpec;
use fsm_core::step::validate_event;
use std::collections::BTreeMap;

fn limit() -> ExecError {
    ExecError::new(
        "exec/contract_limit",
        "complete contract report exceeded its declared budget",
    )
    .hint("reduce the checked closure or use caller sublimits within the hard ceilings")
}

/// Check effects and both outcomes in the supplied static definition closure.
/// No clock, store mutation, execution admission or reachability proof is used.
pub fn analyze_contract(
    root: &CompiledMachine,
    catalogue: &BTreeMap<String, CompiledMachine>,
    table: &HandlerTable,
    limits: Limits,
) -> Result<Report, ExecError> {
    analyze_resolved(
        root,
        &|identity| {
            if identity.contains("@sha256:") {
                catalogue
                    .values()
                    .find(|machine| machine.machine_id == identity)
            } else {
                catalogue.get(identity)
            }
        },
        table,
        limits,
    )
}

/// Resolve both invocation digests and full definition identities in a borrowed
/// immutable view; the public owned-catalogue API keeps its existing contract.
pub(super) fn analyze_resolved<'a>(
    root: &'a CompiledMachine,
    resolve: &dyn Fn(&str) -> Option<&'a CompiledMachine>,
    table: &HandlerTable,
    limits: Limits,
) -> Result<Report, ExecError> {
    let ceiling = Limits::default();
    if limits.findings > ceiling.findings || limits.report_bytes > ceiling.report_bytes {
        return Err(limit());
    }
    // Share traversal without constructing effect-only outcome placeholders.
    let mut report = super::effects::analyze_resolved(
        root,
        resolve,
        table,
        Limits {
            findings: limits.findings,
            report_bytes: ceiling.report_bytes,
            ..limits
        },
        false,
    )?;
    report.outcomes_checked = true;
    // A shared handler's static outcome is identical at every site in one
    // definition. Validate its payload once rather than cloning and parsing
    // a potentially large payload for every syntactic emit.
    let mut checks = BTreeMap::new();
    for site in &mut report.effects {
        if site.disposition != "automatic" {
            continue;
        }
        let machine = if site.machine_id == root.machine_id {
            root
        } else {
            resolve(&site.machine_id).ok_or_else(|| {
                ExecError::new(
                    "exec/contract_definition_unknown",
                    "checked definition unavailable",
                )
            })?
        };
        let handler = &table.handlers[&site.effect];
        for (name, advance) in [("on_ok", &handler.on_ok), ("on_failed", &handler.on_failed)] {
            let Some(advance) = advance else {
                continue;
            };
            let (status, findings) = checks
                .entry((site.machine_id.clone(), site.effect.clone(), name))
                .or_insert_with(|| check(machine, advance));
            site.outcomes.insert(name.into(), status.as_str());
            if !machine
                .spec
                .transitions
                .iter()
                .any(|transition| transition.on.as_deref() == Some(&advance.event))
            {
                report.progress.insert("no-transition".into());
            }
            for (code, severity, message, hint, cause) in findings.iter().cloned() {
                if report.findings.len() >= limits.findings {
                    return Err(limit());
                }
                report.findings.push(Finding {
                    code,
                    severity,
                    machine_id: site.machine_id.clone(),
                    path: site.path.clone(),
                    effect: Some(site.effect.clone()),
                    outcome: Some(name.into()),
                    message,
                    hint,
                    cause,
                });
            }
        }
    }
    report.status = if report
        .findings
        .iter()
        .any(|finding| finding.severity == "error")
    {
        CheckStatus::Invalid
    } else if report
        .findings
        .iter()
        .any(|finding| finding.severity == "unknown")
    {
        CheckStatus::Unknown
    } else {
        CheckStatus::Compatible
    };
    report.findings.sort_by(|left, right| {
        (
            &left.machine_id,
            &left.path,
            &left.effect,
            &left.outcome,
            left.code,
        )
            .cmp(&(
                &right.machine_id,
                &right.path,
                &right.effect,
                &right.outcome,
                right.code,
            ))
    });
    if canon_bytes(&report.to_value()).len() > limits.report_bytes {
        return Err(limit());
    }
    Ok(report)
}

type Detail = (&'static str, &'static str, String, String, Option<Value>);

fn check(machine: &CompiledMachine, advance: &Advance) -> (CheckStatus, Vec<Detail>) {
    let mut payload = advance.payload.clone();
    let mut unknown = Vec::new();
    let event = machine
        .spec
        .events
        .iter()
        .find(|event| event.name == advance.event);
    if let Value::Obj(fields) = &mut payload {
        for name in &advance.stamps {
            if fields.contains_key(name) {
                continue;
            }
            let field =
                event.and_then(|event| event.fields.iter().find(|field| field.name == *name));
            let witness = match field.map(|field| &field.ty) {
                Some(TySpec::Enum { of }) => {
                    let matching = machine.spec.enums.get(of).and_then(|members| {
                        members.iter().find(|member| {
                            member
                                .parse::<i64>()
                                .is_ok_and(|value| value.to_string() == **member)
                        })
                    });
                    if let Some(member) = matching {
                        unknown.push(name.clone());
                        member.clone()
                    } else {
                        "0".into()
                    }
                }
                _ => "0".into(),
            };
            // Witness values check the rest of the concrete payload. The
            // family proof above, not this witness, establishes compatibility.
            fields.insert(name.clone(), Value::Str(witness));
        }
    }
    if let Err(cause) = validate_event(machine, &advance.event, &payload) {
        let code = if matches!(cause.code, "req/event_unknown" | "req/event_internal") {
            "exec/contract_outcome_event"
        } else {
            "exec/contract_outcome_payload"
        };
        return (
            CheckStatus::Invalid,
            vec![(
                code,
                "error",
                "configured outcome is not a valid externally sendable event".into(),
                "fix the outcome event, payload or stamped field type using the underlying cause"
                    .into(),
                Some(object([
                    ("code", Value::Str(cause.code.into())),
                    ("message", Value::Str(cause.message)),
                    ("hint", Value::Str(cause.hint)),
                ])),
            )],
        );
    }
    if unknown.is_empty() {
        (CheckStatus::Compatible, Vec::new())
    } else {
        (CheckStatus::Unknown, unknown.into_iter().map(|name| (
        "exec/contract_unknown", "unknown",
        format!("stamped enum field {name} accepts only part of the signed-millisecond family"),
        "supply a literal enum member or use a field type that accepts every signed millisecond string".into(), None,
    )).collect())
    }
}
