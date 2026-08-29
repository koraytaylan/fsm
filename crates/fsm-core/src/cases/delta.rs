//! Running one case against two definitions and reporting what moved.
//!
//! # Why this is the reason to keep case files
//!
//! A definition that declares `supersedes` is making a checkable claim: that
//! it is a corrected version of a specific earlier machine. The earlier
//! machine's cases are what check it. Run them against the new definition, map
//! the expected configurations through the mapping the new definition already
//! declares, and report which outcomes moved — and a migration becomes a
//! **reviewed diff** rather than a hope, in the register plan 0011's
//! `--dry-run` established for instances.
//!
//! # It is a report and never a gate
//!
//! A corrected machine usually changes behaviour on purpose. A rule forbidding
//! that would be wrong, and a gate with an override is a gate everyone
//! overrides. So a completed run reports its deltas and succeeds; only an
//! actual failure to run — a definition that does not compile, a missing
//! mapping — is an error.
//!
//! # The mapping is plan 0011's, not a second copy
//!
//! Translating an expected leaf goes through [`crate::migrate::preview`]: the
//! old expectation is materialized as an instance sitting at that leaf and
//! previewed exactly as `migrate --dry-run` previews a real one. A report that
//! disagreed with what an actual migration would do is worse than no report,
//! and two implementations of one mapping will eventually disagree.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use crate::cases::expect::{Divergence, diverge};
use crate::cases::format::{Case, Expect};
use crate::cases::run::{CaseError, run_case};
use crate::machine::CompiledMachine;
use crate::tree::Tree;

/// What running one case against the superseding definition showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The new definition does what the old one did, under the mapping.
    Unchanged,
    /// It behaves differently, and these are the fields that moved.
    ///
    /// Not a failure: this is usually the point of the correction.
    Changed(Vec<Divergence>),
    /// The new definition rejects a script the old one accepted.
    Refused {
        /// Absent when the case has no script to refuse.
        step: Option<usize>,
        detail: String,
    },
    /// The expectation names a state the mapping does not cover.
    ///
    /// The same gap `migrate --dry-run` reports for instances, met here before
    /// any instance moves — which is the whole reason to look now.
    Uncovered { state: String, detail: String },
}

impl Outcome {
    /// The word a structured report uses, so three outcomes stay three.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Changed(_) => "changed",
            Self::Refused { .. } => "refused",
            Self::Uncovered { .. } => "uncovered",
        }
    }
}

/// One case's delta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseDelta {
    pub name: String,
    /// The expectation as it was translated through the mapping, so a reader
    /// sees what was actually compared rather than what the file said.
    pub translated: Vec<String>,
    pub outcome: Outcome,
}

/// Why a delta run could not start at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaError {
    /// The new definition declares no `supersedes`.
    NotSuperseding,
    /// It declares one, naming a different machine than the old definition.
    SupersedesOther { declared: String, given: String },
}

impl DeltaError {
    pub fn message(&self) -> String {
        match self {
            Self::NotSuperseding => concat!(
                "the new definition declares no `supersedes`, so there is no mapping between ",
                "these two machines and nothing here would mean anything: without it these are ",
                "two unrelated definitions that happen to share a case file",
            )
            .to_string(),
            Self::SupersedesOther { declared, given } => format!(
                "the new definition supersedes {declared}, and the definition given is {given}; \
                 the mapping is what makes this comparison meaningful and it is not for this pair"
            ),
        }
    }
}

/// Check that the pair is actually a supersession before comparing anything.
pub fn require_mapping(old: &CompiledMachine, new: &CompiledMachine) -> Result<(), DeltaError> {
    let Some(supersedes) = &new.spec.supersedes else {
        return Err(DeltaError::NotSuperseding);
    };
    // The block names the bare digest, exactly as `migrate` compares it. This
    // is the same check `apply::attempt` makes and it is made here so the
    // command refuses before running anything, rather than reporting every
    // case as uncovered for one reason.
    if crate::hashes::digest_of(&old.machine_id) != Some(supersedes.machine.as_str()) {
        return Err(DeltaError::SupersedesOther {
            declared: supersedes.machine.clone(),
            given: old.machine_id.clone(),
        });
    }
    Ok(())
}

