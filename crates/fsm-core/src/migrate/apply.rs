//! Moving one instance onto a corrected definition: the pure function.
//!
//! Seven steps in a fixed order — gate, configuration, context, carry-over,
//! invariants, reaction, return — and every refusal is atomic: on any error
//! the caller's `InstanceState` is untouched and no partial state escapes.
//!
//! Plan 0011 task 5401.

// A `Rejection` carries the decision trace an operator needs to see, so it is
// large by design and returned by value everywhere in this engine. `step` sets
// the same allowance for the same reason.
#![allow(clippy::result_large_err)]

use std::collections::{BTreeMap, BTreeSet};

use crate::expr::eval::{Budget, Val, eval};
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::spec::MachineSpec;
use crate::step::{Applied, Rejection};
use crate::tree::Tree;

/// A migrated instance and what the migration did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migrated {
    /// The instance, after the mapping, the carry-over, and the reaction.
    pub state: InstanceState,
    /// What an operator needs to see before approving it.
    pub report: MigrationReport,
}

/// The account of one migration: every decision it made, named.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationReport {
    /// Old leaf to new leaf, one entry per active leaf, keyed by region name
    /// for a parallel instance and by the empty string for a sequential one.
    pub leaves: Vec<(String, String, String)>,
    /// Context variables the mapping computed, with their new values as
    /// canonical strings.
    pub projected: Vec<(String, String)>,
    /// New variables nobody mapped, which took their declared `init`.
    pub defaulted: Vec<String>,
    /// Old variables the new definition does not declare, which are dropped.
    pub dropped: Vec<String>,
    /// Monitor invariants that failed on the migrated state. They do not
    /// block; an operator sees them and decides.
    pub monitor_flags: Vec<String>,
    /// History bindings the mapping does not cover, `owner/child` each.
    pub dropped_history: Vec<String>,
    /// Every timer whose due time moved: name, the time it had, the time it
    /// has now. Migration restarts the clock, and this is where an operator
    /// sees it.
    pub rescheduled_deadlines: Vec<(String, Option<i64>, Option<i64>)>,
    /// Effect ids carried verbatim.
    pub retained_effects: Vec<String>,
    /// Signal ids carried verbatim.
    pub retained_signals: Vec<String>,
    /// Slots dropped because their result was already delivered.
    pub dropped_slots: Vec<String>,
    /// The reaction the migrated instance ran, if any.
    pub microsteps: Vec<crate::trace::MicrostepTrace>,
}

/// One migration attempt, carrying what it worked out before it finished.
///
/// A preview and an apply are the *same* attempt read two ways: a preview
/// wants the report even when the outcome is a refusal, and an apply wants
/// the state. Structuring it this way is what makes preview/apply agreement
/// a property of the code rather than of two implementations staying in
/// step.
pub(crate) struct Attempt {
    /// What the attempt determined, however far it got.
    pub(crate) report: MigrationReport,
    /// The configuration the mapping produced, before any reaction.
    pub(crate) mapped_configuration: Option<ActiveConfiguration>,
    /// The migrated instance, or the refusal that stopped it.
    pub(crate) outcome: Result<InstanceState, Rejection>,
}

