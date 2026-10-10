"""Historical archive substitution must fail before old candidate builds run."""
import hashlib
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import workflow_upgrade_source as source


class FrozenArchive(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(dir=os.environ['TMPDIR'], prefix='fsm-upgrade-archive-')
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.content = b'independent historical source\n'
        (self.root / 'source.rs').write_bytes(self.content)
        (self.root / 'source.rs').chmod(0o644)
        self.digest = hashlib.sha1(b'blob ' + str(len(self.content)).encode() + b'\0' + self.content).hexdigest()
        self.tree = f'100644 blob {self.digest}\tsource.rs\0'.encode()

    def verify(self, tree=None):
        with patch.object(source.subprocess, 'check_output', return_value=self.tree if tree is None else tree) as command:
            result = source.verify(Path('/fixture/repo'), self.root)
        self.assertEqual(command.call_args.args[0], ['git', 'ls-tree', '-rz', '--full-tree', source.ORIGINAL_COMMIT])
        self.assertEqual(command.call_args.kwargs['timeout'], 30)
        return result

    def test_accepts_only_the_frozen_complete_tree(self):
        self.assertEqual(self.verify(), source.ORIGINAL_COMMIT)

    def test_refuses_changed_source_even_when_file_inventory_matches(self):
        (self.root / 'source.rs').write_bytes(b'changed historical source\n')
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.verify()

    def test_refuses_missing_extra_or_git_metadata_files(self):
        for name in ['extra.rs', '.git']:
            with self.subTest(name=name):
                extra = self.root / name
                extra.write_text('extra archive bytes')
                with self.assertRaisesRegex(ValueError, 'differs'):
                    self.verify()
                extra.unlink()
        (self.root / 'source.rs').unlink()
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.verify()

    def test_refuses_changed_executable_mode(self):
        (self.root / 'source.rs').chmod(0o755)
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.verify()

    def test_refuses_extra_empty_metadata_directory(self):
        (self.root / '.git').mkdir()
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.verify()

    def test_refuses_symlink_substitution(self):
        path = self.root / 'source.rs'
        path.unlink()
        path.symlink_to('absent')
        with self.assertRaisesRegex(ValueError, 'unsupported file type'):
            self.verify()

    def test_refuses_empty_nonblob_duplicate_and_traversal_tree_entries(self):
        for tree in [b'', self.tree + self.tree,
                     self.tree.replace(b'100644 blob', b'160000 commit'),
                     self.tree.replace(b'source.rs', b'../source.rs')]:
            with self.subTest(tree=tree), self.assertRaises(ValueError):
                self.verify(tree)


if __name__ == '__main__':
    unittest.main()
