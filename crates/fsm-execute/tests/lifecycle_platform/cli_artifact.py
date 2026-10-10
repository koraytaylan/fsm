"""Build and select the exact CLI artifacts used by native acceptance."""
import json
import os
from pathlib import Path
import subprocess


def build_host_test(repo, toolchain):
    """Select the private host harness without adding a public test API."""
    return build_completion_test(repo, toolchain, 'fsm-cli', ['--lib'],
                                 'fsm_cli', ['lib'])


def build_owner_test(repo, toolchain):
    """Select the private lifecycle capacity observer without running it."""
    return build_completion_test(repo, toolchain, 'fsm-execute', ['--lib'],
                                 'fsm_execute', ['lib'])


def build_boundary_test(repo, toolchain):
    """Select the public async completion integration test, without running it."""
    return build_completion_test(repo, toolchain, 'fsm-execute',
                                 ['--test', 'async_completion'],
                                 'async_completion', ['test'])


def build_contract_mcp_test(repo, toolchain):
    """Select the exact real-client draft/repair observer without running it."""
    return build_completion_test(repo, toolchain, 'fsm-cli',
                                 ['--test', 'executor_contract_mcp'],
                                 'executor_contract_mcp', ['test'])


def build_workflow_handler(repo, toolchain):
    """Require the exact feature-gated protocol marker binary, without running it."""
    command = ['cargo', '+' + toolchain, 'build', '-p', 'fsm-cli', '--bin',
               'fsm-lifecycle-fixture', '--features', 'lifecycle-test-fixture',
               '--message-format=json']
    result = subprocess.run(command, cwd=repo, capture_output=True, timeout=180,
                            env=dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_STRIP='debuginfo'))
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    if result.returncode:
        rendered = [row['message'].get('rendered', '') for row in messages
                    if row.get('reason') == 'compiler-message']
        raise RuntimeError('Workflow marker build failed: ' + ''.join(rendered)
                           + result.stderr.decode(errors='replace'))
    matches = [row['executable'] for row in messages
               if row.get('reason') == 'compiler-artifact'
               and row['target']['name'] == 'fsm-lifecycle-fixture'
               and row['target']['kind'] == ['bin']
               and row['profile']['test'] is False and row.get('executable')]
    assert len(matches) == 1, ('fsm-lifecycle-fixture', matches)
    return Path(matches[0]).resolve()


def build_completion_test(repo, toolchain, package, selection, target, kind):
    """Require exactly one successful compiler-produced completion observer."""
    command = ['cargo', '+' + toolchain, 'test', '-p', package, *selection,
               '--features', 'lifecycle-test-fixture', '--no-run',
               '--message-format=json']
    result = subprocess.run(command, cwd=repo, capture_output=True, timeout=180,
                            env=dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_STRIP='debuginfo'))
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    if result.returncode:
        rendered = [row['message'].get('rendered', '') for row in messages
                    if row.get('reason') == 'compiler-message']
        raise RuntimeError('Completion observer build failed: ' + ''.join(rendered)
                           + result.stderr.decode(errors='replace'))
    matches = [row['executable'] for row in messages
               if row.get('reason') == 'compiler-artifact'
               and row['target']['name'] == target
               and row['target']['kind'] == kind
               and row['profile']['test'] is True and row.get('executable')]
    assert len(matches) == 1, (target, matches)
    return Path(matches[0]).resolve()


def build_crash_artifacts(repo, toolchain):
    """Select the feature-gated observer, fixture and production CLI together."""
    command = ['cargo', '+' + toolchain, 'test', '-p', 'fsm-cli', '--test',
               'executor_lifecycle_crash', '--features', 'lifecycle-test-fixture',
               '--no-run', '--message-format=json']
    result = subprocess.run(command, cwd=repo, capture_output=True, timeout=180,
                            env=dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_STRIP='debuginfo'))
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    if result.returncode:
        rendered = [row['message'].get('rendered', '') for row in messages
                    if row.get('reason') == 'compiler-message']
        raise RuntimeError('Crash fixture build failed: ' + ''.join(rendered)
                           + result.stderr.decode(errors='replace'))
    artifacts = {}
    for variable, target, test in [('TEST', 'executor_lifecycle_crash', True),
                                   ('FIXTURE', 'fsm-lifecycle-fixture', False),
                                   ('CLI', 'fsm', False)]:
        matches = [row['executable'] for row in messages
                   if row.get('reason') == 'compiler-artifact'
                   and row['target']['name'] == target
                   and row['profile']['test'] is test and row.get('executable')]
        assert len(matches) == 1, (target, matches)
        artifacts[variable] = Path(matches[0]).resolve()
    return artifacts


def build_cli(repo, toolchain, test):
    target = 'mcp_execute_workflow' if test else 'fsm'
    command = ['cargo', '+' + toolchain, 'test' if test else 'build',
               '-p', 'fsm-cli', '--test' if test else '--bin', target,
               '--message-format=json']
    if test:
        command.append('--no-run')
    result = subprocess.run(command, cwd=repo, capture_output=True, timeout=180,
                            env=dict(os.environ, CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_STRIP='debuginfo'))
    messages = [json.loads(line) for line in result.stdout.splitlines()]
    if result.returncode:
        rendered = [row['message'].get('rendered', '') for row in messages
                    if row.get('reason') == 'compiler-message']
        raise RuntimeError('CLI build failed: ' + ''.join(rendered)
                           + result.stderr.decode(errors='replace'))
    artifacts = [row['executable'] for row in messages
                 if row.get('reason') == 'compiler-artifact'
                 and row['target']['name'] == target
                 and row['profile']['test'] is test and row.get('executable')]
    assert len(artifacts) == 1
    return Path(artifacts[0]).resolve()
