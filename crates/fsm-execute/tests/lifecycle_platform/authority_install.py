"""Exclusive, owned-only native CI fixture installation, not product provisioning."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import uuid

TARGET = Path('/usr/libexec/fsm-containment-authority')


def protected_parent():
    for directory in (TARGET.parent, *TARGET.parent.parents):
        metadata = directory.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
            raise ValueError('fixture installation parent is not root protected')


def sync_parent():
    descriptor = os.open(TARGET.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def install(source, expected):
    descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, 'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError('authority artifact is not regular')
        encoded = stream.read(64 * 1024 * 1024 + 1)
    if len(encoded) > 64 * 1024 * 1024 or hashlib.sha256(encoded).hexdigest() != expected:
        raise ValueError('authority artifact exceeds bound or differs from frozen build')
    temporary = TARGET.with_name('.fsm-native-probe-' + uuid.uuid4().hex)
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(descriptor, 'wb') as stream:
            stream.write(encoded)
            stream.flush()
            os.fchmod(stream.fileno(), 0o711)
            os.fsync(stream.fileno())
            metadata = os.fstat(stream.fileno())
        # Existing product installations are never overwritten or reused.
        os.link(temporary, TARGET, follow_symlinks=False)
        sync_parent()
    finally:
        temporary.unlink()
        sync_parent()
    return {'device': metadata.st_dev, 'inode': metadata.st_ino, 'sha256': expected}


def remove(device, inode, expected):
    descriptor = os.open(TARGET, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, 'rb') as stream:
        metadata = os.fstat(stream.fileno())
        encoded = stream.read(64 * 1024 * 1024 + 1)
    current = TARGET.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022 \
            or (metadata.st_dev, metadata.st_ino) != (device, inode) \
            or (current.st_dev, current.st_ino) != (device, inode) \
            or not stat.S_ISREG(current.st_mode) \
            or current.st_uid != metadata.st_uid or current.st_gid != metadata.st_gid \
            or current.st_mode != metadata.st_mode \
            or len(encoded) > 64 * 1024 * 1024 \
            or hashlib.sha256(encoded).hexdigest() != expected:
        raise ValueError('fixture refuses to remove changed authority installation')
    TARGET.unlink()
    sync_parent()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('install', 'remove'))
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--source', type=Path)
    parser.add_argument('--device', type=int)
    parser.add_argument('--inode', type=int)
    args = parser.parse_args()
    if os.geteuid() != 0 or not re.fullmatch(r'[0-9a-f]{64}', args.sha256):
        parser.error('root and exact artifact digest required')
    protected_parent()
    if args.operation == 'install':
        if args.source is None or args.device is not None or args.inode is not None:
            parser.error('install requires only source and digest')
        print(json.dumps(install(args.source, args.sha256)))
    else:
        if args.source is not None or args.device is None or args.inode is None:
            parser.error('remove requires only owned identity and digest')
        remove(args.device, args.inode, args.sha256)


if __name__ == '__main__':
    main()