/// Move `st` from `from` onto `to` under `to`'s own `supersedes` mapping.
pub fn migrate(
    from: &CompiledMachine,
    to: &CompiledMachine,
    tree_to: &Tree,
    st: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<Migrated, Rejection> {
    let attempt = attempt(from, to, tree_to, st, now_ms, budget);
    attempt.outcome.map(|state| Migrated {
        state,
        report: attempt.report,
    })
}

pub(crate) fn attempt(
    from: &CompiledMachine,
    to: &CompiledMachine,
    tree_to: &Tree,
    st: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> Attempt {
    let mut report = MigrationReport::default();
    let mut mapped_configuration = None;
    macro_rules! refuse {
        ($rejection:expr) => {
            return Attempt {
                report,
                mapped_configuration,
                outcome: Err($rejection),
            }
        };
    }
    let Some(supersedes) = &to.spec.supersedes else {
        refuse!(reject(
            "req/migrate_not_superseded",
            format!("{} supersedes nothing", to.spec.name),
            "migrate onto a definition whose supersedes block names the one this instance is on",
        ));
    };
    // And it must supersede *this* machine. A mapping written against some
    // other definition would map states that happen to share names and
    // project a context it was never checked against — the admission rules
    // ran against the machine the block names, not this one.
    if crate::hashes::digest_of(&from.machine_id) != Some(supersedes.machine.as_str()) {
        refuse!(reject(
            "req/migrate_not_superseded",
            format!(
                "{} supersedes {} and the instance is on {}",
                to.spec.name, supersedes.machine, from.machine_id
            ),
            "migrate onto the definition that supersedes the one this instance is on, or migrate \
             in hops if a chain of definitions stands between them",
        ));
    }

    // Step one — gate. There is nothing to save in a settled instance, and
    // migrating a finished workflow would change what it did.
    if st.status != Status::Running {
        refuse!(reject(
            "req/migrate_settled",
            format!("the instance is {}", st.status.as_str()),
            "a settled instance has nothing to migrate; read its history instead",
        ));
    }

    // Step two — map the configuration. A leaf with no entry refuses the
    // whole migration: partial migration is never performed and no leaf is
    // ever guessed.
    let mapping: BTreeMap<&str, &str> = supersedes
        .states
        .iter()
        .map(|(old, new)| (old.as_str(), new.as_str()))
        .collect();
    let configuration = match &st.configuration {
        ActiveConfiguration::Sequential { leaf } => {
            let mapped = match map_leaf(&mapping, leaf, None) {
                Ok(mapped) => mapped,
                Err(rejection) => refuse!(rejection),
            };
            report
                .leaves
                .push((String::new(), leaf.clone(), mapped.clone()));
            ActiveConfiguration::Sequential { leaf: mapped }
        }
        ActiveConfiguration::Parallel { leaves } => {
            let mut mapped_leaves = BTreeMap::new();
            for (region, leaf) in leaves {
                let mapped = match map_leaf(&mapping, leaf, Some(region)) {
                    Ok(mapped) => mapped,
                    Err(rejection) => refuse!(rejection),
                };
                report
                    .leaves
                    .push((region.clone(), leaf.clone(), mapped.clone()));
                mapped_leaves.insert(region.clone(), mapped);
            }
            ActiveConfiguration::Parallel {
                leaves: mapped_leaves,
            }
        }
    };

    // Step three — project the context, against the *old* context: a
    // migration expression reads what the instance holds today.
    let projections: BTreeMap<&str, &str> = supersedes
        .context
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();
    let mut ctx = BTreeMap::new();
    for declared in &to.spec.context {
        match projections.get(declared.name.as_str()) {
            Some(source) => {
                let value = match project(source, &st.ctx, budget) {
                    Ok(value) => value,
                    Err(rejection) => refuse!(rejection),
                };
                report
                    .projected
                    .push((declared.name.clone(), value.canonical_string()));
                ctx.insert(declared.name.clone(), value);
            }
            None => {
                let value = match crate::step::parse_init_for(&declared.init, &declared.ty) {
                    Ok(value) => value,
                    Err(code) => refuse!(reject(
                        code,
                        format!("{} has no init this migration can use", declared.name),
                        "give the variable a valid init, or map it in the supersedes block",
                    )),
                };
                report.defaulted.push(declared.name.clone());
                ctx.insert(declared.name.clone(), value);
            }
        }
    }
    let declared: BTreeSet<&str> = to
        .spec
        .context
        .iter()
        .map(|var| var.name.as_str())
        .collect();
    report.dropped = st
        .ctx
        .keys()
        .filter(|name| !declared.contains(name.as_str()))
        .cloned()
        .collect();

    // Step four — carry over everything an instance holds besides its state.
    // A seam at this stage: task 5402 fills it, and the step order does not
    // move to accommodate what it will do.
    mapped_configuration = Some(configuration.clone());
    let carried = match super::carryover::carry_over(
        to,
        tree_to,
        st,
        &mapping,
        &configuration,
        &ctx,
        now_ms,
        budget,
    ) {
        Ok(carried) => carried,
        Err(rejection) => refuse!(rejection),
    };
    report.dropped_history = carried.dropped_history.clone();
    report.rescheduled_deadlines = carried.rescheduled_deadlines.clone();
    report.retained_effects = carried.retained_effects.clone();
    report.retained_signals = carried.retained_signals.clone();
    report.dropped_slots = carried.dropped_slots.clone();

    // Step five — the new definition's invariants, on the migrated state.
    // Migrating an instance into a state its own definition calls invalid is
    // precisely what this prevents, and it runs *before* the reaction so a
    // reaction cannot paper over it.
    let active = tree_to.active_state_names(&configuration);
    let (holds, monitor_flags, invariants) =
        crate::step::eval_invariants_for(&to.spec, &to.compiled_exprs, &ctx, &active, budget);
    if !holds {
        let mut rejection = reject(
            "run/invariant",
            failing(&to.spec, &invariants),
            "the migrated state breaks an enforce invariant; correct the mapping or the definition",
        );
        rejection.trace.invariants = invariants.clone();
        refuse!(rejection);
    }
    report.monitor_flags = monitor_flags;

    // Step six — the reaction phase. A migrated instance parked on a leaf
    // whose new definition has an eventless exit would be sitting in a state
    // its own machine says it should have left; `create` and a deadline poll
    // already run macrosteps for that reason, and this is the third case.
    let migrated = InstanceState {
        status: Status::Running,
        configuration,
        ctx,
        history: carried.history,
        deadlines: carried.deadlines,
        pending: carried.pending,
        invocations: carried.invocations,
        signals: carried.signals,
    };
    let applied: Applied = match crate::step::react_from(to, tree_to, &migrated, now_ms, budget) {
        Ok(applied) => applied,
        Err(rejection) => refuse!(rejection),
    };
    report.microsteps = applied.trace.microsteps.clone();

    // Step seven — return. `seq` is the store's business, and a pure function
    // that invented one would be lying about ordering.
    Attempt {
        report,
        mapped_configuration,
        outcome: Ok(InstanceState {
            status: applied.status_after,
            configuration: applied.configuration_after,
            ctx: applied.ctx_after,
            history: applied.history_after,
            deadlines: applied.deadlines_after,
            pending: migrated.pending,
            invocations: applied.invocations_after,
            signals: migrated.signals,
        }),
    }
}

/// One leaf's mapping, or the refusal that stops the whole migration.
fn map_leaf(
    mapping: &BTreeMap<&str, &str>,
    leaf: &str,
    region: Option<&str>,
) -> Result<String, Rejection> {
    mapping.get(leaf).map(|to| (*to).to_string()).ok_or_else(|| {
        let where_ = match region {
            Some(region) => format!("{leaf} in region {region}"),
            None => leaf.to_string(),
        };
        reject(
            "req/migrate_unmapped",
            format!("the mapping has no entry for {where_}"),
            "map every leaf an instance can be in, or migrate instances that are not in this one",
        )
    })
}

/// Evaluate one projection against the old context.
fn project(
    source: &str,
    old: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<Val, Rejection> {
    let expression = crate::expr::parser::parse(source)
        .map_err(|error| reject("run/action_error", error.message, error.hint))?;
    let bindings = crate::expr::eval::Bindings {
        ctx: old,
        evt: None,
        active: None,
    };
    match eval(&expression, &bindings, budget, false) {
        (Ok(value), _) => Ok(value),
        (Err(error), _) => {
            let mut rejection = reject("run/action_error", error.message, error.hint);
            // The block name is `migration`, reusing the vocabulary a reader
            // already knows rather than inventing one for this case.
            rejection.block = Some("migration".into());
            rejection.cause = Some(error.code);
            rejection.span = Some((error.span.start, error.span.end));
            Err(rejection)
        }
    }
}

/// The failing enforce invariants, named.
fn failing(spec: &MachineSpec, invariants: &[crate::trace::InvariantTrace]) -> String {
    let names: Vec<&str> = invariants
        .iter()
        .filter(|trace| !trace.passed)
        .map(|trace| trace.name.as_str())
        .collect();
    let _ = spec;
    format!("invariants failed after migration: {}", names.join(", "))
}

fn reject(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Rejection {
    Rejection {
        code,
        message: message.into(),
        hint: hint.into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: Default::default(),
        cause: None,
    }
}
