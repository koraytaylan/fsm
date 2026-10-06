pub(super) enum Host {
    Primitive,
    BoundClosure,
    BoundClosureMcp,
    BoundInterruption,
    BoundInterruptionMcp,
    BoundOwnedDriver,
    BoundOwnedDriverMcp,
    Shared,
    Fresh,
    FreshMcp,
    Cold,
    ColdMcp,
    ColdConflict,
    ColdConflictMcp,
    ColdRejected,
    ColdRejectedMcp,
    Admission,
    AdmissionMcp,
    AdmissionCancellation,
    AdmissionMcpCancellation,
    AdmissionCompetition,
    AdmissionMcpCompetition,
}

pub(super) fn dispatch_fresh(
    fixture: &mut super::Fixture,
    binding: &super::Value,
    effect: &str,
    successor: &super::NativeDomain,
    host: &Host,
) -> bool {
    match host {
        Host::BoundOwnedDriver | Host::BoundOwnedDriverMcp => super::claimed_closure_cases::run(
            fixture,
            binding,
            effect,
            successor,
            super::claimed_closure_cases::Outcome::OwnedInterrupted,
        ),
        Host::BoundInterruption | Host::BoundInterruptionMcp => super::claimed_closure_cases::run(
            fixture,
            binding,
            effect,
            successor,
            super::claimed_closure_cases::Outcome::Interrupted,
        ),
        Host::BoundClosure | Host::BoundClosureMcp => super::claimed_closure_cases::run(
            fixture,
            binding,
            effect,
            successor,
            super::claimed_closure_cases::Outcome::Retained,
        ),
        Host::ColdRejected | Host::ColdRejectedMcp => {
            super::fresh_cases::rejected(fixture, binding, effect, successor)
        }
        Host::ColdConflict | Host::ColdConflictMcp => {
            super::fresh_cases::conflict(fixture, binding, effect, successor)
        }
        Host::Cold | Host::ColdMcp => super::fresh_cases::cold(fixture, binding, effect, successor),
        Host::Fresh | Host::FreshMcp => {
            super::fresh_cases::run(fixture, binding, effect, successor)
        }
        _ => return false,
    }
    true
}
