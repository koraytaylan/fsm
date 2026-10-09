//! Pure machine/handler structural checks. This module never authorizes a spawn.

mod admission;
mod effects;
mod outcomes;
mod report;

#[cfg(target_os = "linux")]
pub(crate) use admission::check_claimed;
pub(crate) use admission::check_outcome;
pub use admission::check_pending;
pub use effects::analyze_effects;
pub use outcomes::analyze_contract;
pub use report::{CheckStatus, EffectSite, FORMAT, Finding, Limits, Report};

/// Index verified identities once; caller-owned catalogue keys remain separate.
fn definition_index<'a>(
    machines: impl IntoIterator<Item = &'a fsm_core::machine::CompiledMachine>,
) -> std::collections::BTreeMap<&'a str, &'a fsm_core::machine::CompiledMachine> {
    let mut index = std::collections::BTreeMap::new();
    for machine in machines {
        index.insert(machine.machine_id.as_str(), machine);
        if let Some(digest) = fsm_core::hashes::digest_of(&machine.machine_id) {
            index.insert(digest, machine);
        }
    }
    index
}
