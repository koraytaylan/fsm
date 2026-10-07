"""Authority fixture teardown refuses unresolved state; no native operations."""
import hashlib
import importlib.util
import json
from pathlib import Path
import stat
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

specification = importlib.util.spec_from_file_location(
    'authority_retirement_probe', Path(__file__).with_name('authority_probe.py'))
probe = importlib.util.module_from_spec(specification)
specification.loader.exec_module(probe)


class AuthorityRetirementTests(unittest.TestCase):
    def test_only_absent_or_empty_protected_authority_state_allows_retirement(self):
        cases = [('absent', True), ('empty', True), ('retained', False),
                 ('unreadable', False), ('symlink', False), ('foreign', False),
                 ('writable', False), ('regular', False)]
        for state, expected in cases:
            with self.subTest(state=state):
                directory = MagicMock()
                mode = stat.S_IFDIR | 0o755
                if state == 'symlink':
                    mode = stat.S_IFLNK | 0o777
                if state == 'regular':
                    mode = stat.S_IFREG | 0o644
                if state == 'writable':
                    mode |= 0o022
                directory.lstat.return_value = SimpleNamespace(
                    st_uid=1 if state == 'foreign' else 0, st_mode=mode)
                directory.iterdir.side_effect = lambda: iter(
                    ['original-domain'] if state == 'retained' else [])
                if state == 'absent':
                    directory.lstat.side_effect = FileNotFoundError()
                if state == 'unreadable':
                    directory.iterdir.side_effect = PermissionError()
                with patch.object(probe, 'Path', return_value=directory):
                    self.assertEqual(probe.authority_state_is_clear(), expected)

    def test_main_retains_exact_installed_authority_when_namespace_survives(self):
        self.check_main_teardown(retained=True)

    def test_main_removes_matched_authority_only_after_state_is_clear(self):
        self.check_main_teardown(retained=False)

    def test_main_refuses_preexisting_state_before_authority_installation(self):
        self.check_main_teardown(retained=False, initially_clear=False)

    def test_native_timeout_retains_exact_authority_for_unknown_state(self):
        self.check_main_teardown(retained=True, timeout=True)

    def test_native_timeout_records_failed_case_and_partial_output_after_clear_teardown(self):
        self.check_main_teardown(retained=False, timeout=True)

    def check_main_teardown(self, retained, initially_clear=True, timeout=False):
        report = MagicMock()
        artifact = MagicMock()
        artifact.read_bytes.return_value = b'fixture'
        installed = dict(device=17, inode=23,
                         sha256=hashlib.sha256(b'fixture').hexdigest())
        process = MagicMock()
        process.poll.return_value = None

        def output(command, **_options):
            if command[0] == 'git':
                return 'a' * 40 if 'rev-parse' in command else ''
            if command[0] == 'rustc':
                return 'rustc fixture'
            self.assertIn('install', command)
            return json.dumps(installed).encode()

        def run(command, **_options):
            if '--exact' in command:
                if timeout:
                    raise probe.subprocess.TimeoutExpired(command, 30, output=b'partial stdout',
                                                          stderr=b'partial stderr')
                name = command[command.index('--exact') + 1]
                return SimpleNamespace(returncode=0, stderr=b'',
                                       stdout=('test ' + name + ' ... ok').encode())
            self.assertIn('remove', command)
            return SimpleNamespace(returncode=0)

        with (
            patch.object(probe.argparse.ArgumentParser, 'parse_args', return_value=
                         SimpleNamespace(toolchain='stable', report=report)),
            patch.object(probe, 'build_authority', return_value=artifact),
            patch.object(probe, 'authority_state_is_clear',
                         side_effect=[initially_clear, not retained]),
            patch.object(probe.subprocess, 'check_output', side_effect=output) as outputs,
            patch.object(probe.subprocess, 'Popen', return_value=process),
            patch.object(probe.subprocess, 'run', side_effect=run) as calls,
        ):
            if not initially_clear:
                with self.assertRaisesRegex(AssertionError, 'requires clear authority state'):
                    probe.main()
                self.assertFalse(calls.called)
                self.assertFalse(any('install' in call.args[0] for call in outputs.call_args_list))
            elif retained:
                with self.assertRaisesRegex(RuntimeError, 'retained exact installed authority') as failure:
                    probe.main()
                if timeout:
                    self.assertIsInstance(failure.exception.__cause__, probe.subprocess.TimeoutExpired)
                self.assertFalse(any('remove' in call.args[0] for call in calls.call_args_list))
                record = json.loads(report.with_name.return_value.write_text.call_args.args[0])
                for name, value in installed.items():
                    self.assertEqual(record[name], value)
                self.assertEqual(record['source_commit'], 'a' * 40)
                self.assertFalse(record['passed'])
                if timeout:
                    self.assertTrue(record['cases'][0]['timed_out'])
                    self.assertIsNone(record['cases'][0]['exit_code'])
            else:
                self.assertEqual(probe.main(), 1 if timeout else 0)
                self.assertEqual(sum('remove' in call.args[0] for call in calls.call_args_list), 1)
                record = json.loads(report.write_text.call_args.args[0])
                if timeout:
                    self.assertFalse(record['passed'])
                    self.assertTrue(record['cases'][0]['timed_out'])
                    self.assertIsNone(record['cases'][0]['exit_code'])
                else:
                    broker = next(call for call in calls.call_args_list
                                  if any(str(value).endswith('::provisioned_broker_access')
                                         for value in call.args[0]))
                    self.assertEqual(broker.kwargs['timeout'], 90)
                    self.assertEqual(calls.call_args_list[0].kwargs['timeout'], 30)
            if timeout:
                report.with_name.return_value.write_bytes.assert_called_once_with(
                    b'partial stdoutpartial stderr')


if __name__ == '__main__':
    unittest.main()
