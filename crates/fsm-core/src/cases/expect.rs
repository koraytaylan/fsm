//! Comparing what a case expected against what its run observed.
//!
//! # The ordering rules are asymmetric on purpose
//!
//! A reader will assume all four fields compare the same way. They do not, and
//! each follows the engine's own rule rather than a convention chosen here:
//!
//! * **`effects` compare in emission order.** That order is deterministic and
//!   load-bearing — an executor runs them in it — so a case that pins it is
//!   pinning something real.
//! * **`configuration` compares as a set.** A configuration *is* a set of
//!   active leaves, and parallel regions make any list order an artefact of
//!   how it was written down.
//! * **`enabled` compares as a set.** It derives from a scan whose order the
//!   spec deliberately does not fix, so pinning it would pin an implementation
//!   detail.
//! * **`context` compares key by key**, reporting each key that differs rather
//!   than two whole maps an author has to diff by eye.
//!
//! # Context compares through the canonical string, and that is the point
//!
//! A decimal's scale is part of its value in this engine: `10.0` and `10.00`
//! are different, exact arithmetic is the reason, and a comparison that
//! coerced one to the other would hide the difference a case exists to catch.
//! The canonical string is total and exact, so comparing through it reports a
//! scale change as the change it is and shows both scales.
//!
//! # This returns data, never prose
//!
//! Rendering belongs to the CLI. Returning structured divergences is what lets
//! the human output and `--json` agree by construction rather than by two
//! formatters staying in step.

use std::collections::BTreeSet;

use crate::analyze::EventStatus;
use crate::cases::format::Expect;
use crate::cases::run::{CaseRun, StepOutcome};
use crate::replay::ctx_val_string;
use crate::step::{DeadlineOutcome, Outcome};

/// The comparison rule a field was checked under.
///
/// Carried in the divergence so a report can say *why* two lists that look the
/// same disagree — which is the question an author asks first when `effects`
/// fails and `configuration` did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Compared in order: the sequence is part of the claim.
    Ordered,
    /// Compared as a set: the order is an artefact.
    Set,
    /// Compared one key at a time.
    Keyed,
    /// A single value.
    Scalar,
    /// Not a comparison at all — the script could not run this step.
    Script,
}

/// One way a run differed from what its case expected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    /// The `expect` field, or `script` for a step that could not run.
    pub field: &'static str,
    /// The context key, when the field is compared key by key.
    pub key: Option<String>,
    pub expected: String,
    pub found: String,
    /// Where in the script this was observed. Final-state comparisons carry
    /// the last step's index, so a ten-step failure still says where.
    pub step: usize,
    pub rule: Rule,
}

impl Divergence {
    fn new(
        field: &'static str,
        rule: Rule,
        expected: impl Into<String>,
        found: impl Into<String>,
        step: usize,
    ) -> Self {
        Self {
            field,
            key: None,
            expected: expected.into(),
            found: found.into(),
            step,
            rule,
        }
    }
}

fn render(items: &[String]) -> String {
    items.join(", ")
}

/// A set-compared field, as a set.
///
/// Deduplicated as well as sorted, because "compares as a set" has to mean it
/// on **both** sides: a repeated leaf in an expectation is the same set as one
/// leaf, and reporting `[x, x]` against `[x]` as a difference is comparing a
/// multiset while claiming to compare a set.
fn as_set(items: &[String]) -> Vec<String> {
    let mut out: Vec<String> = items.to_vec();
    out.sort();
    out.dedup();
    out
}

