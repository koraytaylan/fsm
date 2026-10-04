"""Shared private native endpoint-route reader; not production discovery."""
from pathlib import Path
import stat


def socket_path(base):
    path = base / 'endpoint'
    metadata = path.lstat()
    assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid == 0 and not metadata.st_mode & 0o022
    with path.open() as source:
        value = source.read(4097)
    assert len(value) <= 4096
    fields = value.splitlines()
    assert len(fields) == 4 and fields[0] == 'endpoint/1'
    epoch = int(fields[1])
    assert 0 < epoch < 2**64 and len(fields[2]) == 36 and 0 < int(fields[3]) < 2**32
    assert fields[2] == Path('/proc/sys/kernel/random/boot_id').read_text().strip()
    return base / f'control-{epoch}.sock'
