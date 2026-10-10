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
from acceptance.suite.soak_resources import observe_process_resources
from acceptance.suite.soak import schedule
from acceptance.suite.soak_installed import run_block,InstalledBlock
from acceptance.suite.executor_control import OwnerUnavailable


def census():
    process=dict(host=dict(identity={'pid':123,'pid_starttime':'456'},
        metrics=dict(rss_bytes=100,file_descriptors=3,active_children=0,cpu_ns=20)),
        pipes={'pipes':[]},unix={'endpoints':[]})
    return dict(owner=process,processes=[],descendants=[],disk_bytes=7)


class AccountingTests(unittest.TestCase):
    def test_standalone_retries_only_typed_retryable_lock_with_the_original_request(self):
        from acceptance.suite.fsm import Result,CliError
        block=InstalledBlock(MagicMock(),Path('/owned-store'),MagicMock(),Path('/owned-table'),'standalone','process',0)
        arguments=dict(instance_id='inst-owned',event={'name':'start'},request_id='original-request')
        locked=Result(1,'',json.dumps(dict(code='store/lock',retryable=True)),['fsm'])
        success=Result(0,'{"accepted":true}','',['fsm'])
        with patch('acceptance.suite.soak_installed.fsm.run',side_effect=[locked,success]) as run, \
             patch('acceptance.suite.soak_installed.time.monotonic',return_value=0), \
             patch('acceptance.suite.soak_installed.time.sleep'):
            self.assertEqual(block.call('instance_send',arguments),{'accepted':True})
        self.assertEqual(run.call_args_list[0],run.call_args_list[1])
        self.assertIn('--request-id=original-request',run.call_args.args)
        for failure in (Result(1,'',json.dumps(dict(code='store/lock',retryable=False)),['fsm']),
                        Result(1,'',json.dumps(dict(code='req/instance_not_found',retryable=True)),['fsm']),
                        Result(1,'','unstructured failure',['fsm'])):
            with patch('acceptance.suite.soak_installed.fsm.run',return_value=failure) as run:
                with self.assertRaises(CliError):block.call('instance_send',arguments)
            run.assert_called_once()
        with patch('acceptance.suite.soak_installed.fsm.run',return_value=locked) as run, \
             patch('acceptance.suite.soak_installed.time.monotonic',side_effect=[0,3]):
            with self.assertRaises(CliError):block.call('instance_send',arguments)
        run.assert_called_once()

    def test_cancelled_projection_preserves_original_pending_effect_without_acknowledgement(self):
        report=MagicMock()
        report.equal.side_effect=lambda actual,expected,message:self.assertEqual(actual,expected,message)
        block=InstalledBlock(report,Path('/owned-store'),MagicMock(),Path('/owned-table'),'standalone','process',0)
        original=['inst-cancel/2/0']
        projection=dict(status='cancelled',leaf='validating',effects_pending=list(original))
        with patch.object(block,'call',return_value=projection):
            self.assertEqual(block.verify_terminal('inst-cancel','cancel',original),projection)
        for pending in ([],['inst-cancel/3/0'],original+original):
            with patch.object(block,'call',return_value={**projection,'effects_pending':pending}):
                with self.assertRaises(AssertionError):block.verify_terminal('inst-cancel','cancel',original)
        with patch.object(block,'call',return_value={**projection,'effects_pending':[]}):
            block.verify_terminal('inst-manual','manual',['inst-manual/2/0'])

    def test_original_process_sample_retries_transient_files_without_zero_filling(self):
        identity=dict(pid=123,pid_starttime='456')
        missing=FileNotFoundError(2,'gone','/proc/123/fd/5')
        with patch('acceptance.suite.soak_resources.observe_live_host',return_value={'original':True}) as host, \
             patch('acceptance.suite.soak_resources.observe_pipe_queues',return_value={'queued_pipe_bytes':7}), \
             patch('acceptance.suite.soak_socket_queues.observe_unix_queues',side_effect=[missing,{'queued_unix_bytes':9}]), \
             patch('acceptance.suite.soak_resources.time.sleep'):
            value=observe_process_resources(identity)
        self.assertEqual(value,dict(host={'original':True},pipes={'queued_pipe_bytes':7},
            unix={'queued_unix_bytes':9},sample_attempts=2))
        self.assertEqual(host.call_args_list,[unittest.mock.call(identity)]*2)

    def test_original_process_sample_refuses_death_changed_birth_and_persistent_gaps(self):
        identity=dict(pid=123,pid_starttime='456')
        failures=[FileNotFoundError(2,'dead','/proc/123/stat'),
                  FileNotFoundError(2,'foreign','/proc/124/fd/5'),
                  ValueError('resource observation lost its original live host identity')]
        for failure in failures:
            with patch('acceptance.suite.soak_resources.observe_live_host',side_effect=failure) as host:
                with self.assertRaises(type(failure)):observe_process_resources(identity)
            host.assert_called_once_with(identity)
        with patch('acceptance.suite.soak_resources.observe_live_host',side_effect=
                   FileNotFoundError(2,'gone','/proc/123/task/124/children')) as host, \
             patch('acceptance.suite.soak_resources.time.monotonic',side_effect=[0,1,2]), \
             patch('acceptance.suite.soak_resources.time.sleep'):
            with self.assertRaises(FileNotFoundError):observe_process_resources(identity)
        self.assertEqual(host.call_count,2)

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

    def test_startup_waits_for_original_publication_but_never_accepts_ambiguous_ownership(self):
        block=InstalledBlock(MagicMock(),Path('/owned-store'),MagicMock(),Path('/owned-table'),'standalone','process',0)
        context=MagicMock();host=MagicMock();host.poll.return_value=None
        context.__enter__.return_value=(None,host)
        def run_wait(predicate,process,seconds):
            self.assertFalse(predicate());self.assertFalse(predicate());self.assertTrue(predicate())
        with patch('acceptance.suite.soak_installed._restart_host',return_value=context), \
             patch('acceptance.suite.soak_installed.birth',return_value={'pid':123,'pid_starttime':'456'}), \
             patch('acceptance.suite.soak_installed.observe_owner',side_effect=[FileNotFoundError(),OwnerUnavailable(),{'phase':'running'}]), \
             patch('acceptance.suite.soak_installed._wait_for_files',side_effect=run_wait), \
             patch.object(block,'quiescent',return_value=census()):
            block.start()
        self.assertEqual(block.warmed,census())
        with patch('acceptance.suite.soak_installed._restart_host',return_value=context), \
             patch('acceptance.suite.soak_installed.birth',return_value={'pid':123,'pid_starttime':'456'}), \
             patch('acceptance.suite.soak_installed.observe_owner',side_effect=ValueError('ambiguous owner')), \
             patch('acceptance.suite.soak_installed._wait_for_files',side_effect=lambda predicate,*args:predicate()):
            with self.assertRaisesRegex(ValueError,'ambiguous'):block.start()

    def test_noise_observation_binds_the_closed_physical_run_and_original_birth(self):
        report=MagicMock()
        def equal(actual,expected,message):self.assertEqual(actual,expected,message)
        report.equal.side_effect=equal
        block=InstalledBlock(report,Path('/owned-store'),MagicMock(),Path('/owned-table'),'standalone','process',0)
        original=dict(run='original-validation',operation='validate',pid=123,pid_starttime='456')
        physical=dict(run=original['run'],pid=123,pid_starttime='456',bytes=131072,descriptor=2)
        import hashlib
        records=[dict(kind='execution_stopped',body=dict(instance_id='inst-noise',outcome=dict(result={
            'stderr':'n'*4096,'stderr_sha256':hashlib.sha256(b'n'*131072).hexdigest()})))]
        with patch('acceptance.suite.soak_installed._fixture_rows',return_value=[physical]):
            block._noise(records,'inst-noise',dict(entries=[original]))
        for key,value in (('run','foreign'),('pid_starttime','789'),('descriptor',1)):
            changed={**physical,key:value}
            with patch('acceptance.suite.soak_installed._fixture_rows',return_value=[changed]):
                with self.assertRaises(AssertionError):block._noise(records,'inst-noise',dict(entries=[original]))


if __name__=='__main__':unittest.main()
