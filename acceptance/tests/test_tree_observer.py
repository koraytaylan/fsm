"""Labelled descendant/pipe controls; ordinary Python is not native proof."""
import copy
import importlib.util
import json
from pathlib import Path
import stat
import subprocess
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from acceptance.suite.executor_tree import tree_rows, NOISE_BYTES
from acceptance.suite.executor_lifecycle import process_observation
from acceptance.suite.fsm import Scratch


class TreeObserverTests(unittest.TestCase):
    def observations(self):
        parent=dict(run='original',resource='supplier',operation='validate',pid=123,pid_starttime='456')
        streams={str(number):dict(device=1,inode=100+number,mode=stat.S_IFSOCK|0o600) for number in (1,2)}
        child=dict(run='original',resource='supplier',pid=124,pid_starttime='457',parent_pid=123,
            parent=dict(pid=123,pid_starttime='456'),cgroup='0::/labelled.service\n',
            parent_cgroup='0::/labelled.service\n',streams=streams,parent_streams=copy.deepcopy(streams))
        noise=dict(run='original',bytes=NOISE_BYTES,descriptor=2,pid=123,pid_starttime='456')
        return parent,child,noise

    def check(self,parents,children,noise):
        with patch('acceptance.suite.executor_tree._fixture_rows',side_effect=[children,noise]):
            return tree_rows(Path('labelled'),parents)

    def test_missing_duplicate_or_foreign_descendants_and_noise_refuse(self):
        parent,child,noise=self.observations()
        self.assertEqual(self.check([parent],[child],[noise]),([child],[noise]))
        for children,floods in (([],[noise]),([child],[]),([child,child],[noise]),([child],[noise,noise])):
            with self.subTest(children=children,floods=floods),self.assertRaises(ValueError):
                self.check([parent],children,floods)
        for change in (dict(pid=True),dict(pid=123),dict(pid_starttime='unknown'),dict(parent_pid=True),
            dict(parent={'pid':125,'pid_starttime':'456'}),dict(parent_cgroup='0::/foreign.service\n'),
            dict(resource='foreign'),dict(run='foreign'),dict(parent_streams={})):
            with self.subTest(change=change),self.assertRaises(ValueError):
                self.check([parent],[{**child,**change}],[noise])

    def test_foreign_pipe_bytes_and_boolean_output_counts_refuse(self):
        parent,child,noise=self.observations()
        for change in (dict(bytes=True),dict(bytes=NOISE_BYTES-1),dict(pid=125),dict(pid=True),
            dict(pid_starttime='999'),dict(descriptor=True),dict(descriptor=1),dict(run='foreign')):
            with self.subTest(change=change),self.assertRaises(ValueError):
                self.check([parent],[child],[{**noise,**change}])
        for change in (dict(inode=True),dict(mode=stat.S_IFDIR|0o700),dict(device=-1),dict(inode=0)):
            value=copy.deepcopy(child);value['streams']['1'].update(change);value['parent_streams']['1'].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                self.check([parent],[value],[noise])

    def test_every_original_tree_cell_requires_both_kinds_and_all_three_hosts(self):
        from acceptance.suite.executor_scenarios import executor_original_descendants_and_noisy_pipes_close_before_replacement
        calls=[]
        with patch('acceptance.suite.executor_helper_cut.installed_helper_cut',
            side_effect=lambda report,kind,transport,cut,profile:calls.append((kind,transport,cut,profile))):
            executor_original_descendants_and_noisy_pipes_close_before_replacement(None)
        self.assertEqual(len(calls),6)
        self.assertEqual(set(calls),{(kind,transport,'candidate-before-fence','root-exit-retained-pipes')
            for kind in ('process','mcp') for transport in ('standalone','stdio','http')})

    def test_real_ordinary_descendant_inherits_parent_streams_and_retires_its_owned_child(self):
        fixture=Path(__file__).resolve().parents[1]/'fixtures/executor_handler.py'
        spec=importlib.util.spec_from_file_location('labelled_tree_fixture',fixture)
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        children=[];real_spawn=subprocess.Popen
        def spawn(*args,**kwargs):
            child=real_spawn(*args,**kwargs);children.append(child);return child
        with Scratch('labelled-ordinary-descendant') as scratch:
            args=SimpleNamespace(root=Path(scratch.path),run='labelled',resource='supplier',operation='validate',
                release=None,wait_seconds=5,failure='none',items=2,descendant=True,noise_bytes=0,
                observe_timing=False,fail_first=False,delay_ms=0)
            try:
                with patch.object(module.subprocess,'Popen',side_effect=spawn):
                    self.assertEqual(module.operation(args),0)
                self.assertEqual(len(children),1);self.assertIsNone(children[0].poll())
                child=json.loads((args.root/'descendants.jsonl').read_text())
                parent=json.loads((args.root/'entries.jsonl').read_text())
                self.assertEqual(child['pid'],children[0].pid)
                self.assertEqual(child['parent'],{key:parent[key] for key in ('pid','pid_starttime')})
                self.assertEqual(child['streams'],child['parent_streams'])
                self.assertEqual(child['cgroup'],child['parent_cgroup'])
                self.assertTrue(process_observation(child)['alive'])
                self.assertEqual(json.loads((args.root/'results.jsonl').read_text())['exit_code'],0)
            finally:
                for process in children:
                    if process.poll() is None:process.terminate()
                    process.wait(timeout=5)
            self.assertFalse(process_observation(child)['alive'])


if __name__ == '__main__':unittest.main()
