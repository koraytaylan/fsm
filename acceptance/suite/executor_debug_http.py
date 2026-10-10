"""One real HTTP trigger and original listening-socket identity at a hardware cut.

The initiating reply may be stopped with the host; sending once and retaining
its original socket endpoints does not require a progress request or a retry.
"""
import json
from pathlib import Path
import re
import socket
import stat

from .executor_scenarios import _wait_for_files
from .mcp import HttpClient, PROTOCOL_VERSION


def attach_http(directory: Path, owner, port: int):
    def listening():
        try:
            with socket.create_connection(('127.0.0.1', port), timeout=0.2):
                return True
        except OSError:
            return False
    _wait_for_files(listening, owner, 10)
    client = HttpClient('127.0.0.1', port)
    client.cut_directory = directory
    client.cut_connection = None
    return client


def trigger_http(report, client, instance: str, request_id: str) -> None:
    initialization = client.initialize()
    report.equal(initialization.get('protocolVersion'), PROTOCOL_VERSION,
        'the original debugged HTTP host completes genuine MCP initialization before one trigger')
    if not isinstance(client.session, str) or not 1 <= len(client.session) <= 256:
        raise ValueError('the initialized original HTTP host did not issue a bounded session identity')
    client._next_id += 1
    trigger = dict(jsonrpc='2.0', id=client._next_id, method='tools/call', params=dict(
        name='instance_send', arguments=dict(instance_id=instance, event={'name':'start'}, request_id=request_id)))
    connection = client._connection(timeout=10)
    client.cut_connection = connection
    connection.request('POST', client.path, json.dumps(trigger), client._headers())
    client.cut_initialization = initialization
    client.cut_trigger = trigger
    client.cut_endpoints = dict(local=list(connection.sock.getsockname()), peer=list(connection.sock.getpeername()))


def close_http(client) -> None:
    if client.cut_connection is not None:
        client.cut_connection.close()
    client.close()


def listener_record(encoded: str, port: int, descriptors: dict[str, str]) -> dict:
    if not isinstance(encoded, str) or len(encoded.encode()) > 1_048_576:
        raise ValueError('original HTTP socket inventory exceeds its bound')
    local = '0100007F:' + format(port, '04X')
    matches = []
    for line in encoded.splitlines()[1:]:
        fields = line.split()
        if len(fields) >= 10 and fields[1:4] == [local, '00000000:0000', '0A']:
            for descriptor, target in descriptors.items():
                if target == 'socket:[' + fields[9] + ']':
                    matches.append(dict(fd=int(descriptor), inode=int(fields[9]), local=fields[1],
                        remote=fields[2], state=fields[3], row=line))
    if len(matches) != 1:
        raise ValueError('the original host does not uniquely own its exact loopback HTTP listener')
    return matches[0]


def validate_http(value: dict, original: dict, binary_hash: str) -> dict:
    port = value.get('port')
    listener = value.get('listener', {})
    executable = value.get('executable', {})
    endpoints = value.get('trigger_endpoints', {})
    if (value.get('original') != original or value.get('binary_sha256') != binary_hash
        or value.get('initialization', {}).get('protocolVersion') != PROTOCOL_VERSION
        or not isinstance(value.get('session'), str) or not 1 <= len(value['session']) <= 256
        or value.get('host') != '127.0.0.1' or value.get('path') != '/mcp'
        or type(port) is not int or not 0 < port < 65_536
        or executable != value.get('expected_executable')
        or any(type(executable.get(key)) is not int for key in ('device','inode','uid','mode'))
        or executable['inode'] <= 0 or executable['device'] < 0 or not stat.S_ISREG(executable['mode'])
        or type(listener.get('fd')) is not int or listener['fd'] < 0
        or type(listener.get('inode')) is not int or listener['inode'] <= 0
        or listener.get('local') != '0100007F:' + format(port, '04X')
        or listener.get('remote') != '00000000:0000' or listener.get('state') != '0A'
        or endpoints.get('peer') != ['127.0.0.1', port]
        or not isinstance(endpoints.get('local'), list) or len(endpoints['local']) != 2
        or endpoints['local'][0] != '127.0.0.1' or type(endpoints['local'][1]) is not int
        or not 0 < endpoints['local'][1] < 65_536):
        raise ValueError('original installed HTTP endpoint or initialization is unproven')
    descriptors = value.get('descriptors', {})
    if (not isinstance(descriptors, dict) or not 1 <= len(descriptors) <= 256
        or any(not isinstance(key, str) or not re.fullmatch('[0-9]+', key)
            or not isinstance(target, str) for key, target in descriptors.items())
        or listener_record(value.get('tcp'), port, descriptors) != listener):
        raise ValueError('original HTTP listener does not match its retained socket inventory')
    trigger = value.get('trigger', {})
    arguments = trigger.get('params', {}).get('arguments', {})
    if (trigger.get('jsonrpc') != '2.0' or trigger.get('method') != 'tools/call'
        or type(trigger.get('id')) is not int or trigger['id'] <= 0
        or trigger.get('params', {}).get('name') != 'instance_send'
        or arguments.get('event') != {'name':'start'}
        or not isinstance(arguments.get('instance_id'), str) or not arguments['instance_id']
        or not isinstance(arguments.get('request_id'), str) or not arguments['request_id']):
        raise ValueError('the original HTTP request is not one workflow trigger')
    return value


def capture_http(client, ready: dict, binary: Path) -> dict:
    process = Path('/proc', str(ready['original']['pid']))
    descriptors = list((process / 'fd').iterdir())
    if not 1 <= len(descriptors) <= 256:
        raise ValueError('original HTTP descriptor inventory exceeds its bound')
    targets = {}
    for descriptor in descriptors:
        try:
            targets[descriptor.name] = str(descriptor.readlink())
        except FileNotFoundError:
            # A captured /proc directory descriptor may disappear after listing;
            # a missing listener still fails the unique owned-socket check.
            continue
    with (process / 'net/tcp').open() as stream:
        tcp = stream.read(1_048_577)
    def metadata(path):
        value = path.stat()
        return dict(device=value.st_dev,inode=value.st_ino,uid=value.st_uid,mode=value.st_mode)
    value = dict(original=ready['original'],binary_sha256=ready['binary_sha256'],
        initialization=client.cut_initialization,trigger=client.cut_trigger,session=client.session,
        host=client.host,port=client.port,path=client.path,trigger_endpoints=client.cut_endpoints,
        executable=metadata(process / 'exe'),expected_executable=metadata(binary),
        descriptors=targets,tcp=tcp,listener=listener_record(tcp,client.port,targets))
    validate_http(value,ready['original'],ready['binary_sha256'])
    (client.cut_directory / 'http.json').write_text(json.dumps(value,sort_keys=True),encoding='utf-8')
    return value
