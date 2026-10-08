//! Workflow scenario categories and independent transition fixture material.
use super::{Value, object, string};
pub(super) fn interrupted_scenario(failures: &str) -> bool {
    !failures.starts_with("active-stop-complete")
        && ["crash-", "full-disk", "failed-stop", "active-stop"]
            .iter()
            .any(|prefix| failures.starts_with(prefix))
}
pub(super) fn transition(from: &str, event: &str, to: &str) -> Value {
    object([
        ("from", string(from)),
        ("on", string(event)),
        ("to", string(to)),
    ])
}
