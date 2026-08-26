//! Structural validation of a parsed `fsm.machine/1` definition.
//!
//! [`validate`] runs the structural rules SPEC.md's `def/*` table names and
//! returns every finding at once. Finding order is observable — the
//! `spec_validate`, `analyze_golden`, and naive-caller suites, and the
//! `machine_create` error payload, all depend on which finding comes first —
//! so [`validate_with_compatibility`] calls the phases in [`structure`],
//! [`blocks`], and [`reactive`] in one fixed sequence, and a phase never
//! reorders its own pushes.

use std::collections::BTreeSet;

use super::{Finding, MachineSpec, Topology};

mod blocks;
mod reactive;
mod structure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DefinitionCompatibility {
    Current,
    HistoricalPersistence,
}

impl DefinitionCompatibility {
    fn permits_legacy_history_shapes(self, spec: &MachineSpec) -> bool {
        self == Self::HistoricalPersistence
            && matches!(spec.topology, Topology::Sequential { .. })
            && spec.deadlines.is_empty()
    }
}

pub fn validate(spec: &MachineSpec) -> Result<(), Vec<Finding>> {
    validate_with_compatibility(spec, DefinitionCompatibility::Current)
}

pub(super) fn validate_with_compatibility(
    spec: &MachineSpec,
    compatibility: DefinitionCompatibility,
) -> Result<(), Vec<Finding>> {
    let mut errs = Vec::new();
    let permits_legacy_history_shapes = compatibility.permits_legacy_history_shapes(spec);
    structure::check_regions(spec, &mut errs);
    let tables = structure::collect_states(spec, &mut errs);
    structure::check_state_count(&tables, &mut errs);
    structure::check_nodes(&tables, permits_legacy_history_shapes, &mut errs);
    structure::check_initial_chains(spec, &tables, &mut errs);
    let event_names: BTreeSet<_> = spec.events.iter().map(|e| e.name.as_str()).collect();
    let effect_names: BTreeSet<_> = spec.effects.iter().map(|e| e.name.as_str()).collect();
    structure::check_declarations(spec, &mut errs);
    let cell = blocks::check_transitions(
        spec,
        &tables,
        &event_names,
        &effect_names,
        permits_legacy_history_shapes,
        &mut errs,
    );
    blocks::check_deadlines(spec, &tables, &effect_names, &mut errs);
    blocks::check_state_block_limits(spec, &effect_names, &mut errs);
    blocks::check_cell_limits(cell, &mut errs);
    structure::check_field_counts(spec, &mut errs);
    structure::check_enum_references(spec, &mut errs);
    reactive::validate_reactive(spec, &mut errs);
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}