/// Translate an expected configuration through the declared mapping.
///
/// Goes through [`crate::migrate::apply::mapped_leaf`] — *the* leaf mapping, the one
/// `attempt` calls — so this report and an actual migration cannot disagree
/// about which states are covered.
///
/// It deliberately does **not** go through `migrate::preview`. A preview
/// refuses a settled instance, which is correct for an instance and wrong
/// here: a case's expected configuration is very often terminal, and "the
/// migration would refuse to move an instance sitting there" is not an answer
/// to "where does this state map to".
fn translate(
    old: &CompiledMachine,
    tree_old: &Tree,
    new: &CompiledMachine,
    leaves: &[String],
) -> Result<Vec<String>, Outcome> {
    let mut mapped = Vec::new();
    for leaf in leaves {
        // The region is the old tree's answer, so a parallel machine's leaf is
        // mapped with the same region context a migration would give it.
        let region = tree_old
            .id(leaf)
            .and_then(|id| tree_old.region_of(id))
            .map(str::to_string);
        match crate::migrate::apply::mapped_leaf(new, leaf, region.as_deref()) {
            Ok(target) => mapped.push(target),
            Err(rejection) => {
                return Err(Outcome::Uncovered {
                    state: leaf.clone(),
                    detail: format!("{}: {}", rejection.code, rejection.message),
                });
            }
        }
    }
    // A mapping that names a state the new definition does not have would map
    // an expectation onto nothing, and that is a gap too.
    let tree_new = Tree::for_machine(&new.spec);
    // Paired with the source leaf, because the source is what the author has
    // to fix in the `supersedes` block. Naming the *target* told them a state
    // they never wrote, and when several old leaves map onto one missing
    // target the report could not be worked backwards at all.
    for (source, target) in leaves.iter().zip(mapped.iter()) {
        if tree_new.id(target).is_none() {
            return Err(Outcome::Uncovered {
                state: source.clone(),
                detail: format!(
                    "the mapping sends {source} to {target}, which the new definition does not \
                     declare"
                ),
            });
        }
    }
    let _ = old;
    // A mapping may merge two old leaves onto one new state. `configuration`
    // compares as a set, so the translated expectation has to be one too —
    // otherwise `[x, x]` against an observed `[x]` reports a delta the
    // migration would not produce.
    mapped.sort();
    mapped.dedup();
    Ok(mapped)
}

/// Run one case against both definitions and classify the result.
pub fn delta(
    old: &CompiledMachine,
    tree_old: &Tree,
    new: &CompiledMachine,
    tree_new: &Tree,
    case: &Case,
) -> CaseDelta {
    // The expectation is the old machine's committed behaviour, so its
    // configuration is stated in the old machine's names and has to be
    // translated before it can be compared against anything.
    let mut translated = Vec::new();
    let mut expect = case.expect.clone();
    if let Some(leaves) = &case.expect.configuration {
        match translate(old, tree_old, new, leaves) {
            Ok(mapped) => {
                translated = mapped.clone();
                expect.configuration = Some(mapped);
            }
            Err(outcome) => {
                return CaseDelta {
                    name: case.name.clone(),
                    translated,
                    outcome,
                };
            }
        }
    }
    match run_case(new, tree_new, case) {
        Err(error) => CaseDelta {
            name: case.name.clone(),
            translated,
            outcome: Outcome::Refused {
                step: None,
                detail: match error {
                    CaseError::Context { key, message } => format!("context {key}: {message}"),
                    CaseError::Create(rejection) => {
                        format!("{}: {}", rejection.code, rejection.message)
                    }
                },
            },
        },
        Ok(run) => {
            let divergences = diverge(&expect, &run);
            // A script step the new definition would not run at all is a
            // different thing from an outcome that moved, and an author reads
            // it differently: one says "your machine no longer accepts this",
            // the other says "it accepts it and does something else".
            if let Some(refusal) = divergences.iter().find(|d| d.field == "script") {
                return CaseDelta {
                    name: case.name.clone(),
                    translated,
                    outcome: Outcome::Refused {
                        step: refusal.step,
                        detail: refusal.found.clone(),
                    },
                };
            }
            CaseDelta {
                name: case.name.clone(),
                translated,
                outcome: if divergences.is_empty() {
                    Outcome::Unchanged
                } else {
                    Outcome::Changed(divergences)
                },
            }
        }
    }
}

/// The whole case file, as a delta.
pub fn delta_all(
    old: &CompiledMachine,
    tree_old: &Tree,
    new: &CompiledMachine,
    tree_new: &Tree,
    cases: &[&Case],
) -> Vec<CaseDelta> {
    cases
        .iter()
        .map(|case| delta(old, tree_old, new, tree_new, case))
        .collect()
}

/// How many cases fell into each outcome, for the summary line.
pub fn tally(deltas: &[CaseDelta]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::from([
        ("unchanged", 0),
        ("changed", 0),
        ("refused", 0),
        ("uncovered", 0),
    ]);
    for delta in deltas {
        *counts.entry(delta.outcome.name()).or_insert(0) += 1;
    }
    counts
}

/// Kept so a caller can build an expectation from a translated configuration
/// without reaching into this module's internals.
pub fn translated_expect(expect: &Expect, configuration: Vec<String>) -> Expect {
    Expect {
        configuration: Some(configuration),
        ..expect.clone()
    }
}
