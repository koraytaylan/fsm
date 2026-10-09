"""Verify genuine native contract refusal/repair evidence without completing 9103."""
from verify_completion_evidence import main, verify_owner

COORDINATOR = 'authority::allocator::native_tests::crash_matrix::provisioned_contract_admission_matrix'
PROFILE = dict(basename='contract-admission', schema='fsm.native-contract-admission/1',
               scope='native-contract-refusal-repair', inventory='CONTRACT_ADMISSION_CASES',
               artifacts=('CONTRACT', 'FIXTURE', 'CLI'), coordinator=COORDINATOR)


def verify(repo, directory, commit, rustc):
    return verify_owner(repo, directory, commit, rustc, PROFILE)


if __name__ == '__main__':
    main(PROFILE)
