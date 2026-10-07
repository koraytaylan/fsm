"""Independent export controls; no original native state is created or removed."""
import hashlib
import json
import os
from pathlib import Path
import tempfile
import stat
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import workflow_failure_export as exporter


class Export(unittest.TestCase):
    def test_protected_observations_are_copied_with_their_digest_and_original_identity(self):
        record = dict(stage='/usr/libexec/fsm-workflow-aabb', device=1, inode=2,
                      files=[dict(name='failure-aabb-native.json', device=1, inode=3,
                                  sha256=hashlib.sha256(b'original observation').hexdigest(),
                                  hex=b'original observation'.hex())])
        encoded = json.dumps(record).encode()
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as scratch:
            destination = Path(scratch)
            with patch.object(exporter.subprocess, 'check_output', return_value=encoded) as reader:
                result = exporter.export({Path(record['stage'])}, destination)
            reader.assert_called_once()
            self.assertEqual(reader.call_args.kwargs['timeout'], 10)
            self.assertEqual(result[0]['sha256'], hashlib.sha256(encoded).hexdigest())
            self.assertEqual((destination / result[0]['snapshot']).read_bytes(), encoded)
            self.assertEqual(json.loads(encoded)['inode'], 2)

    def test_unexpected_stage_path_refuses_before_reading(self):
        for path in ['/home/not-a-workflow', '/usr/libexec/fsm-workflow-not-hex']:
            with self.assertRaisesRegex(ValueError, 'unexpected retained'):
                exporter.read_stage(path)

    def test_stage_count_bound_refuses_before_privileged_read(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as scratch:
            with patch.object(exporter.subprocess, 'check_output') as reader:
                with self.assertRaisesRegex(ValueError, 'stage count'):
                    exporter.export({Path(f'/usr/libexec/fsm-workflow-{index:x}') for index in range(9)}, Path(scratch))
                reader.assert_not_called()

    def test_directory_entry_bound_refuses_before_opening_any_file(self):
        path = '/usr/libexec/fsm-workflow-aa'
        stage = SimpleNamespace(parent=Path('/usr/libexec'), name='fsm-workflow-aa',
                                lstat=lambda: SimpleNamespace(st_mode=stat.S_IFDIR | 0o700, st_uid=0),
                                iterdir=lambda: iter(Path(f'entry-{index}') for index in range(129)))
        with patch.object(exporter, 'Path', side_effect=lambda value: stage if value == path else Path(value)):
            with patch.object(exporter.os, 'open') as opened:
                with self.assertRaisesRegex(ValueError, 'entry count'):
                    exporter.read_stage(path)
                opened.assert_not_called()

    def test_existing_export_is_never_overwritten(self):
        with tempfile.TemporaryDirectory(dir=os.environ['TMPDIR']) as scratch:
            destination = Path(scratch)
            original = destination / 'workflow-failure-0.json'
            original.write_bytes(b'previous evidence')
            with patch.object(exporter.subprocess, 'check_output', return_value=b'{"files": []}'):
                with self.assertRaises(FileExistsError):
                    exporter.export({Path('/usr/libexec/fsm-workflow-aa')}, destination)
            self.assertEqual(original.read_bytes(), b'previous evidence')


if __name__ == '__main__':
    unittest.main()
