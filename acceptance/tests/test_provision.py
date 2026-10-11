"""Focused secret-redaction and failure-cleanup controls for disposable runners."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import subprocess
import unittest
from unittest.mock import patch

from acceptance.suite.fsm import Scratch

SPEC = importlib.util.spec_from_file_location('podman_provision_controls',
    Path(__file__).resolve().parents[1] / 'provision-podman-runner.py')
PROVISION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROVISION)


class ProvisionTests(unittest.TestCase):
    def test_rootful_isolation_maps_only_the_callers_complete_subordinate_ranges(self):
        host = PROVISION.Host.__new__(PROVISION.Host)
        host.rootful = True
        name = PROVISION.pwd.getpwuid(os.getuid()).pw_name
        with patch.object(PROVISION.Path, 'read_text', return_value=
                          f'other-user:200000:65536\n{name}:100000:65536\n'):
            options = host.isolation()
        self.assertIn(f'--uidmap=0:{os.getuid()}:1', options)
        self.assertIn(f'--gidmap=0:{os.getgid()}:1', options)
        self.assertIn('--uidmap=1:100000:65535', options)
        self.assertIn('--gidmap=1:100000:65535', options)
        self.assertNotIn('--privileged', options)

    def test_rootful_isolation_refuses_host_root_and_incomplete_ranges(self):
        host = PROVISION.Host.__new__(PROVISION.Host)
        host.rootful = True
        name = PROVISION.pwd.getpwuid(os.getuid()).pw_name
        for row in (f'{name}:0:65536', f'{name}:100000:65535', 'other-user:100000:65536'):
            with self.subTest(row=row), patch.object(PROVISION.Path, 'read_text', return_value=row):
                with self.assertRaisesRegex(ValueError, 'complete assigned subordinate ID range'):
                    host.isolation()
        for uid, gid in ((0, os.getgid()), (os.getuid(), 0)):
            with self.subTest(uid=uid, gid=gid), \
                 patch.object(PROVISION.os, 'getuid', return_value=uid), \
                 patch.object(PROVISION.os, 'getgid', return_value=gid):
                with self.assertRaisesRegex(ValueError, 'non-root caller'):
                    host.isolation()

    def test_registration_failure_does_not_disclose_stdin_or_output(self):
        secret = 'synthetic-registration-secret'
        command = [sys.executable, '-B', '-c',
            'import sys; value=sys.stdin.read(); print(value); print(value,file=sys.stderr); sys.exit(101)']
        with Scratch('runner-secret-control') as scratch, \
             patch.dict(os.environ, TMPDIR=str(scratch.path)):
            with self.assertRaises(RuntimeError) as raised:
                PROVISION.run(command, input=secret)
            self.assertNotIn(secret, str(raised.exception))
            self.assertIn('exit 101', str(raised.exception))
            diagnostic = Path(scratch.path) / 'disposable-podman-host/registration-diagnostic.log'
            self.assertNotIn(secret, diagnostic.read_text())
            self.assertIn('<redacted>', diagnostic.read_text())
            self.assertEqual(diagnostic.stat().st_mode & 0o777, 0o600)

    def test_readiness_api_failure_still_removes_local_container(self):
        with Scratch('podman-cleanup-control') as scratch:
            host = PROVISION.Host.__new__(PROVISION.Host)
            host.cache = Path(scratch.path)
            host.state = host.cache / 'runner-state.json'
            host.podman = ['podman']
            state = dict(container_id='a' * 64, runner_name='owned-control')
            host.save(state)
            commands = []

            def execute(arguments, **_kwargs):
                commands.append(arguments)
                if arguments[0] == 'gh':
                    raise RuntimeError('synthetic remote outage')
                return '{}'

            with patch.object(PROVISION, 'run', side_effect=execute), \
                 patch.object(PROVISION.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)), \
                 patch.object(PROVISION, 'api', return_value={'runners': []}):
                with self.assertRaisesRegex(RuntimeError, 'local container removed'):
                    host.stop()
            self.assertIn(['podman', 'rm', '--force', '--ignore', state['container_id']], commands)
            self.assertEqual(json.loads(host.state.read_text()), state)
            self.assertTrue((host.cache / 'owned-control-inspect.json').exists())


if __name__ == '__main__':
    unittest.main()
