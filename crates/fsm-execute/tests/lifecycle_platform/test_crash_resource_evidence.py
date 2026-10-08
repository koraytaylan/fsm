"""Retained resource measurements must independently meet the declared bounds."""
import copy
import json
import unittest

from verify_crash_evidence import verify_resources


class ResourceEvidence(unittest.TestCase):
    def setUp(self):
        self.inventory = [(host, kind, 'repeated-noisy')
                          for host in ('standalone', 'embedded')
                          for kind in ('process', 'mcp')]
        self.rows = [dict(host=host, kind=kind, run=run,
                          descriptors=10, threads=4, rss_kib=4096)
                     for host, kind, _ in self.inventory for run in range(12)]

    def verify(self, rows):
        lines = [b'FSM_NATIVE_RESOURCE_OBSERVATION ' + json.dumps(row).encode()
                 for row in rows]
        return verify_resources(lines, self.inventory)

    def test_exact_inventory_and_exact_bounds_pass(self):
        for row in self.rows:
            if row['run'] > 1:
                row.update(descriptors=12, threads=6, rss_kib=20480)
        self.assertEqual(self.verify(self.rows), 48)

    def test_each_limit_plus_one_is_rejected(self):
        for name, value in [('descriptors', 13), ('threads', 7), ('rss_kib', 20481)]:
            with self.subTest(name=name):
                rows = copy.deepcopy(self.rows)
                rows[-1][name] = value
                with self.assertRaisesRegex(ValueError, 'resource bound'):
                    self.verify(rows)

    def test_missing_duplicate_reordered_and_wrong_axis_are_rejected(self):
        variants = [self.rows[:-1], self.rows + [self.rows[-1]],
                    list(reversed(self.rows)), copy.deepcopy(self.rows)]
        variants[-1][-1]['host'] = 'other'
        for rows in variants:
            with self.subTest(rows=rows[-1]):
                with self.assertRaisesRegex(ValueError, 'inventory'):
                    self.verify(rows)

    def test_boolean_zero_and_extra_measurements_are_rejected(self):
        for name, value in [('run', True), ('threads', True), ('rss_kib', 0),
                            ('extra', 1)]:
            with self.subTest(name=name):
                rows = copy.deepcopy(self.rows)
                rows[1][name] = value
                with self.assertRaises(ValueError):
                    self.verify(rows)

    def test_older_inventory_requires_no_resource_observations(self):
        self.assertEqual(verify_resources([], [('standalone', 'process', 'hold-result')]), 0)


if __name__ == '__main__':
    unittest.main()
