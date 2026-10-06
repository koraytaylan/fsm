//! Named axes shared by the root and unprivileged handoff controls.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Handoff {
    Warm,
    Cold,
    Conflicting,
    Rejected,
}

impl Handoff {
    pub(super) fn cold(self) -> bool {
        self != Self::Warm
    }

    pub(super) fn retained(self) -> bool {
        matches!(self, Self::Conflicting | Self::Rejected)
    }
}
