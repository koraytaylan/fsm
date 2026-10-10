"""Auxiliary operational accounting and native-invocation refusal controls."""
import copy
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch, MagicMock

from acceptance.suite.fsm import Scratch
from acceptance.suite.metrics import METRICS, GROWTH, validate_sample
from acceptance.suite.soak_run import observed_sample, disk_size, workload_digest
from acceptance.suite.soak_native_metrics import queue_bytes, ROOT_SAMPLE, sample_native
from acceptance.suite.soak import schedule
from acceptance.suite.soak_installed import run_block


def census():
    process=dict(host=dict(identity={'pid':123,'pid_starttime':'456'},
        metrics=dict(rss_bytes=100,file_descriptors=3,active_children=0,cpu_ns=20)),
        pipes={'pipes':[]},unix={'endpoints':[]})
    return dict(owner=process,processes=[],descendants=[],disk_bytes=7)


class AccountingTests(unittest.TestCase):
    def observation(self):
        return dict(active=census(),quiescent=census(),warmed=census(),
            schedule=dict(handler_kind='process',transport='stdio'),started_ns=10,
            completed_ns=30,control_latency_ns=2,scheduler_lag_ns=3)

    def test_archive_and_native_bytes_remain_accounted_with_exact_descendant_identities(self):
        with Scratch('ordinary-operational-accounting') as scratch:
            root=Path(scratch.path);store=root/'store';(store/'journal').mkdir(parents=True)
            (store/'journal/seg').write_bytes(b'abc')
            (root/'archives').mkdir();(root/'archives/retained').write_bytes(b'original')
            observation=self.observation()
            observation['active']['processes']=[copy.deepcopy(observation['active']['owner'])]
            observation['active']['descendants']=[dict(pid=123,pid_starttime='456',alive=True)]
            active=observed_sample(observation,'active',store,root)
            self.assertEqual(set(active['metrics']),set(METRICS))
            self.assertEqual(active['metrics']['journal_bytes'],11)
            self.assertEqual(active['metrics']['disk_bytes'],18)
            self.assertEqual(active['metrics']['surviving_descendants'],1)
            self.assertEqual(active['metrics']['rss_bytes'],200)
            observation['active']['descendants']=[]
            self.assertEqual(observed_sample(observation,'active',store,root)['metrics']['surviving_descendants'],0)

    def test_warmed_growth_uses_the_original_host_birth_and_rejects_leaks(self):
        with Scratch('ordinary-operational-growth') as scratch:
            root=Path(scratch.path);store=root/'store';(store/'journal').mkdir(parents=True)
            observation=self.observation()
            warmed=observed_sample(observation,'warmed',store,root)
            calibration=dict(ceilings={key:10000 for key in METRICS},quiescent_growth={key:10 for key in GROWTH})
            observation['quiescent']['owner']['host']['metrics']['rss_bytes']=111
            with self.assertRaisesRegex(ValueError,'growth'):validate_sample(observed_sample(observation,'quiescent',store,root),calibration,warmed)
            observation['quiescent']['owner']['host']['metrics']['rss_bytes']=100
            observation['quiescent']['owner']['host']['identity']['pid_starttime']='789'
            with self.assertRaisesRegex(ValueError,'equivalent'):validate_sample(observed_sample(observation,'quiescent',store,root),calibration,warmed)
            observation=self.observation();observation['quiescent']['descendants']=[{'alive':True}]
            with self.assertRaisesRegex(ValueError,'retired'):validate_sample(observed_sample(observation,'quiescent',store,root),calibration)

    def test_queue_deduplication_preserves_bytes_and_excludes_listener_connections(self):
        value=census();value['owner']['pipes']['pipes']=[dict(device=1,inode=2,queued_bytes=5)]
        value['owner']['unix']['endpoints']=[dict(inode=3,queued_bytes=None),dict(inode=4,queued_bytes=7)]
        value['processes']=[copy.deepcopy(value['owner'])]
        self.assertEqual(queue_bytes(value),12)

    def test_missing_disk_or_symlink_cannot_be_a_zero_observation(self):
        with Scratch('ordinary-operational-disk') as scratch:
            root=Path(scratch.path)
            with self.assertRaises(ValueError):disk_size(root/'absent')
            (root/'link').symlink_to(root)
            with self.assertRaises(ValueError):disk_size(root)

    def test_workload_digest_is_stable_and_seed_bound_without_calibration_self_reference(self):
        repository=Path(__file__).resolve().parents[2]
        document=json.loads((repository/'acceptance/profiles/operational.json').read_text())
        first=workload_digest(repository,document['profiles'],123)
        self.assertEqual(first,workload_digest(repository,document['profiles'],123))
        self.assertNotEqual(first,workload_digest(repository,document['profiles'],124))
        self.assertNotEqual(first,workload_digest(repository,document['profiles'],123,'sustained'))

    def test_protected_sampler_has_valid_syntax_and_cannot_be_invoked_locally(self):
        compile(ROOT_SAMPLE,'protected-sampler','exec')
        with patch.dict(os.environ,{'GITHUB_ACTIONS':'false'},clear=False):
            with self.assertRaisesRegex(RuntimeError,'disposable'):sample_native(None,{'pid':123,'pid_starttime':'456'})

    def test_passing_mid_block_stops_new_work_and_still_drains_and_verifies(self):
        with Scratch('ordinary-block-stop-control') as scratch:
            native=MagicMock();native.cache=Path(scratch.path);native.cleaned=True
            authority=MagicMock();authority.__enter__.return_value=native
            block=MagicMock();block.run.side_effect=lambda entry:entry
            calls=[]
            def consume(entry):calls.append(entry);return len(calls)<3
            report=MagicMock()
            with patch('acceptance.suite.soak_installed.require_disposable_runner'), \
                 patch('acceptance.suite.soak_installed.DisposableAuthority',return_value=authority), \
                 patch('acceptance.suite.soak_installed.block_inputs',return_value={'machines':{},'table':{}}), \
                 patch('acceptance.suite.soak_installed.stage_sampler'), \
                 patch('acceptance.suite.soak_installed.InstalledBlock',return_value=block), \
                 patch('acceptance.suite.soak_installed.fsm.run_json',side_effect=[{'health':'Ok'},{'agreement':True}]):
                run_block(report,Path(scratch.path)/'store',list(schedule(123,12)),consume)
            self.assertEqual(len(calls),3)
            self.assertEqual(block.run.call_count,3)
            self.assertTrue(block.stop.called)
            self.assertTrue((Path(scratch.path)/'block-00000000-verification.json').exists())


if __name__=='__main__':unittest.main()
