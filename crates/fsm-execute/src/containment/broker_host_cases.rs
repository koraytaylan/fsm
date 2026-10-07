//! Native broker host axes and their complete sequential inventory.

use super::run_case;

pub(in super::super) fn public_service_loop() {
    run_case(false, Host::Admission);
    run_case(false, Host::AdmissionMcp);
}

pub(super) enum Host {
    Primitive,
    BoundClosure,
    BoundClosureMcp,
    BoundInterruption,
    BoundInterruptionMcp,
    BoundOwnedDriver,
    BoundOwnedDriverMcp,
    BoundPairedDriver,
    BoundPairedDriverMcp,
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
        Host::BoundPairedDriver | Host::BoundPairedDriverMcp => super::claimed_closure_cases::run(
            fixture,
            binding,
            effect,
            successor,
            super::claimed_closure_cases::Outcome::PairedInterrupted,
        ),
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

pub(in super::super) fn run() {
    for timeout in [false, true] {
        run_case(timeout, Host::Primitive);
    }
    run_case(false, Host::Shared);
    run_case(false, Host::BoundClosure);
    run_case(false, Host::BoundClosureMcp);
    run_case(false, Host::BoundInterruption);
    run_case(false, Host::BoundInterruptionMcp);
    run_case(false, Host::BoundOwnedDriver);
    run_case(false, Host::BoundOwnedDriverMcp);
    run_case(false, Host::BoundPairedDriver);
    run_case(false, Host::BoundPairedDriverMcp);
    run_case(false, Host::Fresh);
    run_case(false, Host::FreshMcp);
    run_case(false, Host::Cold);
    run_case(false, Host::ColdMcp);
    run_case(false, Host::ColdConflict);
    run_case(false, Host::ColdConflictMcp);
    run_case(false, Host::ColdRejected);
    run_case(false, Host::ColdRejectedMcp);
    run_case(false, Host::Admission);
    run_case(false, Host::AdmissionMcp);
    run_case(false, Host::AdmissionCancellation);
    run_case(false, Host::AdmissionMcpCancellation);
    run_case(false, Host::AdmissionCompetition);
    run_case(false, Host::AdmissionMcpCompetition);
}
