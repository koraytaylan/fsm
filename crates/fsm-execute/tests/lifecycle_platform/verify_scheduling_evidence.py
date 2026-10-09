"""Verify the independent private scheduling inventory, without completing 8903."""
from verify_completion_evidence import main, verify_owner

COORDINATOR = 'authority::allocator::native_tests::crash_matrix::provisioned_private_scheduling_owner_matrix'
PROFILE = dict(basename='scheduling', schema='fsm.native-scheduling-owner/1',
               scope='private-owner-scheduling', inventory='PRIVATE_SCHEDULING_CASES',
               artifacts=('HOST', 'FIXTURE', 'CLI'), coordinator=COORDINATOR)


def verify(repo, directory, commit, rustc):
    return verify_owner(repo, directory, commit, rustc, PROFILE)


if __name__ == '__main__':
    main(PROFILE)
