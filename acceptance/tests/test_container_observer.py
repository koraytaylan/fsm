"""Labelled faulty consumer reports and filesystem stubs, never native proof."""
import copy
from contextlib import ExitStack
import json
from pathlib import Path
import stat
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from acceptance.suite import container, run
from acceptance.suite.evidence import Evidence, digest
from acceptance.suite.fsm import Scratch


class ContainerReportTests(unittest.TestCase):
    def report(self, only=None):
        names = [name for name, _ in run.discover(only)]
        return dict(verdict='passed', filter=only, release_eligible=only is None,
            required_scenarios=[name for name, _ in run.discover(None)], selected_scenarios=names,
            scenarios=[dict(name=name, verdict='passed', assertions=[dict(passed=True)]) for name in names],
            candidate=dict(source_commit='a'*40, binary_sha256='b'*64,
                identity_matches=True, build_provenance_verified=True))

    def test_full_and_filtered_reports_have_distinct_exact_inventories(self):
        for only in (None, 'executor'):
            report = self.report(only)
            container.validate_report(report, report['selected_scenarios'], 'a'*40, 'b'*64, only)
        report = self.report('executor')
        with self.assertRaises(ValueError):
            container.validate_report(report, self.report()['selected_scenarios'], 'a'*40, 'b'*64, None)

    def test_missing_duplicate_stale_unverified_and_failed_rows_refuse(self):
        changes = [lambda r: r['scenarios'].pop(), lambda r: r['scenarios'].append(r['scenarios'][0]),
            lambda r: r['candidate'].update(source_commit='c'*40),
            lambda r: r['candidate'].update(binary_sha256='c'*64),
            lambda r: r['candidate'].update(build_provenance_verified=False),
            lambda r: r['scenarios'][0].update(verdict='skipped'),
            lambda r: r['scenarios'][0].update(assertions=[]),
            lambda r: r['scenarios'][0]['assertions'][0].update(passed=1)]
        for change in changes:
            report = copy.deepcopy(self.report()); change(report)
            with self.assertRaises(ValueError):
                container.validate_report(report, self.report()['selected_scenarios'], 'a'*40, 'b'*64, None)


class ConsumerRetentionTests(unittest.TestCase):
    def invoke(self, fail_copy=False, swap='0'):
        with Scratch('labelled-container-producer-stub') as scratch, ExitStack() as patches:
            root = Path(scratch.path); cache=root/'cache';cache.mkdir()
            evidence=root/'evidence'; evidence.mkdir()
            binary=root/'candidate'; binary.write_bytes(b'labelled CLI stub')
            helper=root/'helper'; helper.write_bytes(b'labelled helper stub')
            receipt=root/'receipt.json'
            receipt.write_text(json.dumps(dict(source=dict(source_commit='a'*40,dirty=False),binary_sha256=digest(binary))))
            cgroup=root/'cgroup';cgroup.mkdir()
            (cgroup/'memory.max').write_text('1073741824');(cgroup/'memory.swap.max').write_text(swap)
            names=[name for name,_ in run.discover('executor')]
            def consumer(*_args, **_kwargs):
                identity=dict(source_commit='a'*40, dirty=False, binary_sha256=digest(binary),
                    identity_matches=True, build_provenance_verified=True, errors=[])
                report=Evidence(evidence/'reports',identity,[name for name,_ in run.discover(None)],names,'executor')
                for name in names: report.record(name,[(True,'labelled report control')],None,None,0.1)
                diagnostic=report.directory/'diagnostics.txt'
                diagnostic.write_text('Labelled report stub; no native execution.\n')
                report.report['artifacts'].append(dict(path=diagnostic.name,sha256=digest(diagnostic)))
                report.finish()
                for index in range(85):
                    fixture=cache/f'installed-native-{index}';fixture.mkdir()
                    (fixture/'retirement.json').write_text('{"cleaned":true}')
                return SimpleNamespace(returncode=0)
            patches.enter_context(patch.dict('os.environ',dict(FSM_ACCEPTANCE_DISPOSABLE_NATIVE='1',
                FSM_EVIDENCE_DIR=str(evidence),FSM_BUILD_RECEIPT=str(receipt))))
            patches.enter_context(patch.object(sys,'argv',['labelled-stub','executor']))
            metadata=SimpleNamespace(st_mode=stat.S_IFREG|0o711,st_uid=0,st_dev=1,st_ino=2)
            patches.enter_context(patch.object(container,'AUTHORITY',SimpleNamespace(lstat=lambda:metadata)))
            for name,value in dict(FSM=str(binary),REPO=str(root),BUILT_AUTHORITY=helper,CGROUP=cgroup).items():
                patches.enter_context(patch.object(container,name,value))
            patches.enter_context(patch.object(container,'require_disposable_runner'))
            patches.enter_context(patch.object(container,'verify_source'))
            patches.enter_context(patch.object(container,'task_cache',return_value=str(cache)))
            patches.enter_context(patch.object(container,'privileged',return_value=digest(helper)+' helper'))
            invoked=patches.enter_context(patch.object(container.subprocess,'run',side_effect=consumer))
            if fail_copy:
                patches.enter_context(patch.object(container.shutil,'copytree',side_effect=OSError('original fixture copy failed')))
            result=container.main();control=json.loads((evidence/'producer.json').read_text())
            return result,control,invoked.call_count

    def test_original_artifact_copy_failure_overrides_a_passing_stub_report(self):
        result,control,invoked=self.invoke(fail_copy=True)
        self.assertEqual(invoked,1);self.assertEqual(result,1);self.assertFalse(control['passed'])
        self.assertIn('original fixture copy failed',control['error'])

    def test_enforced_zero_swap_is_required_before_the_stub_consumer(self):
        result,control,invoked=self.invoke(swap='1')
        self.assertEqual(result,1);self.assertFalse(control['passed']);self.assertEqual(invoked,0)

    def test_valid_stub_receipt_and_original_copy_can_finish_the_producer(self):
        result,control,invoked=self.invoke()
        self.assertEqual(result,0);self.assertTrue(control['passed']);self.assertEqual(invoked,1)
        self.assertEqual(control['cells'],85);self.assertFalse(control['complete_matrix'])
