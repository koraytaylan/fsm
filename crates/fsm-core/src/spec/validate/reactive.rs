//! Rules for the reactive definition shapes plan 0009 introduces.
//!
//! This file is a destination, not dead code: workstream 0043 fills it with
//! the eventless-transition rules (`def/eventless_*`), 0044 with the
//! `raise` and internal-event rules, and 0045 with the `final` state rules
//! (`def/final_*`). [`validate_reactive`] runs last in
//! [`super::validate_with_compatibility`], so every finding it adds lands
//! after the structural findings and existing golden order is untouched.

use super::super::{Finding, MachineSpec};

pub(super) fn validate_reactive(_spec: &MachineSpec, _errs: &mut Vec<Finding>) {}
