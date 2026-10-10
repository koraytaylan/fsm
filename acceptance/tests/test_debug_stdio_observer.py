"""Labelled descriptor/protocol controls, never installed native execution."""
import copy
import json
from pathlib import Path
import stat
import subprocess
import sys
from types import SimpleNamespace
import unittest

from acceptance.suite.executor_debug_stdio import StdioPipes, validate_stdio
from acceptance.suite.fsm import Scratch
from acceptance.suite.mcp import StdioClient, PROTOCOL_VERSION
from acceptance.suite.installed import retain_failed_stores


class DebugStdioTests(unittest.TestCase):
    def observation(self):
        executable=dict(device=1,inode=10,uid=1000,mode=stat.S_IFREG|0o755)
        value=dict(original={'pid':123,'pid_starttime':'456'},binary_sha256='a'*64,
            initialization={'protocolVersion':PROTOCOL_VERSION},executable=executable,
            expected_executable=copy.deepcopy(executable),streams={})
        for number,name in enumerate(('stdin','stdout','stderr')):
            expected=dict(device=1,inode=20+number,uid=1000,mode=stat.S_IFIFO|0o600)
            value['streams'][name]=dict(fd=number,actual=expected,expected=copy.deepcopy(expected))
        return value

    def test_debugger_output_or_foreign_pipes_cannot_prove_original_stdio(self):
        original=self.observation();self.assertEqual(validate_stdio(original,original['original'],'a'*64),original)
        for change in (dict(original={'pid':124,'pid_starttime':'456'}),dict(binary_sha256='b'*64),
            dict(initialization={}),dict(executable={'inode':11})):
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_stdio({**original,**change},original['original'],'a'*64)
        value=copy.deepcopy(original);value['executable']['inode']=11
        with self.assertRaises(ValueError):validate_stdio(value,original['original'],'a'*64)
        for change in (dict(inode=99),dict(mode=stat.S_IFREG|0o600),dict(uid=0),dict(uid=True)):
            value=copy.deepcopy(original);value['streams']['stdout']['actual'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_stdio(value,original['original'],'a'*64)

    def test_shared_or_public_endpoints_cannot_prove_independent_stdio_streams(self):
        for change in (dict(mode=stat.S_IFIFO|0o666),dict(inode=20),dict(mode=stat.S_IFREG|0o600)):
            value=self.observation()
            value['streams']['stdout']['actual'].update(change);value['streams']['stdout']['expected'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_stdio(value,value['original'],'a'*64)
        value=self.observation();value['streams']['stdin']['fd']=False
        with self.assertRaises(ValueError):validate_stdio(value,value['original'],'a'*64)

    def test_attached_real_fifo_protocol_retires_its_labelled_ordinary_child(self):
        script="""import json,os,sys
from pathlib import Path
directory=Path(sys.argv[1])
for number,name in enumerate(('stdin','stdout','stderr')):
 descriptor=os.open(directory/(name+'.fifo'),os.O_RDONLY if number==0 else os.O_WRONLY)
 os.dup2(descriptor,number);os.close(descriptor)
for line in sys.stdin:
 value=json.loads(line)
 if 'id' in value:
  print(json.dumps(dict(jsonrpc='2.0',id=value['id'],result={'protocolVersion':'2025-06-18'})),flush=True)
"""
        with Scratch('labelled-fifo-client') as scratch, StdioPipes(Path(scratch.path)) as pipes:
            child=subprocess.Popen([sys.executable,'-c',script,scratch.path],stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            client=None
            try:
                process=SimpleNamespace(stdin=pipes.streams[0],stdout=pipes.streams[1],stderr=pipes.streams[2],
                    poll=child.poll,wait=child.wait,kill=child.kill)
                client=StdioClient.from_process(process)
                self.assertEqual(client.initialize()['protocolVersion'],PROTOCOL_VERSION)
                pipes.release_guards();client.close()
                self.assertEqual(child.returncode,0)
                self.assertFalse(client._reader.worker.is_alive())
                self.assertFalse(client._error_worker.is_alive())
            finally:
                pipes.release_guards()
                if child.poll() is None:child.kill();child.wait(timeout=5)
                if client is not None:client.close()

    def test_failed_store_retains_original_bytes_and_fifo_identities_without_reading_streams(self):
        with Scratch('labelled-fifo-retention') as scratch:
            root=Path(scratch.path);store=root/'fsm-acceptance-settlement-cut-installed-labelled'
            (store/'debugger').mkdir(parents=True);(store/'store').mkdir()
            (store/'store'/'journal.jsonl').write_bytes(b'labelled original failure bytes\n')
            import os
            for name in ('stdin','stdout','stderr'):os.mkfifo(store/'debugger'/(name+'.fifo'),0o600)
            evidence=root/'evidence';evidence.mkdir()
            retain_failed_stores(root,evidence,1)
            retained=evidence/('failed-'+store.name)
            self.assertEqual((retained/'store'/'journal.jsonl').read_bytes(),b'labelled original failure bytes\n')
            identities=json.loads((retained/'original-stdio-endpoints.json').read_text())
            self.assertEqual({row['path'] for row in identities},{'debugger/'+name+'.fifo' for name in ('stdin','stdout','stderr')})
            for row in identities:
                self.assertEqual(row['inode'],(store/row['path']).stat().st_ino)
                self.assertFalse((retained/row['path']).exists())
