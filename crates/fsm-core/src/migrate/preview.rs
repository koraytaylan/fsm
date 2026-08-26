//! What a migration *would* do, answered before anything is written.
//!
//! A preview runs every one of the apply's steps, reaction phase included:
//! `migrate` runs the reaction, so a preview that stopped before it would
//! report a configuration the migration never lands on. Both call the same
//! attempt, which is what makes preview/apply agreement a property of the
//! code rather than of two implementations staying in step.
//!
//! A refusal is **returned, not raised**: "this one cannot migrate, and here
//! is the code" is exactly the information the caller asked for.
//!
//! Plan 0011 task 5403.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState};
use crate::step::Rejection;
use crate::tree::Tree;

use super::apply::MigrationReport;

/// What one instance's migration would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationPreview {
    /// The configuration the mapping produces, before any reaction — absent
    /// when the attempt refused before it got that far.
    pub mapped_configuration: Option<ActiveConfiguration>,
    /// The configuration after quiescence: where the instance actually lands.
    pub settled_configuration: Option<ActiveConfiguration>,
    /// Every context variable, as it is now and as it would be. A reader sees
    /// what changes rather than only the result.
    pub context: Vec<(String, Option<String>, Option<String>)>,
    /// Everything the attempt determined, in the apply's own vocabulary.
    pub report: MigrationReport,
    /// The refusal this instance would meet, if any.
    pub refusal: Option<Rejection>,
}

impl MigrationPreview {
    /// Whether this instance would migrate.
    pub fn clean(&self) -> bool {
        self.refusal.is_none()
    }
}

/// Preview one instance's migration.
pub fn preview(
    from: &CompiledMachine,
    to: &CompiledMachine,
    tree_to: &Tree,
    st: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> MigrationPreview {
    let attempt = super::apply::attempt(from, to, tree_to, st, now_ms, budget);
    let (settled, refusal) = match attempt.outcome {
        Ok(state) => (Some(state.configuration), None),
        Err(rejection) => (None, Some(rejection)),
    };
    let after: BTreeMap<&str, &str> = attempt
        .report
        .projected
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let mut names: Vec<String> = st.ctx.keys().cloned().collect();
    names.extend(to.spec.context.iter().map(|var| var.name.clone()));
    names.sort();
    names.dedup();
    let context = names
        .into_iter()
        .map(|name| {
            let before = st
                .ctx
                .get(&name)
                .map(crate::expr::eval::Val::canonical_string);
            // A variable the mapping projected shows its computed value; one
            // that took its declared init shows that; one the new definition
            // does not declare shows nothing, which is how a reader sees a
            // drop.
            let value = after.get(name.as_str()).map(|value| (*value).to_string());
            let value = value.or_else(|| {
                to.spec
                    .context
                    .iter()
                    .find(|var| var.name == name)
                    .map(|var| var.init.clone())
            });
            (name, before, value)
        })
        .collect();
    MigrationPreview {
        mapped_configuration: attempt.mapped_configuration,
        settled_configuration: settled,
        context,
        report: attempt.report,
        refusal,
    }
}

/// One outcome and the instances that share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewGroup {
    /// The refusal code, or `None` for the instances that migrate cleanly.
    pub code: Option<&'static str>,
    /// The active state the refusal names, when the code is about one.
    pub detail: String,
    /// How many instances share this outcome.
    pub count: usize,
    /// Their ids, in the order the caller supplied them.
    pub instances: Vec<String>,
}

/// Preview a cohort, grouped by outcome.
///
/// An operator sees "412 migrate cleanly, 8 are in a state your map does not
/// cover" rather than discovering the eight one at a time. Groups are ordered
/// by descending count, then by code, so the summary is stable and the
/// biggest cohort reads first.
pub fn preview_all(
    from: &CompiledMachine,
    to: &CompiledMachine,
    tree_to: &Tree,
    instances: &[(String, InstanceState)],
    now_ms: i64,
    budget: &mut Budget,
) -> Vec<PreviewGroup> {
    let mut groups: BTreeMap<(Option<&'static str>, String), Vec<String>> = BTreeMap::new();
    for (id, state) in instances {
        let outcome = preview(from, to, tree_to, state, now_ms, budget);
        let key = match &outcome.refusal {
            None => (None, String::new()),
            Some(rejection) => (Some(rejection.code), rejection.message.clone()),
        };
        groups.entry(key).or_default().push(id.clone());
    }
    let mut out: Vec<PreviewGroup> = groups
        .into_iter()
        .map(|((code, detail), instances)| PreviewGroup {
            code,
            detail,
            count: instances.len(),
            instances,
        })
        .collect();
    out.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.code.cmp(&b.code))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    out
}
