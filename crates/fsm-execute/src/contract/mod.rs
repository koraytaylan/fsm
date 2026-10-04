//! Pure machine/handler structural checks. This module never authorizes a spawn.

mod effects;
mod outcomes;
mod report;

pub use effects::analyze_effects;
pub use outcomes::analyze_contract;
pub use report::{CheckStatus, EffectSite, FORMAT, Finding, Limits, Report};
