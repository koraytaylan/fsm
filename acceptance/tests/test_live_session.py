"""Synthetic session-budget controls, never live-model acceptance evidence."""
import unittest
import os
import subprocess
import sys
from pathlib import Path

from acceptance.suite.fsm import Scratch

from acceptance.manual.session import Budget, Session, publish


class SessionBudgetTests(unittest.TestCase):
    def test_executable_entrypoints_do_not_shadow_standard_library(self):
        repository = Path(__file__).resolve().parents[2]
        for name in ('client.py', 'provision.py'):
            with self.subTest(entrypoint=name):
                result = subprocess.run([sys.executable, '-B', str(repository / 'acceptance/manual' / name),
                    '--help'], cwd=repository, env=dict(os.environ, PYTHONDONTWRITEBYTECODE='1'),
                    text=True, capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('--candidate', result.stdout)

    def test_fortieth_call_and_exact_elapsed_boundary_pass(self):
        budget = Budget(100)
        for index in range(40):
            budget.observe({'method': 'tools/call'}, 100 + index)
        self.assertEqual(budget.calls, 40)
        self.assertEqual(budget.finish(1000), 900)
        with self.assertRaisesRegex(RuntimeError, 'call budget'):
            budget.observe({'method': 'tools/call'}, 1000)
        self.assertEqual(budget.calls, 41)

    def test_discovery_reads_failed_calls_and_retries_cannot_hide(self):
        budget = Budget(0)
        for method in ('tools/list', 'resources/list', 'resources/read', 'prompts/list',
                       'prompts/get', 'tools/call', 'tools/call'):
            budget.observe({'method': method}, 1)
        self.assertEqual(budget.calls, 7)
        budget.observe({'method': 'initialize'}, 1)
        budget.observe({'method': 'notifications/initialized'}, 1)
        self.assertEqual(budget.calls, 7)

    def test_elapsed_excess_or_reversed_clock_refuses(self):
        for observed in (99.999, 1000.001):
            with self.subTest(observed=observed), self.assertRaisesRegex(RuntimeError, 'elapsed budget'):
                Budget(100).observe({'method': 'tools/call'}, observed)

    def test_late_receipt_counts_post_delivery_discovery_but_excludes_earlier_setup(self):
        with Scratch('live-delivery-counter-control') as scratch:
            root = Path(scratch.path)
            session = Session(root, [], {})
            session.charge({'method': 'tools/list'}, 99)
            session.charge({'method': 'resources/list'}, 101)
            publish(root / 'brief-delivered.json', {'monotonic': 100})
            session.charge({'method': 'tools/call'}, 102)
            self.assertEqual(session.budget.calls, 2)
            self.assertEqual(session.budget.started, 100)
            session.begin({'monotonic': 102})
            self.assertEqual(session.budget.started, 100)

    def test_finite_initialization_cannot_hide_unbounded_requests_before_receipt(self):
        with Scratch('live-initialization-counter-control') as scratch:
            session = Session(Path(scratch.path), [], {})
            for _index in range(32):
                session.charge({'method': 'tools/list'}, 100)
            with self.assertRaisesRegex(RuntimeError, 'initialization exceeds'):
                session.charge({'method': 'tools/list'}, 100)


if __name__ == '__main__':
    unittest.main()
