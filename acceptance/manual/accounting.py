"""Reconcile genuine model tool attempts with original underlying MCP calls.

A client-side refusal or cached result still consumes one model call; multiple
underlying requests consume their full inventory, including failed requests.
Initialization after brief delivery is deliberately counted conservatively.
"""
from collections import Counter
import hashlib
import json


def request_signature(method, parameters):
    parameters = parameters if isinstance(parameters, dict) else {}
    if method == 'tools/call':
        parameters = {key: parameters.get(key) for key in ('name', 'arguments')}
    elif method == 'resources/read':
        parameters = {'uri': parameters.get('uri')}
    elif method == 'resources/list':
        parameters = {}
    encoded = json.dumps([method, parameters], sort_keys=True, separators=(',', ':'))
    return hashlib.sha256(encoded.encode()).hexdigest()


def model_signature(name, arguments):
    if name.startswith('mcp__fsm__'):
        return request_signature('tools/call', dict(name=name.removeprefix('mcp__fsm__'), arguments=arguments))
    if name == 'ReadMcpResourceTool':
        return request_signature('resources/read', arguments)
    if name == 'ListMcpResourcesTool':
        return request_signature('resources/list', {})
    return None


def model_call_count(requests, signatures):
    if (not isinstance(signatures, list) or len(signatures) > 41
        or any(value is not None and (not isinstance(value, str) or len(value) != 64
               or any(character not in '0123456789abcdef' for character in value)) for value in signatures)):
        raise RuntimeError('original model tool attempt inventory is missing or invalid')
    requests = [frame for frame in requests if isinstance(frame.get('method'), str)
                and frame['method'].startswith(('tools/', 'resources/', 'prompts/'))]
    inventory = Counter(request_signature(frame['method'], frame.get('params')) for frame in requests)
    calls = len(requests)
    for signature in signatures:
        if signature is not None and inventory[signature]:
            inventory[signature] -= 1
        else:
            calls += 1
    return calls
