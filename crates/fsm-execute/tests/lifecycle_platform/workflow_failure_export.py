"""Read bounded retained workflow diagnostics without retiring original state."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

# Completed scenarios retain per-allocation observations alongside diagnostics;
# cap the entire directory separately from the 64 exported diagnostic files.
STAGE_ENTRY_CAP = 1024

def read_stage(stage):
    stage = Path(stage)
    if stage.parent != Path('/usr/libexec') or not re.fullmatch(r'fsm-(workflow|crash)-[0-9a-f]{1,64}', stage.name):
        raise ValueError('unexpected retained workflow stage')
    before = stage.lstat()
    if not stat.S_ISDIR(before.st_mode) or before.st_uid != 0 or before.st_mode & 0o022:
        raise ValueError('retained stage is not root protected')
    records = []
    entries = []
    for path in stage.iterdir():
        if len(entries) == STAGE_ENTRY_CAP:
            raise ValueError('retained stage entry count exceeds bound')
        entries.append(path)
    for path in sorted(entries):
        if not (path.name.endswith('.inventory.json') or
                path.name.startswith('failure-') and path.suffix in ('.log', '.json') or
                stage.name.startswith('fsm-crash-') and re.fullmatch(
                    r'(standalone|embedded)-(process|mcp)(-(hold-result|noisy-result|collected-timeout|collected-result|supervisor-death|closed-result))?\.log', path.name)):
            continue
        if len(records) >= 64:
            raise ValueError('retained diagnostic count exceeds bound')
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor, 'rb') as source:
            metadata = os.fstat(source.fileno())
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
                raise ValueError('diagnostic is not a root-protected regular file')
            encoded = source.read(65537)
            after = os.fstat(source.fileno())
        if len(encoded) > 65536 or (metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise ValueError('diagnostic exceeds bound or changed during read')
        records.append(dict(name=path.name, device=metadata.st_dev, inode=metadata.st_ino,
                            sha256=hashlib.sha256(encoded).hexdigest(), hex=encoded.hex()))
    after = stage.lstat()
    if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
        raise ValueError('retained stage identity changed')
    return dict(stage=str(stage), device=before.st_dev, inode=before.st_ino, files=records)


def export(stages, destination):
    """Copy diagnostic bytes into user-readable CI evidence; leave originals intact."""
    if len(stages) > 8:
        raise ValueError('retained stage count exceeds bound')
    results = []
    for index, stage in enumerate(sorted(stages)):
        encoded = subprocess.check_output(
            ['sudo', '-n', sys.executable, str(Path(__file__).resolve()), str(stage)], timeout=10)
        if len(encoded) > 9 * 1024 * 1024:
            raise ValueError('retained diagnostic export exceeds bound')
        record = json.loads(encoded)
        # Preserve the original paths and identities in the exported snapshot.
        prefix = 'crash' if stage.name.startswith('fsm-crash-') else 'workflow'
        path = destination / f'{prefix}-failure-{index}.json'
        with path.open('xb') as target:
            target.write(encoded)
        results.append(dict(stage=str(stage), snapshot=path.name,
                            sha256=hashlib.sha256(encoded).hexdigest(), files=len(record['files'])))
    return results


if __name__ == '__main__':
    assert os.geteuid() == 0 and len(sys.argv) == 2
    print(json.dumps(read_stage(sys.argv[1]), sort_keys=True))
