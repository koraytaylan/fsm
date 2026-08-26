//! Admission checks for a `supersedes` mapping: everything decidable once
//! both definitions are in hand.
//!
//! An operator should learn their mapping is wrong when they write it, not
//! when they try to move a live workflow with it — so these run at
//! `define_machine`, before a single instance is at risk. What they answer is
//! "is this mapping coherent"; whether a *particular* instance can move is a
//! run-time question and reports `req/migrate_*`.
//!
//! Plan 0011 task 5302.

use std::collections::BTreeSet;

use crate::expr::parser;
use crate::expr::typeck::{Scope, ScopeKind, Ty, typecheck};
use crate::machine::CompiledMachine;
use crate::spec::Finding;
use crate::tree::{NodeKind, Tree};

/// Every catalogue-dependent `supersedes` rule, in a stable order: the state
/// mapping in document order, then the context mapping, then the slots.
pub fn validate_supersedes(
    old: &CompiledMachine,
    tree_old: &Tree,
    new: &CompiledMachine,
    tree_new: &Tree,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(supersedes) = &new.spec.supersedes else {
        return findings;
    };

    // Region topology is not mappable, and this plan does not pretend
    // otherwise: an instance's configuration is a set of leaves keyed by
    // region, so two machines that disagree on the region set disagree on
    // what an instance even is.
    let regions = |tree: &Tree| -> Vec<String> {
        tree.root_initials
            .iter()
            .filter_map(|(region, _)| region.clone())
            .collect()
    };
    let (old_regions, new_regions) = (regions(tree_old), regions(tree_new));
    if old_regions != new_regions {
        findings.push(Finding::err(
            "def/supersedes_region",
            "/supersedes",
            format!(
                "the superseded machine has regions {old_regions:?} and this one has {new_regions:?}"
            ),
            "keep the region names and shape identical across a migration, or migrate by \
             cancelling and re-creating: region topology is not mappable",
        ));
    }

    for (index, (from, to)) in supersedes.states.iter().enumerate() {
        let path = format!("/supersedes/states/{from}");
        let _ = index;
        if !tree_old.index.contains_key(from) {
            findings.push(Finding::err(
                "def/supersedes_unknown_state",
                &path,
                format!("the superseded machine has no state {from}"),
                "map a state the old definition actually has",
            ));
        }
        let Some(target) = tree_new.index.get(to).copied() else {
            findings.push(Finding::err(
                "def/supersedes_unknown_state",
                &path,
                format!("this machine has no state {to}"),
                "map onto a state this definition declares",
            ));
            continue;
        };
        // An active configuration only ever holds leaves, so a mapping onto a
        // compound or a history pseudostate names something an instance can
        // never be *in*.
        if !matches!(tree_new.kind[target as usize], NodeKind::Leaf) {
            findings.push(Finding::err(
                "def/supersedes_target_not_leaf",
                &path,
                format!("{to} is not a leaf state"),
                "map onto the leaf the instance should be in, not its parent or a history node",
            ));
            continue;
        }
        if is_terminal(new, to) || tree_new.final_owner[target as usize].is_some() {
            findings.push(Finding::err(
                "def/supersedes_target_terminal",
                &path,
                format!("{to} ends the instance or its compound"),
                "map onto a state the instance can continue from; completing a workflow by \
                 migrating it hides the completion from its own history",
            ));
        }
    }

    // A context expression is typed with the **old** machine's context in
    // scope and the **new** machine's variable as the assignment target: it
    // reads what the instance has and writes what the instance will have.
    for (name, source) in &supersedes.context {
        let path = format!("/supersedes/context/{name}");
        let Some(declared) = new.spec.context.iter().find(|var| var.name == *name) else {
            findings.push(Finding::err(
                "def/supersedes_ctx_unknown",
                &path,
                format!("this machine has no context variable {name}"),
                "map a variable this definition declares",
            ));
            continue;
        };
        match type_in_old_scope(source, old) {
            Err(error) => findings.push(match error.code {
                // A name the old machine does not declare is the mapping's
                // mistake, not an expression bug: it is reported as such so
                // an operator reads one story, not two.
                "expr/unknown_field" | "expr/unknown_var" | "expr/unknown_ident" => Finding::err(
                    "def/supersedes_ctx_unknown",
                    &path,
                    error.message.clone(),
                    "read a variable the superseded definition declares: the expression sees \
                     the old machine's context, which is what the instance holds",
                ),
                code => Finding::err(code, &path, error.message.clone(), error.hint.clone()),
            }),
            Ok(got) => {
                let want = declared.ty.to_ty();
                if got != want {
                    findings.push(Finding::err(
                        "def/supersedes_ctx_type",
                        &path,
                        format!("{name} is {want} and the expression is {got}"),
                        "match the declared type exactly, decimal scale included",
                    ));
                }
            }
        }
    }

    // A slot the old machine has and the new one does not is work an instance
    // could be holding with nowhere to put it.
    let slots = |machine: &CompiledMachine| -> BTreeSet<String> {
        machine
            .spec
            .walk_states()
            .into_iter()
            .flat_map(|(node, _)| node.invokes.iter().map(|invoke| invoke.id.clone()))
            .collect()
    };
    for slot in slots(old).difference(&slots(new)) {
        findings.push(Finding::err(
            "def/supersedes_slot",
            "/supersedes",
            format!("the superseded machine has an invoke slot {slot} this one does not"),
            "declare the slot here too, or migrate only instances that hold no invocation",
        ));
    }
    findings
}

/// Whether a state is declared terminal in this machine.
fn is_terminal(machine: &CompiledMachine, name: &str) -> bool {
    machine
        .spec
        .walk_states()
        .into_iter()
        .any(|(node, _)| node.name == name && node.terminal)
}

/// Type one `context` expression with the **old** machine's context in scope.
///
/// A migration expression reads what the instance holds today and writes what
/// it will hold tomorrow, so the reading half is typed against the definition
/// the instance is still on.
fn type_in_old_scope(source: &str, old: &CompiledMachine) -> Result<Ty, crate::expr::ExprError> {
    let expression = parser::parse(source)?;
    let ctx: std::collections::BTreeMap<String, Ty> = old
        .spec
        .context
        .iter()
        .map(|var| (var.name.clone(), var.ty.to_ty()))
        .collect();
    let states: BTreeSet<String> = old
        .spec
        .walk_states()
        .into_iter()
        .map(|(node, _)| node.name.clone())
        .collect();
    let scope = Scope {
        // A migration expression is a context assignment: no event in scope,
        // which is what `ScopeKind::Block` already means here.
        kind: ScopeKind::Block,
        ctx: &ctx,
        evt: None,
        enums: &old.spec.enums,
        states: &states,
    };
    typecheck(&expression, &scope).map(|(ty, _, _)| ty)
}
