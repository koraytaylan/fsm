"""Drive fresh genuine CLI clients against the disposable job's live sessions.

Authentication remains with the installed host client; only published briefs,
documentation and the synthetic MCP service enter the model conversation.
Every delivered attempt is retained, including interrupted or failed attempts.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time


def briefs(path):
    result, name = {}, None
    for line in path.read_text().splitlines():
        if line.startswith('## Brief: '):
            name = line.removeprefix('## Brief: ')
            result[name] = []
        elif line.startswith('>') and name is not None:
            result[name].append(line.removeprefix('>').lstrip())
    return {name: '\n'.join(lines) for name, lines in result.items()}


def remote_receipt(host, container, directory, name, value):
    program = "from pathlib import Path;import json,os,sys; p=Path(sys.argv[1])/sys.argv[2]; " \
              "value=json.loads(sys.stdin.read());temporary=p.with_suffix('.pending'); " \
              "temporary.write_text(json.dumps(value)+'\\n');temporary.replace(p)"
    host.exec(container, ['python3', '-B', '-c', program, directory, name],
              user='fsm', input=json.dumps(value))


def run_client(host, container, current, destination, prompt):
    destination.mkdir(mode=0o700)
    arguments = host.podman[1:] + ['exec', '--interactive', '--user', 'fsm',
        '--env', 'PYTHONDONTWRITEBYTECODE=1', container, 'python3', '-B',
        current['source'] + '/acceptance/manual/bridge.py', current['socket']]
    configuration = {'mcpServers': {'fsm': {'command': host.podman[0], 'args': arguments}}}
    (destination / 'mcp.json').write_text(json.dumps(configuration, indent=2) + '\n')
    (destination / 'prompt.txt').write_text(prompt + '\n')
    cli = ['claude', '--restricted', '--strict-mcp-config', '--mcp-config',
        str(destination / 'mcp.json'), '--tools', 'ListMcpResourcesTool,ReadMcpResourceTool',
        '--allowedTools', 'ListMcpResourcesTool,ReadMcpResourceTool,mcp__fsm__*',
        '--setting-sources', '', '--disable-slash-commands', '--no-session-persistence',
        '--print', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose']
    (destination / 'invocation.json').write_text(json.dumps(cli, indent=2) + '\n')
    model, version, final, process = None, None, None, None
    started = time.monotonic()
    receipt = dict(monotonic=started, prompt_sha256=hashlib.sha256(prompt.encode()).hexdigest(),
                   session_id=current['session_id'], candidate=current['candidate'])
    try:
        with (destination / 'client-output.jsonl').open('wb') as output, \
             (destination / 'client-stderr.log').open('wb') as error:
            process = subprocess.Popen(cli, cwd=destination, env=dict(os.environ),
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error, start_new_session=True)
            started = time.monotonic()
            process.stdin.write((json.dumps(dict(type='user',
                message=dict(role='user', content=prompt))) + '\n').encode())
            process.stdin.flush()
            receipt['monotonic'] = started
            (destination / 'brief-delivered.json').write_text(json.dumps(receipt) + '\n')
            remote_receipt(host, container, current['directory'], 'brief-delivered.json', receipt)
            selector = selectors.DefaultSelector()
            selector.register(process.stdout, selectors.EVENT_READ)
            pending = bytearray()
            while time.monotonic() - started <= 900:
                if not selector.select(0.25):
                    if process.poll() is not None:
                        break
                    continue
                encoded = os.read(process.stdout.fileno(), 65_536)
                if not encoded:
                    break
                if output.tell() + len(encoded) > 64 * 1_048_576:
                    raise RuntimeError('original client transcript exceeds its bound')
                output.write(encoded)
                output.flush()
                pending.extend(encoded)
                while b'\n' in pending:
                    line, _, remainder = pending.partition(b'\n')
                    pending = bytearray(remainder)
                    if len(line) > 1_048_576:
                        raise RuntimeError('original client frame exceeds its bound')
                    event = json.loads(line)
                    if event.get('subtype') == 'init':
                        model, version = event.get('model'), event.get('claude_code_version')
                    if event.get('type') == 'assistant':
                        model = event.get('message', {}).get('model', model)
                    if event.get('type') == 'result':
                        final = dict(monotonic=time.monotonic(), has_final_answer=not event.get('is_error', False),
                            model=model, client_version=version, result=event.get('result'),
                            elapsed_seconds=time.monotonic() - started)
                if len(pending) > 1_048_576:
                    raise RuntimeError('original incomplete client frame exceeds its bound')
                if final is not None:
                    break
            selector.close()
    finally:
        if final is None:
            final = dict(monotonic=time.monotonic(), has_final_answer=False, model=model,
                         client_version=version, elapsed_seconds=time.monotonic() - started)
        (destination / 'client-result.json').write_text(json.dumps(final, indent=2) + '\n')
        try:
            remote_receipt(host, container, current['directory'], 'client-result.json', final)
            for name in ('client-output.jsonl', 'client-stderr.log', 'prompt.txt', 'mcp.json',
                         'invocation.json', 'brief-delivered.json'):
                host_path = destination / name
                if host_path.exists():
                    host.exec(container, ['mkdir', '-p', current['directory'] + '/client-originals'], user='fsm')
                    subprocess.run(host.podman + ['cp', str(host_path),
                        container + ':' + current['directory'] + '/client-originals/' + name],
                        check=True, capture_output=True, timeout=30)
            host.exec(container, ['chown', '-R', 'fsm:fsm', current['directory'] + '/client-originals'])
            remote_receipt(host, container, current['directory'], 'client-artifacts-ready.json',
                           dict(complete=True, monotonic=time.monotonic()))
        finally:
            if process is not None and process.poll() is None:
                if process.stdin is not None and not process.stdin.closed:
                    process.stdin.close()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGTERM)
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait(timeout=5)
    return final


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', required=True, type=int)
    parser.add_argument('--candidate', required=True)
    args = parser.parse_args()
    os.umask(0o077)
    repository = Path(__file__).resolve().parents[2]
    spec = importlib.util.spec_from_file_location('podman_live_host', repository / 'acceptance/provision-podman-runner.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    host = module.Host(rootful=True)
    state = json.loads(host.state.read_text())
    container = state['container_id']
    root = host.cache / ('live-clients-' + str(args.run_id))
    root.mkdir(mode=0o700)
    assigned = briefs(repository / 'acceptance/manual/briefs.md')
    current_path = f'/home/fsm/.cache/fsm-live-model-{args.run_id}-1/live-model/current.json'
    deadline = time.monotonic() + 10_800
    completed = set()
    while len(completed) < 9 and time.monotonic() < deadline:
        program = "from pathlib import Path;import sys;p=Path(sys.argv[1]);print(p.read_text() if p.exists() else 'null')"
        current = json.loads(host.exec(container, ['python3', '-B', '-c', program, current_path], user='fsm'))
        if current is None or current['session_id'] in completed:
            run = json.loads(module.run(['gh', 'run', 'view', str(args.run_id), '--json', 'status,conclusion']))
            if run['status'] == 'completed':
                raise RuntimeError('original live-model job finished before all nine sessions completed')
            time.sleep(1)
            continue
        if current['candidate'] != args.candidate:
            raise RuntimeError('live-session candidate differs from the requested frozen campaign')
        identifier = current['session_id']
        prompt = assigned[identifier.rsplit('-', 1)[0]]
        result = run_client(host, container, current, root / identifier, prompt)
        completed.add(identifier)
        print(json.dumps(dict(session_id=identifier, has_final_answer=result['has_final_answer'],
                              elapsed_seconds=result['elapsed_seconds'], model=result['model'])), flush=True)
        if not result['has_final_answer']:
            return 1
    if len(completed) != 9:
        raise RuntimeError('campaign did not provide all nine frozen session slots')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
