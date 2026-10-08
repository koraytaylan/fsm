"""Native logs retain exact case identity with portable artifact filenames."""
import unittest

import authority_probe as producer
import verify_native_evidence as verifier


class AuthorityArtifactNamesTests(unittest.TestCase):
    def test_nested_case_uses_portable_filename_and_preserves_segments(self):
        case = 'admission_cases::binding_identity_cases::binding_refuses_live_replacement_cgroup_identity'
        expected = 'authority-admission_cases--binding_identity_cases--binding_refuses_live_replacement_cgroup_identity.log'
        self.assertEqual(producer.authority_log_name(case), expected)
        self.assertEqual(verifier.authority_log_name(case, '--'), expected)
        self.assertEqual(verifier.authority_log_name(case, None), 'authority-' + case + '.log')

    def test_entire_inventory_has_unique_artifact_safe_names(self):
        names = [producer.authority_log_name(case) for case in producer.INVENTORY]
        self.assertEqual(len(names), len(set(names)))
        for name in names:
            self.assertFalse(any(character in name for character in '\\/:*?"<>|'))

    def test_malformed_names_cannot_alias_or_escape_evidence(self):
        for case in ('', '../case', '/case', 'case/name', 'case\\name',
                     'case:name', 'case:::name', 'case--name', 'case::', None):
            with self.subTest(case=case):
                with self.assertRaises(ValueError):
                    producer.authority_log_name(case)
                with self.assertRaises(ValueError):
                    verifier.authority_log_name(case, '--')

    def test_unknown_frozen_encoding_is_refused(self):
        for separator in ('/', '-', '::', '', False):
            with self.subTest(separator=separator), self.assertRaises(ValueError):
                verifier.authority_log_name('case', separator)
