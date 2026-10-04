//! Conservative traversal of actual emits, not declared fields or reachable states.

use super::report::{object, strings};
use super::{CheckStatus, EffectSite, Finding, Limits, Report};
use crate::config::{Advance, HandlerTable};
use crate::error::ExecError;
use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;
use fsm_core::machine::{CompiledMachine, ExprSlot};
use fsm_core::sha256::{sha256, to_hex};
use fsm_core::spec::{EmitSpec, StateNode, Topology};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn limit() -> ExecError {
    ExecError::new(
        "exec/contract_limit",
        "executor contract analysis exceeded its declared budget",
    )
    .hint("reduce the checked closure or emits, or raise caller sublimits within the documented hard ceilings")
}

fn advance(value: Option<&Advance>) -> Value {
    value.map_or(Value::Null, |value| {
        object([
            ("event", Value::Str(value.event.clone())),
            ("payload", value.payload.clone()),
            ("stamps", strings(value.stamps.iter().cloned())),
        ])
    })
}

fn identity(table: &HandlerTable, required: &BTreeMap<String, BTreeSet<String>>) -> String {
    let metadata = object([
        (
            "manual_effects",
            strings(table.manual_effects.iter().cloned()),
        ),
        ("max_inflight", Value::Num(table.max_inflight.to_string())),
        (
            "max_inflight_per_instance",
            Value::Num(table.max_inflight_per_instance.to_string()),
        ),
        (
            "handlers",
            Value::Arr(
                table
                    .handlers
                    .iter()
                    .map(|(name, handler)| {
                        object([
                            ("effect", Value::Str(handler.effect.clone())),
                            ("kind", Value::Str(handler.kind.as_str().into())),
                            ("required_args", strings(required[name].iter().cloned())),
                            ("timeout_ms", Value::Num(handler.timeout_ms.to_string())),
                            ("on_ok", advance(handler.on_ok.as_ref())),
                            ("on_failed", advance(handler.on_failed.as_ref())),
                            (
                                "retry",
                                object([
                                    ("attempts", Value::Num(handler.retry.attempts.to_string())),
                                    (
                                        "backoff_ms",
                                        Value::Num(handler.retry.backoff_ms.to_string()),
                                    ),
                                    (
                                        "max_backoff_ms",
                                        Value::Num(handler.retry.max_backoff_ms.to_string()),
                                    ),
                                    ("on", strings(handler.retry.on.iter().cloned())),
                                ]),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let mut bytes = b"fsm:executor-contract:1\0".to_vec();
    bytes.extend(canon_bytes(&metadata));
    to_hex(&sha256(&bytes))
}

/// Analyze every emit in the root and supplied static invocation closure.
///
/// Catalogue keys are 64-hex child digests; values must carry the matching
/// compiled identity. Only parsed tables and compiled definitions are inputs.
/// This effects-only phase leaves configured outcome validation explicitly
/// unknown; it never performs execution admission or reads a clock.
pub fn analyze_effects(
    root: &CompiledMachine,
    catalogue: &BTreeMap<String, CompiledMachine>,
    table: &HandlerTable,
    limits: Limits,
) -> Result<Report, ExecError> {
    analyze(root, catalogue, table, limits, true)
}

pub(super) fn analyze(
    root: &CompiledMachine,
    catalogue: &BTreeMap<String, CompiledMachine>,
    table: &HandlerTable,
    limits: Limits,
    defer_outcomes: bool,
) -> Result<Report, ExecError> {
    let ceiling = Limits::default();
    if limits.definitions > ceiling.definitions
        || limits.sites > ceiling.sites
        || limits.findings > ceiling.findings
        || limits.report_bytes > ceiling.report_bytes
    {
        return Err(limit());
    }
    let required = table
        .handlers
        .iter()
        .map(|(name, handler)| (name.clone(), handler.required_args()))
        .collect();
    let report = Report {
        status: CheckStatus::Compatible,
        machine_id: Some(root.machine_id.clone()),
        contract_id: Some(identity(table, &required)),
        definitions: BTreeSet::new(),
        effects_checked: true,
        outcomes_checked: !defer_outcomes,
        dynamic_signals: false,
        findings: Vec::new(),
        effects: Vec::new(),
        progress: BTreeSet::new(),
    };
    let mut walker = Walker {
        table,
        limits,
        report,
        required,
        site_bytes: 0,
        finding_bytes: 0,
        defer_outcomes,
    };
    let mut queue = VecDeque::from([root]);
    let mut queued = BTreeSet::from([root.machine_id.clone()]);
    if limits.definitions == 0 {
        return Err(limit());
    }
    while let Some(machine) = queue.pop_front() {
        walker.report.definitions.insert(machine.machine_id.clone());
        for (digest, path) in walker.machine(machine)? {
            let child = if root
                .machine_id
                .rsplit_once("@sha256:")
                .map(|(_, hash)| hash)
                == Some(digest.as_str())
            {
                Some(root)
            } else {
                catalogue.get(&digest)
            };
            let child = child.filter(|child| {
                child
                    .machine_id
                    .rsplit_once("@sha256:")
                    .map(|(_, hash)| hash)
                    == Some(digest.as_str())
            });
            if let Some(child) = child {
                if !queued.contains(&child.machine_id) {
                    if queued.len() >= limits.definitions {
                        return Err(limit());
                    }
                    queued.insert(child.machine_id.clone());
                    queue.push_back(child);
                }
            } else {
                walker.finding(
                    machine,
                    &path,
                    None,
                    "exec/contract_definition_unknown",
                    "unknown",
                    "a static invocation has no matching supplied definition",
                    "supply the content-addressed child definition before checking execution",
                    None,
                )?;
            }
        }
        walker.bytes()?;
    }
    walker.report.effects.sort_by(|left, right| {
        (&left.machine_id, &left.path, &left.effect).cmp(&(
            &right.machine_id,
            &right.path,
            &right.effect,
        ))
    });
    walker.report.findings.sort_by(|left, right| {
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
    walker.bytes()?;
    Ok(walker.report)
}

struct Walker<'a> {
    table: &'a HandlerTable,
    limits: Limits,
    report: Report,
    required: BTreeMap<String, BTreeSet<String>>,
    site_bytes: usize,
    finding_bytes: usize,
    defer_outcomes: bool,
}

impl Walker<'_> {
    fn bytes(&self) -> Result<(), ExecError> {
        let size = canon_bytes(&self.report.envelope())
            .len()
            .checked_add(self.site_bytes)
            .and_then(|size| size.checked_add(self.finding_bytes))
            .and_then(|size| size.checked_add(self.report.effects.len().saturating_sub(1)))
            .and_then(|size| size.checked_add(self.report.findings.len().saturating_sub(1)))
            .ok_or_else(limit)?;
        if size > self.limits.report_bytes {
            Err(limit())
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)] // One finding's complete semantic context.
    fn finding(
        &mut self,
        machine: &CompiledMachine,
        path: &str,
        effect: Option<&str>,
        code: &'static str,
        severity: &'static str,
        message: &str,
        hint: &str,
        outcome: Option<&str>,
    ) -> Result<(), ExecError> {
        if self.report.findings.len() >= self.limits.findings {
            return Err(limit());
        }
        if severity == "error" {
            self.report.status = CheckStatus::Invalid;
        } else if severity == "unknown" && self.report.status != CheckStatus::Invalid {
            self.report.status = CheckStatus::Unknown;
        }
        let finding = Finding {
            code,
            severity,
            machine_id: machine.machine_id.clone(),
            path: path.into(),
            effect: effect.map(str::to_owned),
            outcome: outcome.map(str::to_owned),
            message: message.into(),
            hint: hint.into(),
            cause: None,
        };
        self.finding_bytes = self
            .finding_bytes
            .checked_add(canon_bytes(&finding.to_value()).len())
            .ok_or_else(limit)?;
        self.report.findings.push(finding);
        Ok(())
    }

    fn emits(
        &mut self,
        machine: &CompiledMachine,
        emits: &[EmitSpec],
        path: &str,
        slot: impl Fn(usize, &str) -> ExprSlot,
    ) -> Result<(), ExecError> {
        for (index, emit) in emits.iter().enumerate() {
            if self.report.effects.len() >= self.limits.sites {
                return Err(limit());
            }
            let path = format!("{path}/emit/{index}");
            let mut arguments = BTreeMap::new();
            for key in emit.args.keys() {
                let inferred = machine
                    .compiled_exprs
                    .get(&slot(index, key))
                    .map(|expression| expression.ty.to_string());
                if inferred.is_none() {
                    self.finding(
                        machine,
                        &format!("{path}/args/{}", key.replace('~', "~0").replace('/', "~1")),
                        Some(&emit.effect),
                        "exec/contract_argument_unknown",
                        "unknown",
                        "an emitted argument has no authoritative compiled type",
                        "compile this definition with its complete expression environment",
                        None,
                    )?;
                }
                arguments.insert(key.clone(), inferred);
            }
            let handler = self.table.handlers.get(&emit.effect);
            let required_args = self.required.get(&emit.effect).cloned().unwrap_or_default();
            let disposition = if handler.is_some() {
                "automatic"
            } else if self.table.manual_effects.contains(&emit.effect) {
                "manual"
            } else {
                "missing"
            };
            if disposition == "missing" {
                self.finding(
                    machine,
                    &path,
                    Some(&emit.effect),
                    "exec/contract_handler_missing",
                    "error",
                    "an emitted effect has neither automatic nor manual disposition",
                    "configure an operator handler or explicitly classify this effect as manual",
                    None,
                )?;
            }
            for required in &required_args {
                if !emit.args.contains_key(required) {
                    self.finding(machine, &format!("{path}/args/{}", required.replace('~', "~0").replace('/', "~1")), Some(&emit.effect),
                        "exec/contract_argument_missing", "error", "an emit omits an argument required by its handler",
                        "supply the required argument at this emit site or correct the operator template", None)?;
                }
            }
            let mut outcomes = BTreeMap::new();
            if let Some(handler) = handler {
                for (name, configured) in [
                    ("on_ok", handler.on_ok.as_ref()),
                    ("on_failed", handler.on_failed.as_ref()),
                ] {
                    if configured.is_some() {
                        outcomes.insert(name.into(), "unknown");
                        if self.defer_outcomes {
                            self.finding(
                            machine,
                            &path,
                            Some(&emit.effect),
                            "exec/contract_unknown",
                            "unknown",
                            "configured outcome validation is outside this effect-analysis phase",
                            "run complete outcome validation before authorizing external execution",
                            Some(name),
                        )?;
                        }
                    } else {
                        outcomes.insert(name.into(), "no-outcome");
                        self.report.progress.insert("no-outcome".into());
                    }
                }
                self.report.progress.insert("runtime-dependent".into());
            } else if disposition == "manual" {
                self.report.progress.insert("manual".into());
            }
            let site = EffectSite {
                machine_id: machine.machine_id.clone(),
                path,
                effect: emit.effect.clone(),
                arguments,
                disposition,
                required_args,
                outcomes,
            };
            self.site_bytes = self
                .site_bytes
                .checked_add(canon_bytes(&site.to_value()).len())
                .ok_or_else(limit)?;
            self.report.effects.push(site);
            self.bytes()?;
        }
        Ok(())
    }

    fn machine(&mut self, machine: &CompiledMachine) -> Result<Vec<(String, String)>, ExecError> {
        let mut invocations = Vec::new();
        let mut states: Vec<(&StateNode, String)> = match &machine.spec.topology {
            Topology::Sequential { states, .. } => states
                .iter()
                .enumerate()
                .map(|(index, node)| (node, format!("/states/{index}")))
                .collect(),
            Topology::Parallel { regions } => regions
                .iter()
                .enumerate()
                .flat_map(|(region, spec)| {
                    spec.states.iter().enumerate().map(move |(index, node)| {
                        (node, format!("/regions/{region}/states/{index}"))
                    })
                })
                .collect(),
        };
        // Set scope before charging any serialized prefix: false -> true is
        // one byte shorter and must not reject an exact final-byte boundary.
        let mut signal_scan: Vec<&StateNode> = states.iter().map(|(node, _)| *node).collect();
        while let Some(node) = signal_scan.pop() {
            self.report.dynamic_signals |= node
                .entry
                .as_ref()
                .is_some_and(|block| !block.signals.is_empty())
                || node
                    .exit
                    .as_ref()
                    .is_some_and(|block| !block.signals.is_empty());
            signal_scan.extend(&node.states);
        }
        self.report.dynamic_signals |= machine
            .spec
            .transitions
            .iter()
            .any(|transition| !transition.signals.is_empty())
            || machine
                .spec
                .deadlines
                .iter()
                .any(|deadline| !deadline.signals.is_empty());
        while let Some((node, path)) = states.pop() {
            for (index, child) in node.states.iter().enumerate() {
                states.push((child, format!("{path}/states/{index}")));
            }
            for (index, invoke) in node.invokes.iter().enumerate() {
                invocations.push((
                    invoke.machine.clone(),
                    format!("{path}/invoke/{index}/machine"),
                ));
            }
            if let Some(block) = &node.entry {
                self.report.dynamic_signals |= !block.signals.is_empty();
                self.emits(
                    machine,
                    &block.emits,
                    &format!("{path}/entry"),
                    |index, key| ExprSlot::StateEntryEmitArg(node.name.clone(), index, key.into()),
                )?;
            }
            if let Some(block) = &node.exit {
                self.report.dynamic_signals |= !block.signals.is_empty();
                self.emits(
                    machine,
                    &block.emits,
                    &format!("{path}/exit"),
                    |index, key| ExprSlot::StateExitEmitArg(node.name.clone(), index, key.into()),
                )?;
            }
        }
        for (index, transition) in machine.spec.transitions.iter().enumerate() {
            self.report.dynamic_signals |= !transition.signals.is_empty();
            self.emits(
                machine,
                &transition.emits,
                &format!("/transitions/{index}"),
                |emit, key| ExprSlot::TransitionEmitArg(index, emit, key.into()),
            )?;
        }
        for (index, deadline) in machine.spec.deadlines.iter().enumerate() {
            self.report.dynamic_signals |= !deadline.signals.is_empty();
            self.emits(
                machine,
                &deadline.emits,
                &format!("/deadlines/{index}"),
                |emit, key| ExprSlot::DeadlineEmitArg(index, emit, key.into()),
            )?;
        }
        if self.report.dynamic_signals {
            self.report.progress.insert("runtime-dependent".into());
        }
        Ok(invocations)
    }
}
