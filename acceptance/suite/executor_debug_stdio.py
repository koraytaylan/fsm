"""Separate task-owned stdio pipes for an externally debugged installed host.

The protocol reader attaches to real stream endpoints; debugger diagnostics
stay in their own file and cannot be mistaken for MCP responses.
"""
import json
import os
from pathlib import Path
import stat
from types import SimpleNamespace

from .executor_scenarios import _wait_for_files
from .mcp import StdioClient, PROTOCOL_VERSION
from .native_fixture import privileged

NAMES = ('stdin', 'stdout', 'stderr')


class StdioPipes:
    def __init__(self, directory: Path):
        self.directory = directory
        self.guards = []
        self.streams = []

    def __enter__(self):
        try:
            for name in NAMES:
                path = self.directory / (name + '.fifo')
                os.mkfifo(path, 0o600)
                self.guards.append(os.open(path, os.O_RDWR | os.O_NONBLOCK | os.O_NOFOLLOW))
                flags = os.O_WRONLY if name == 'stdin' else os.O_RDONLY
                descriptor = os.open(path, flags | os.O_NONBLOCK | os.O_NOFOLLOW)
                os.set_blocking(descriptor, True)
                self.streams.append(os.fdopen(descriptor, 'w' if name == 'stdin' else 'r', encoding='utf-8'))
            return self
        except BaseException:
            self.__exit__()
            raise

    def release_guards(self):
        for descriptor in self.guards:
            os.close(descriptor)
        self.guards.clear()

    def __exit__(self, *_exception):
        self.release_guards()
        for stream in self.streams:
            if not stream.closed:
                stream.close()

    def attach(self, owner, unit):
        _wait_for_files(lambda: (self.directory / 'stdio-launch.json').exists(), owner, 10)
        process = SimpleNamespace(stdin=self.streams[0],stdout=self.streams[1],stderr=self.streams[2],
            poll=owner.poll,wait=owner.wait,kill=lambda: privileged('systemctl', 'stop', unit))
        client = StdioClient.from_process(process)
        client.cut_pipes = self
        return client


def trigger_stdio(report, client, instance: str, request_id: str) -> None:
    initialization = client.initialize()
    report.equal(initialization.get('protocolVersion'), PROTOCOL_VERSION,
        'the original debugged stdio host completes a genuine MCP initialization before triggering work')
    client.cut_pipes.release_guards()
    client._next_id += 1
    trigger = dict(jsonrpc='2.0',id=client._next_id,method='tools/call',params=dict(
        name='instance_send',arguments=dict(instance_id=instance,event={'name':'start'},request_id=request_id)))
    # The hardware stop can precede delivery of the initiating reply; sending
    # once and observing the original accepted journal avoids a hidden poll.
    client._send(trigger)
    client.cut_initialization = initialization
    client.cut_trigger = trigger


def validate_stdio(value: dict, original: dict, binary_hash: str) -> dict:
    if (value.get('original') != original or value.get('binary_sha256') != binary_hash
        or value.get('initialization', {}).get('protocolVersion') != PROTOCOL_VERSION
        or value.get('executable') != value.get('expected_executable')
        or not isinstance(value.get('executable'), dict)
        or any(type(value['executable'].get(key)) is not int for key in ('device','inode','uid','mode'))
        or value['executable']['inode'] <= 0 or value['executable']['device'] < 0
        or not stat.S_ISREG(value['executable']['mode'])
        or set(value.get('streams', {})) != set(NAMES)):
        raise ValueError('original installed stdio identity or initialization is unproven')
    identities = set()
    for number, name in enumerate(NAMES):
        row = value['streams'][name]; expected = row.get('expected', {})
        if (type(row.get('fd')) is not int or row['fd'] != number or row.get('actual') != expected
            or any(type(row.get('actual', {}).get(key)) is not int for key in ('device','inode','uid','mode'))
            or any(type(expected.get(key)) is not int for key in ('device','inode','uid','mode'))
            or expected['device'] < 0 or expected['inode'] <= 0 or expected['uid'] <= 0
            or not stat.S_ISFIFO(expected['mode']) or expected['mode'] & 0o077):
            raise ValueError('original stdin, stdout and stderr are not separate private owned FIFOs')
        identities.add((expected['device'], expected['inode']))
    if len(identities) != 3:
        raise ValueError('original stdio endpoints overlap')
    return value


def capture_stdio(client, ready: dict, binary: Path) -> dict | None:
    if client is None:
        return None
    original = ready['original']; process = Path('/proc', str(original['pid']))
    def metadata(path):
        value = path.stat()
        return dict(device=value.st_dev,inode=value.st_ino,uid=value.st_uid,mode=value.st_mode)
    value = dict(original=original,binary_sha256=ready['binary_sha256'],
        initialization=client.cut_initialization,trigger=client.cut_trigger,
        executable=metadata(process / 'exe'),expected_executable=metadata(binary),streams={})
    for number, name in enumerate(NAMES):
        value['streams'][name] = dict(fd=number,expected=metadata(client.cut_pipes.directory / (name + '.fifo')),
            actual=metadata(process / 'fd' / str(number)))
    validate_stdio(value, original, ready['binary_sha256'])
    (client.cut_pipes.directory / 'stdio.json').write_text(json.dumps(value, sort_keys=True), encoding='utf-8')
    return value
