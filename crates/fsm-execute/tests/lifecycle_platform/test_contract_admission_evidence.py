"""Contract admission evidence faults are independent of native execution."""
import unittest

from test_completion_evidence import exercise_owner
import verify_contract_admission_evidence as verifier

INVENTORY = (('contract', 'process', 'contract-standalone'),
             ('contract', 'mcp', 'contract-standalone'),
             ('contract', 'process', 'contract-borrowed'),
             ('contract', 'mcp', 'contract-borrowed'),
             ('contract', 'process', 'contract-fair-standalone'),
             ('contract', 'mcp', 'contract-fair-standalone'),
             ('contract', 'process', 'contract-fair-borrowed'),
             ('contract', 'mcp', 'contract-fair-borrowed'),
             ('contract', 'process', 'contract-unknown-standalone'),
             ('contract', 'mcp', 'contract-unknown-standalone'),
             ('contract', 'process', 'contract-unknown-borrowed'),
             ('contract', 'mcp', 'contract-unknown-borrowed'),
             ('contract', 'process', 'contract-argument-standalone'),
             ('contract', 'mcp', 'contract-argument-standalone'),
             ('contract', 'process', 'contract-argument-borrowed'),
             ('contract', 'mcp', 'contract-argument-borrowed'),
             ('contract', 'process', 'contract-contention-standalone'),
             ('contract', 'mcp', 'contract-contention-standalone'),
             ('contract', 'process', 'contract-contention-borrowed'),
             ('contract', 'mcp', 'contract-contention-borrowed'),
             ('contract', 'process', 'contract-manual-standalone'),
             ('contract', 'mcp', 'contract-manual-standalone'),
             ('contract', 'process', 'contract-manual-borrowed'),
             ('contract', 'mcp', 'contract-manual-borrowed'),
             ('contract', 'process', 'contract-ack-only-standalone'),
             ('contract', 'mcp', 'contract-ack-only-standalone'),
             ('contract', 'process', 'contract-ack-only-borrowed'),
             ('contract', 'mcp', 'contract-ack-only-borrowed'),
             ('contract', 'process', 'contract-recovery-standalone'),
             ('contract', 'mcp', 'contract-recovery-standalone'),
             ('contract', 'process', 'contract-recovery-borrowed'),
             ('contract', 'mcp', 'contract-recovery-borrowed'),
             ('contract', 'process', 'contract-bound-standalone'),
             ('contract', 'mcp', 'contract-bound-standalone'),
             ('contract', 'process', 'contract-bound-borrowed'),
             ('contract', 'mcp', 'contract-bound-borrowed'),
             ('contract', 'process', 'contract-cancel-standalone'),
             ('contract', 'mcp', 'contract-cancel-standalone'),
             ('contract', 'process', 'contract-cancel-borrowed'),
             ('contract', 'mcp', 'contract-cancel-borrowed'))


class ContractAdmissionEvidence(unittest.TestCase):
    def exercise(self, alter_report=None, alter_log=None):
        return exercise_owner(alter_report, alter_log, profile=verifier.PROFILE,
                              inventory=INVENTORY, call_verify=verifier.verify)

    def test_exact_slice_does_not_complete_task_or_verify_executable_bytes(self):
        verdict = self.exercise()
        self.assertEqual(verdict['cases'], 40)
        self.assertTrue(verdict['verified'])
        self.assertFalse(verdict['task_complete'])
        self.assertFalse(verdict['gate_released'])
        self.assertFalse(verdict['executable_bytes_verified'])

    def test_each_handler_kind_and_unique_runtime_marker_is_required(self):
        for index in range(len(INVENTORY)):
            with self.subTest(index=index), self.assertRaises(ValueError):
                self.exercise(alter_log=lambda lines: lines.pop(index))
        for alter in (lambda lines: lines.insert(0, lines[0]),
                      lambda lines: lines.__setitem__(-1, b'0 passed; 0 failed; 1 ignored;')):
            with self.subTest(alter=alter), self.assertRaises(ValueError):
                self.exercise(alter_log=alter)

    def test_rejects_completion_inventory_scope_and_false_retirement(self):
        for key, value in [('scope', 'public-and-private-held-handlers'),
                           ('schema', 'fsm.native-completion-owner/1'),
                           ('source_commit', 'e' * 40), ('source_dirty', True),
                           ('passed', False), ('task_complete', True),
                           ('gate_released', True), ('timed_out', True),
                           ('exit_code', False), ('retained_authority', {}),
                           ('retained_stages', []), ('rustc', 'other compiler')]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.exercise(lambda report: report.update({key: value}))

    def test_exact_observer_artifact_bindings_cannot_be_omitted_or_rebound(self):
        for name in ('CONTRACT', 'FIXTURE', 'CLI'):
            for alter in (lambda report: report['artifacts'].pop(name),
                          lambda report: report['artifacts'][name].update(path='/other/test'),
                          lambda report: report['artifacts'][name].update(sha256='e' * 64)):
                with self.subTest(name=name, alter=alter), self.assertRaises(ValueError):
                    self.exercise(alter_report=alter)

    def test_original_completion_coordinator_cannot_establish_contract_admission(self):
        with self.assertRaises(ValueError):
            self.exercise(lambda report: report['command'].__setitem__(
                -5, 'authority::allocator::native_tests::crash_matrix::provisioned_private_completion_owner_matrix'))


if __name__ == '__main__':
    unittest.main()
