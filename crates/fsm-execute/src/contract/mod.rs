//! Pure machine/handler structural checks. This module never authorizes a spawn.

mod effects;
mod report;

pub use effects::analyze_effects;
pub use report::{CheckStatus, EffectSite, FORMAT, Finding, Limits, Report};
