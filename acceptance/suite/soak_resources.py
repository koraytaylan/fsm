"""Bounded Linux host observations; unavailable metrics are never zero-filled.

These observations cover a birth-matched live execution owner, including
thread-owned direct children; independently contained handler domains still
need their own fixture identities and authenticated original closure records.
"""
from itertools import islice
import os
from pathlib import Path

from .soak import bounded_integer


def _read(path, maximum=65_536):
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    if len(raw) > maximum:
        raise ValueError('resource observation exceeds its byte bound')
    return raw


def _stat(raw, identity):
    prefix, separator, tail = raw.rpartition(b') ')
    fields = tail.split()
    if (not separator or prefix.split(b' (', 1)[0] != str(identity['pid']).encode()
        or len(fields) < 22 or fields[0] not in (b'R', b'S', b'D', b'T', b't', b'I', b'P')
        or not fields[19].isdigit() or fields[19].decode('ascii') != identity['pid_starttime']
        or any(not fields[index].isdigit() for index in (11, 12))):
        raise ValueError('resource observation lost its original live host identity')
    return fields


def _inventory(directory, maximum):
    with os.scandir(directory) as stream:
        names = [entry.name for entry in islice(stream, maximum + 1)]
    if len(names) > maximum or any(not name.isascii() or not name.isdecimal() for name in names):
        raise ValueError('resource inventory is malformed or exceeds its bound')
    return sorted(names, key=int)


def observe_live_host(identity, proc_root=Path('/proc')):
    """Retain original procfs bytes and counts between two matching birth reads."""
    if (not isinstance(identity, dict) or set(identity) != {'pid', 'pid_starttime'}
        or not isinstance(identity.get('pid_starttime'), str)
        or not identity['pid_starttime'].isascii() or not identity['pid_starttime'].isdecimal()):
        raise ValueError('an exact original host identity is required')
    bounded_integer(identity['pid'], 1, (1 << 31) - 1, 'host PID')
    root = Path(proc_root) / str(identity['pid'])
    before = _read(root / 'stat', 4096)
    initial_fields = _stat(before, identity)
    status = _read(root / 'status')
    rss = [line.split() for line in status.splitlines() if line.startswith(b'VmRSS:')]
    if (len(rss) != 1 or len(rss[0]) != 3 or rss[0][2] != b'kB' or not rss[0][1].isdigit()):
        raise ValueError('required resident-memory observation is unavailable')
    descriptors = _inventory(root / 'fd', 4096)
    children = {}
    threads = _inventory(root / 'task', 256)
    if str(identity['pid']) not in threads:
        raise ValueError('required original host thread inventory is unavailable')
    for thread in threads:
        raw = _read(root / 'task' / thread / 'children', 4096)
        names = raw.split()
        if (len(names) > 256 or any(not name.isdigit() or not 0 < int(name) < (1 << 31) for name in names)
            or len(set(names)) != len(names)):
            raise ValueError('thread-owned child inventory is malformed or oversized')
        children[thread] = [int(name) for name in names]
    after = _read(root / 'stat', 4096)
    fields = _stat(after, identity)
    if any(int(fields[index]) < int(initial_fields[index]) for index in (11, 12)):
        raise ValueError('original host CPU accounting regressed')
    all_children = [pid for values in children.values() for pid in values]
    if len(set(all_children)) != len(all_children):
        raise ValueError('thread-owned child inventory is inconsistent')
    frequency = bounded_integer(os.sysconf('SC_CLK_TCK'), 1, 1_000_000, 'CPU clock frequency')
    metrics = dict(rss_bytes=int(rss[0][1]) * 1024, file_descriptors=len(descriptors),
                   active_children=len(all_children),
                   cpu_ns=(int(fields[11]) + int(fields[12])) * 1_000_000_000 // frequency)
    for key, value in metrics.items():
        bounded_integer(value, 0, (1 << 63) - 1, key)
    return dict(identity=dict(identity), metrics=metrics, stat_before=before.decode('utf-8'),
                stat_after=after.decode('utf-8'), status=status.decode('utf-8'),
                descriptors=descriptors, thread_children=children, clock_ticks_per_second=frequency)
