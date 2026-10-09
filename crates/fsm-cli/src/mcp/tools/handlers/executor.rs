//! Read-only draft evidence; registry/session dispatch is integrated separately.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::{canon::canon_bytes, json::Value, spec::compile_accepted};
use fsm_execute::{
    config::HandlerTable,
    contract::{CheckStatus, Finding, Limits, Report, analyze_contract},
};

use crate::store::{ErrorObj, Store};

/// Private host provenance, never supplied by a tool caller.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private preparation for task 9202 session dispatch"
    )
)]
pub(in crate::mcp) enum ExecutionContext<'a> {
    Embedded(&'a HandlerTable),
    Writer,
    ReadOnly,
    Degraded,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "private preparation for task 9202 session dispatch"
    )
)]
pub(in crate::mcp) fn check(
    store: Option<&Store>,
    arguments: &Value,
    context: ExecutionContext<'_>,
) -> Result<Value, ErrorObj> {
    let context = match store {
        None => ExecutionContext::Degraded,
        Some(store) if store.journal.poisoned => ExecutionContext::Degraded,
        Some(store) if store.journal.is_read_only() => ExecutionContext::ReadOnly,
        Some(_) => context,
    };
    let (table, mode) = match context {
        ExecutionContext::Embedded(table) => (Some(table), None),
        ExecutionContext::Writer => (None, Some("writer")),
        ExecutionContext::ReadOnly => (None, Some("read-only")),
        ExecutionContext::Degraded => (None, Some("degraded")),
    };
    let spec = arguments.get("spec");
    let machine = arguments.get("machine");
    if spec.is_some() == machine.is_some()
        || arguments.as_obj().is_none_or(|fields| {
            fields
                .keys()
                .any(|key| !matches!(key.as_str(), "spec" | "machine"))
        })
        || spec.is_some_and(|value| !value.is_obj())
        || machine.is_some_and(|value| value.as_str().is_none_or(str::is_empty))
    {
        return Err(ErrorObj::new("req/args_invalid", "select exactly one draft spec or stored machine")
            .hint("supply an object spec or a nonempty machine name or identity, without handler overrides"));
    }
    let mut report = empty_report();
    let root = if let Some(spec) = spec {
        match compile_accepted(spec) {
            Ok(compiled) => Some(compiled),
            Err(findings) => {
                if findings.len() + usize::from(mode.is_some()) > Limits::default().findings {
                    return Err(ErrorObj::new(
                        "exec/contract_limit",
                        "draft findings exceeded their declared budget",
                    ));
                }
                report.status = CheckStatus::Invalid;
                report.findings = findings
                    .into_iter()
                    .map(|finding| Finding {
                        code: finding.code,
                        severity: "error",
                        machine_id: String::new(),
                        path: finding.path,
                        effect: None,
                        outcome: None,
                        message: finding.message,
                        hint: finding.hint,
                        cause: None,
                    })
                    .collect();
                None
            }
        }
    } else if let Some(store) = store.filter(|store| !store.journal.poisoned) {
        Some(
            store
                .resolve_machine(machine.unwrap().as_str().unwrap())?
                .compiled
                .clone(),
        )
    } else {
        report.findings.push(Finding {
            code: "exec/contract_definition_unknown",
            severity: "unknown",
            machine_id: String::new(),
            path: "/machine".into(),
            effect: None,
            outcome: None,
            message: "the stored definition is unavailable in this degraded session".into(),
            hint: "check a draft spec independently, or diagnose the store before checking its definition".into(),
            cause: None,
        });
        None
    };
    if let Some(root) = root {
        let catalogue = store
            .filter(|store| !store.journal.poisoned)
            .map(|store| {
                store
                    .state
                    .machines
                    .iter()
                    .filter_map(|(identity, stored)| {
                        fsm_core::hashes::digest_of(identity)
                            .map(|digest| (digest.to_string(), stored.compiled.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        report = analyze_contract(
            &root,
            &catalogue,
            table.unwrap_or(&HandlerTable::default()),
            Limits::default(),
        )
        .map_err(|error| {
            ErrorObj::new(error.code, error.message).hint(error.hint.unwrap_or_default())
        })?;
        if table.is_none() {
            // No empty-table contradiction is evidence about an unavailable table.
            // Replacement text is larger than the original missing-handler finding;
            // provenance also outweighs the removed identity, so an intermediate
            // canonical-byte refusal cannot reject a fitting final report.
            for finding in &mut report.findings {
                if finding.code == "exec/contract_handler_missing" {
                    finding.code = "exec/contract_unknown";
                    finding.severity = "unknown";
                    finding.message = "an emitted effect has no authoritative automatic or manual disposition available in this session".into();
                    finding.hint = "load this host's operator table before determining whether this emitted effect is automatic or explicitly reserved for manual acknowledgement".into();
                }
            }
            report.status = CheckStatus::Unknown;
        }
    }
    if let Some(mode) = mode {
        report.contract_id = None;
        report.effects_checked = false;
        report.outcomes_checked = false;
        if report.status != CheckStatus::Invalid {
            report.status = CheckStatus::Unknown;
        }
        report.findings.push(Finding {
            code: "exec/contract_unknown",
            severity: "unknown",
            machine_id: report.machine_id.clone().unwrap_or_default(),
            path: String::new(),
            effect: None,
            outcome: None,
            message: "this session has no authoritative active execution table; another process's possible executor supplies no compatibility evidence".into(),
            hint: "use a writable session attached to the execution host with its loaded operator table, then check again before relying on execution compatibility".into(),
            cause: Some(Value::Obj(BTreeMap::from([
                ("mode".into(), Value::Str(mode.into())),
                ("table".into(), Value::Str("unavailable".into())),
            ]))),
        });
    }
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
    let value = report.to_value();
    let limits = Limits::default();
    if report.findings.len() > limits.findings || canon_bytes(&value).len() > limits.report_bytes {
        return Err(ErrorObj::new(
            "exec/contract_limit",
            "draft contract report exceeded its declared budget",
        )
        .hint("reduce the draft closure or emitted sites before checking again"));
    }
    Ok(value)
}

fn empty_report() -> Report {
    Report {
        status: CheckStatus::Unknown,
        machine_id: None,
        contract_id: None,
        definitions: BTreeSet::new(),
        effects_checked: false,
        outcomes_checked: false,
        dynamic_signals: false,
        findings: Vec::new(),
        effects: Vec::new(),
        progress: BTreeSet::new(),
    }
}

#[cfg(test)]
mod tests;
