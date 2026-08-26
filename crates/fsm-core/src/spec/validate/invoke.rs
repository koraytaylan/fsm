//! The `invoke` rules that need the child definitions in hand.
//!
//! `validate/reactive.rs` holds the rules a definition decides alone; these
//! five need a catalogue, so they run where one exists — the store's
//! `define_machine`, which refuses a definition invoking a machine it does
//! not hold. Keeping them apart is what lets `fsm-core` stay a pure function
//! of one document while composition stays checkable.
//!
//! Plan 0010 task 4901.

use std::collections::{BTreeMap, BTreeSet};

use crate::limits::MAX_INVOKE_DEPTH;
use crate::machine::{CompiledMachine, ExprSlot};

use super::super::{Finding, MachineSpec};

/// The invoked machines a definition can see, keyed by the 64-hex digest its
/// `invoke` slots name.
pub type Catalogue = BTreeMap<String, MachineSpec>;

/// The five catalogue-dependent `invoke` rules, as findings in document order.
///
/// The graph is finite and immutable precisely because `machine` is a content
/// hash: a definition cannot acquire new edges after admission, so a
/// depth-first walk decides both the cycle and the depth question once.
pub fn validate_catalogue(compiled: &CompiledMachine, catalogue: &Catalogue) -> Vec<Finding> {
    let spec = &compiled.spec;
    let mut findings = Vec::new();
    for (node, _) in spec.walk_states() {
        for (index, invoke) in node.invokes.iter().enumerate() {
            let path = format!("/states/{}/invoke/{index}", node.name);
            let Some(child) = catalogue.get(&invoke.machine) else {
                findings.push(Finding::err(
                    "def/invoke_unknown_machine",
                    format!("{path}/machine"),
                    format!("invoke slot {} names a machine this store does not hold", invoke.id),
                    "define the child machine first: a content-addressed reference can only be checked against a definition that exists",
                ));
                continue;
            };
            let declared = |name: &str| child.context.iter().find(|c| c.name == name);
            for (field, _source) in &invoke.with {
                let Some(target) = declared(field) else {
                    findings.push(Finding::err(
                        "def/invoke_unknown_ctx",
                        format!("{path}/with/{field}"),
                        format!("{} declares no context variable {field}", child.name),
                        format!(
                            "project into one of the child's context variables: {}",
                            names(child)
                        ),
                    ));
                    continue;
                };
                let Some(expression) = compiled.compiled_exprs.get(&ExprSlot::InvokeWith(
                    node.name.clone(),
                    index,
                    field.clone(),
                )) else {
                    continue;
                };
                let want = target.ty.to_ty();
                if expression.ty != want {
                    findings.push(Finding::err(
                        "def/invoke_type",
                        format!("{path}/with/{field}"),
                        format!(
                            "cannot project {} into {}.{field} ({want}): the expression is {}",
                            expression.source, child.name, expression.ty
                        ),
                        "make the scale and class match the child's declaration exactly",
                    ));
                }
            }
            for (field, child_var) in &invoke.returns {
                if declared(child_var).is_none() {
                    findings.push(Finding::err(
                        "def/invoke_unknown_ctx",
                        format!("{path}/returns/{field}"),
                        format!("{} declares no context variable {child_var}", child.name),
                        format!(
                            "project one of the child's context variables: {}",
                            names(child)
                        ),
                    ));
                }
            }
        }
    }
    findings.extend(walk_graph(
        spec,
        crate::hashes::digest_of(&compiled.machine_id).unwrap_or_default(),
        catalogue,
    ));
    findings
}

fn names(spec: &MachineSpec) -> String {
    spec.context
        .iter()
        .map(|c| c.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Depth-first over the invocation graph, reporting the first cycle and the
/// first over-deep chain. Depth counts machines, so a chain of
/// `MAX_INVOKE_DEPTH` machines is the deepest legal one.
///
/// The walk keys on the **digest**, never the name: two machines may share a
/// name and differ in content, and treating that as a cycle would refuse a
/// perfectly ordinary revision invoking its predecessor.
///
/// `def/invoke_cycle` is defence in depth rather than a rule a definition can
/// break today: a cycle would need each machine's digest inside the other's
/// document, which is a hash preimage cycle. It stays because the graph's
/// acyclicity is a property of content addressing, and a later plan that
/// resolves a slot any other way would silently lose it.
fn walk_graph(spec: &MachineSpec, digest: &str, catalogue: &Catalogue) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut deepest = 1usize;
    let mut cycle_reported = false;
    let mut stack: Vec<(&MachineSpec, usize, BTreeSet<String>)> =
        vec![(spec, 1, BTreeSet::from([digest.to_string()]))];
    while let Some((current, depth, seen)) = stack.pop() {
        deepest = deepest.max(depth);
        for (node, _) in current.walk_states() {
            for invoke in &node.invokes {
                let Some(child) = catalogue.get(&invoke.machine) else {
                    continue;
                };
                if seen.contains(&invoke.machine) {
                    if !cycle_reported {
                        cycle_reported = true;
                        findings.push(Finding::err(
                            "def/invoke_cycle",
                            format!("/states/{}/invoke", node.name),
                            format!(
                                "invoking {} closes a cycle back to {}",
                                child.name, current.name
                            ),
                            "a workflow cannot invoke itself, directly or through the machines it invokes; break the cycle or model the repetition inside one machine",
                        ));
                    }
                    continue;
                }
                let mut inner = seen.clone();
                inner.insert(invoke.machine.clone());
                stack.push((child, depth + 1, inner));
            }
        }
    }
    if deepest > MAX_INVOKE_DEPTH {
        findings.push(Finding::err(
            "def/invoke_depth",
            "/states",
            format!("the invocation graph is {deepest} machines deep"),
            format!(
                "at most {MAX_INVOKE_DEPTH} machines deep, counting this one; flatten a level or merge two machines"
            ),
        ));
    }
    findings
}