/// Every way `run` diverged from `expect`, in field order.
///
/// The whole list, always: an author correcting one expectation wants to see
/// the other two in the same run, exactly as the runner runs the whole script.
pub fn diverge(expect: &Expect, run: &CaseRun) -> Vec<Divergence> {
    let mut out = Vec::new();
    let last = run.steps.len().saturating_sub(1);

    // A step that could not run is reported before any expectation. The case
    // failed for a reason that has nothing to do with what it expected, and
    // saying "configuration differs" first would send the author to the wrong
    // half of the file.
    for observation in &run.steps {
        // `expected` is always "the step runs" and `found` is always the
        // reason it did not. Two shapes here would make every consumer pick a
        // half, and the delta report picked the wrong one.
        let reason = match &observation.outcome {
            StepOutcome::Refused(refusal) => Some(if refusal.pending.is_empty() {
                format!("{}; nothing is pending", refusal.message)
            } else {
                format!("{}; pending: {}", refusal.message, render(&refusal.pending))
            }),
            StepOutcome::Sent(Outcome::Rejected(rejection)) => {
                Some(format!("{}: {}", rejection.code, rejection.message))
            }
            // A poll the engine rejected atomically is a step that did not
            // run, exactly as a rejected send is. Dropping it made the case
            // report `ok` for a script the machine refused — the "asserts
            // nothing and reports success" failure this format exists to
            // prevent, arrived at from the other direction.
            StepOutcome::Polled(DeadlineOutcome::Rejected(rejected)) => Some(format!(
                "{}: {}",
                rejected.rejection.code, rejected.rejection.message
            )),
            _ => None,
        };
        if let Some(found) = reason {
            out.push(Divergence {
                field: "script",
                key: None,
                expected: "the step runs".into(),
                found,
                step: observation.index,
                rule: Rule::Script,
            });
        }
    }

    if let Some(expected) = &expect.configuration {
        // A set: a configuration *is* a set, and parallel regions make any
        // written order an artefact.
        let found = configuration_leaves(run);
        if as_set(expected) != as_set(&found) {
            out.push(Divergence::new(
                "configuration",
                Rule::Set,
                render(&as_set(expected)),
                render(&as_set(&found)),
                last,
            ));
        }
    }

    if let Some(expected) = &expect.context {
        // Key by key: two whole maps side by side is a diff an author has to
        // read twice.
        for (key, want) in expected {
            let got = run.final_ctx.get(key).map(ctx_val_string);
            let got = got.unwrap_or_else(|| "(no such context slot)".into());
            if *want != got {
                out.push(Divergence {
                    field: "context",
                    key: Some(key.clone()),
                    expected: want.clone(),
                    found: got,
                    step: last,
                    rule: Rule::Keyed,
                });
            }
        }
    }

    if let Some(expected) = &expect.enabled {
        // A set: the scan order is an implementation detail the spec does not
        // fix, so pinning it would pin the implementation.
        let found: Vec<String> = run
            .final_enabled
            .iter()
            .filter(|report| report.status == EventStatus::Enabled)
            .map(|report| report.event.clone())
            .collect();
        if as_set(expected) != as_set(&found) {
            out.push(Divergence::new(
                "enabled",
                Rule::Set,
                render(&as_set(expected)),
                render(&as_set(&found)),
                last,
            ));
        }
    }

    if let Some(expected) = &expect.effects {
        // In order: emission order is deterministic and an executor runs them
        // in it, so a case that pins the order is pinning something real.
        if *expected != run.final_pending {
            out.push(Divergence::new(
                "effects",
                Rule::Ordered,
                render(expected),
                render(&run.final_pending),
                last,
            ));
        }
    }

    if let Some(expected) = &expect.terminal
        && *expected != run.terminal
    {
        out.push(Divergence::new(
            "terminal",
            Rule::Scalar,
            expected.to_string(),
            run.terminal.to_string(),
            last,
        ));
    }

    out
}

/// The active leaves of a run's final configuration.
fn configuration_leaves(run: &CaseRun) -> Vec<String> {
    match &run.final_configuration {
        crate::machine::ActiveConfiguration::Sequential { leaf } => vec![leaf.clone()],
        crate::machine::ActiveConfiguration::Parallel { leaves } => {
            leaves.values().cloned().collect()
        }
    }
}

/// Whether a run met every expectation its case named.
pub fn passes(expect: &Expect, run: &CaseRun) -> bool {
    diverge(expect, run).is_empty()
}

/// The fields a case asserted that a reader can be told were *checked*, as a
/// set, so a report can say what held as well as what did not.
pub fn checked(expect: &Expect) -> BTreeSet<&'static str> {
    expect.asserted().into_iter().collect()
}
