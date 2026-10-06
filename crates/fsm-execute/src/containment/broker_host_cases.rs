pub(super) enum Host {
    Primitive,
    BoundClosure,
    BoundClosureMcp,
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
        Host::BoundClosure | Host::BoundClosureMcp => {
            super::claimed_closure_cases::run(fixture, binding, effect, successor)
        }
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
