"""Verify the exact historical source archive without creating a branch or worktree."""
import hashlib
from pathlib import Path
import stat
import subprocess

ORIGINAL_COMMIT = '5730f17202cdeabd8c34f9b1c48fcf02f26b0e06'


def verify(repo, directory):
    """Match every archive path, executable mode and blob to the frozen Git tree."""
    directory = Path(directory)
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError('historical source must be a real archive directory')
    rows = subprocess.check_output(
        ['git', 'ls-tree', '-rz', '--full-tree', ORIGINAL_COMMIT],
        cwd=repo, timeout=30)
    expected = {}
    for row in rows.split(b'\0'):
        if not row:
            continue
        header, relative = row.split(b'\t', 1)
        mode, kind, digest = header.decode('ascii').split()
        relative = relative.decode('utf-8')
        path = Path(relative)
        if kind != 'blob' or mode not in ('100644', '100755') or path.is_absolute() or '..' in path.parts:
            raise ValueError('unsupported historical source tree entry')
        if relative in expected:
            raise ValueError('duplicate historical source tree entry')
        expected[relative] = (mode, digest)
    if not expected:
        raise ValueError('historical source tree is empty')
    expected_directories = {parent.as_posix()
                            for relative in expected
                            for parent in Path(relative).parents if parent != Path('.')}
    observed = {}
    observed_directories = set()
    for path in directory.rglob('*'):
        metadata = path.lstat()
        if stat.S_ISDIR(metadata.st_mode):
            observed_directories.add(path.relative_to(directory).as_posix())
            continue
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 64 * 1024 * 1024:
            raise ValueError('historical archive contains unsupported file type or size')
        relative = path.relative_to(directory).as_posix()
        with path.open('rb') as source:
            content = source.read(64 * 1024 * 1024 + 1)
        if len(content) != metadata.st_size:
            raise ValueError('historical source changed while reading')
        digest = hashlib.sha1(b'blob ' + str(len(content)).encode() + b'\0' + content).hexdigest()
        mode = '100755' if metadata.st_mode & 0o111 else '100644'
        observed[relative] = (mode, digest)
    if observed != expected or observed_directories != expected_directories:
        raise ValueError('historical source archive differs from frozen Git tree')
    return ORIGINAL_COMMIT
