//! Pure machine/handler structural checks. This module never authorizes a spawn.

mod admission;
mod effects;
mod outcomes;
mod report;

pub use admission::check_pending;
pub use effects::analyze_effects;
pub use outcomes::analyze_contract;
pub use report::{CheckStatus, EffectSite, FORMAT, Finding, Limits, Report};
