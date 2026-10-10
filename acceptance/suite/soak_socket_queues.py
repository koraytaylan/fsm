"""Observe exact owned Unix socket queues through Linux's diagnostic ABI.

Every request names one procfs-observed inode, never a socket dump; the
observer does not duplicate, read, write or shut down a candidate endpoint.
Queue bytes, retained capture prefixes in the process heap, and retained
journal output have different accounting units and must remain distinct.
"""
import os
from pathlib import Path
import socket
import stat
import struct
import sys
import time

from .soak import bounded_integer
from .soak_resources import observe_live_host, _read, _stat

NETLINK_SOCK_DIAG = 4
SOCK_DIAG_BY_FAMILY = 20
UNIX_DIAG_RQLEN = 4
UNIX_DIAG_PEER = 2


def decode_queue(raw, inode, sequence, port):
    """Require one exact response and the requested queue-length attribute."""
    if not isinstance(raw, bytes) or not 32 <= len(raw) <= 4096:
        raise ValueError('required Unix socket diagnostic is missing or oversized')
    length, kind, flags, observed_sequence, observed_port = struct.unpack('=IHHII', raw[:16])
    if (length != len(raw) or flags != 0 or observed_sequence != sequence or observed_port != port):
        raise ValueError('Unix socket diagnostic changed its request identity')
    if kind == 2:
        raise ValueError('required Unix socket diagnostic returned a kernel error')
    if kind != SOCK_DIAG_BY_FAMILY:
        raise ValueError('required Unix socket diagnostic has an unexpected message type')
    family, socket_kind, state, padding, observed_inode, cookie_low, cookie_high = struct.unpack('=BBBBIII',raw[16:32])
    if family != socket.AF_UNIX or socket_kind != socket.SOCK_STREAM or padding or observed_inode != inode:
        raise ValueError('Unix capture diagnostic names a foreign or unsupported endpoint')
    position, attributes = 32, {}
    while position < length:
        if length-position < 4:
            raise ValueError('Unix socket diagnostic attribute is truncated')
        attribute_length, attribute_type = struct.unpack('=HH',raw[position:position+4])
        rounded = (attribute_length+3) & ~3
        if attribute_length < 4 or position+rounded > length or attribute_type in attributes:
            raise ValueError('Unix socket diagnostic attributes are malformed or duplicated')
        attributes[attribute_type] = raw[position+4:position+attribute_length]
        position += rounded
    if len(attributes.get(UNIX_DIAG_RQLEN,b'')) != 8:
        raise ValueError('required Unix capture queue metric is unavailable')
    queued, outbound = struct.unpack('=II',attributes[UNIX_DIAG_RQLEN])
    # A listener reports connection counts, not bytes; retain that observation
    # explicitly without folding its accounting unit into capture pressure.
    peer = attributes.get(UNIX_DIAG_PEER)
    if peer is not None and len(peer) != 4:
        raise ValueError('Unix socket diagnostic peer identity is malformed')
    return dict(inode=inode, queued_bytes=queued if state != 10 else None,
                queued_connections=queued if state == 10 else None,
                state=state, cookie=[cookie_low,cookie_high],
                peer_inode=struct.unpack('=I',peer)[0] if peer is not None else None,
                raw_hex=raw.hex())


def _queue(inode, sequence, observer, timeout):
    observer.settimeout(timeout)
    payload = struct.pack('=BBHIIIII',socket.AF_UNIX,0,0,0xffffffff,inode,0x14,0xffffffff,0xffffffff)
    # NLM_F_REQUEST only; NLM_F_DUMP would enumerate unrelated sockets.
    request = struct.pack('=IHHII',16+len(payload),SOCK_DIAG_BY_FAMILY,1,sequence,0)+payload
    observer.sendto(request,(0,0))
    raw, _, flags, sender = observer.recvmsg(4096)
    if flags & socket.MSG_TRUNC or sender != (0,0):
        raise ValueError('Unix capture diagnostic is truncated or has a foreign sender')
    return decode_queue(raw,inode,sequence,observer.getsockname()[0])


def observe_unix_queues(identity):
    """Measure a live original host's Unix stream receive queues once per inode."""
    if sys.platform != 'linux':
        raise ValueError('required Unix capture queue observation needs Linux')
    # Verify the kernel exposes the Unix fdinfo discriminator before ignoring
    # descriptors of other socket families; no unavailable metric becomes zero.
    probe, peer = socket.socketpair()
    with probe, peer:
        if b'scm_fds:' not in _read(Path('/proc/self/fdinfo')/str(probe.fileno()),4096):
            raise ValueError('required Unix fdinfo accounting is unavailable')
    host = observe_live_host(identity)
    root = Path('/proc')/str(identity['pid'])
    deadline, endpoints = time.monotonic()+2, {}
    with socket.socket(socket.AF_NETLINK,socket.SOCK_RAW,NETLINK_SOCK_DIAG) as observer:
        for number in host['descriptors']:
            remaining = deadline-time.monotonic()
            if remaining <= 0:
                raise TimeoutError('required Unix capture queue observation exceeded its deadline')
            target = os.readlink(root/'fd'/number)
            if not target.startswith('socket:['):
                continue
            info = _read(root/'fdinfo'/number,4096)
            if b'scm_fds:' not in info:
                continue
            if not target.endswith(']') or not target[8:-1].isascii() or not target[8:-1].isdecimal():
                raise ValueError('original Unix socket descriptor identity is malformed')
            inode = bounded_integer(int(target[8:-1]),1,(1<<32)-1,'original Unix socket inode')
            metadata = (root/'fd'/number).stat()
            if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_ino != inode:
                raise ValueError('original Unix socket descriptor identity changed')
            if inode in endpoints:
                endpoints[inode]['descriptors'].append(number)
                continue
            value = _queue(inode,len(endpoints)+1,observer,remaining)
            value.update(descriptors=[number],fdinfo=info.decode('ascii'))
            endpoints[inode] = value
        after = _read(root/'stat',4096)
        _stat(after,identity)
    total = sum(value['queued_bytes'] for value in endpoints.values() if value['queued_bytes'] is not None)
    return dict(identity=dict(identity),queued_unix_bytes=total,endpoints=list(endpoints.values()),
                stat_before=host['stat_before'],stat_after=after.decode('utf-8'))
