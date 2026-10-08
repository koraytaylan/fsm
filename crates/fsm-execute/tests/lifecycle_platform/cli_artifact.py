"""Build and select the exact CLI artifacts used by native acceptance."""
import json
import os
from pathlib import Path
import subprocess


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
