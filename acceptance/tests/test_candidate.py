"""Synthetic candidate refusal controls; no native execution or human proof."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import zipfile

from acceptance.suite.evidence import Evidence, digest
from acceptance.suite.fsm import Scratch
from acceptance.suite.run import discover

SPEC = importlib.util.spec_from_file_location('candidate_auxiliary_controls', Path(__file__).resolve().parents[1] / 'verify-candidate.py')
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
CANDIDATE = 'a' * 40
REVIEWER = 'independent-human-fixture'
PROTOCOL = b'Synthetic protocol bytes; no live model session was executed.'
CONTRACT = b'Synthetic contract fixture; no candidate completion evidence.'


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, sort_keys=True))


class SyntheticGitHub:
    def __init__(self, bundles):
        self.bundles, self.comment = bundles, None
        self.gates, self.release_gates = [], []

    def artifact(self, reference, *_args):
        return self.bundles[reference['artifact_id']]

    def gate(self, identifier, candidate):
        self.gates.append((identifier, candidate))

    def approval(self, _identifier):
        return self.comment

    def release_gate(self, identifier, candidate):
        self.release_gates.append((identifier, candidate))


class CandidateControls(unittest.TestCase):
    def fixture(self, root):
        index = dict(schema='fsm.candidate-index/1', candidate=CANDIDATE,
            contracts={name: hashlib.sha256(CONTRACT).hexdigest() for name in VERIFY.CONTRACTS},
            binaries={}, native_axes=[], consumer=None, manual=None, repository_gate=dict(run_id=700),
            operational_review=None, approval_comment_id=900)
        bundles = {}
        for identifier, axis in enumerate(sorted(VERIFY.NATIVE_AXES), 1):
            directory = root / str(identifier); directory.mkdir()
            key = '/'.join(axis); binary = hashlib.sha256(key.encode()).hexdigest()
            index['binaries'][key] = binary
            record = dict(axis=axis[0], platform=axis[1], requested_toolchain=axis[2], identity=dict(binary_sha256=binary))
            write(directory / 'native.json', record)
            reference = dict(axis=axis[0], platform=axis[1], toolchain=axis[2], artifact_id=identifier,
                job_id=identifier, run_id=identifier, archive_sha256='b'*64,
                record_path='native.json', record_sha256=digest(directory/'native.json'))
            if axis[0] == 'sustained':
                write(directory / 'lifecycle/native.json', dict(axis='executor', platform='linux', requested_toolchain='stable'))
                reference.update(lifecycle_record_path='lifecycle/native.json', lifecycle_record_sha256=digest(directory/'lifecycle/native.json'))
            index['native_axes'].append(reference);bundles[identifier] = directory
        consumer = root / 'consumer'; consumer.mkdir(); bundles[100] = consumer
        binary = consumer / 'fsm-installed-binary'; binary.write_bytes(b'Synthetic CLI fixture, never executed.')
        helper = consumer / 'fsm-containment-authority-built'; helper.write_bytes(b'Synthetic helper fixture, never executed.')
        index['binaries']['consumer/linux/1.89.0'] = digest(binary)
        source = dict(source_commit=CANDIDATE,dirty=False,files={})
        receipt = dict(source=source, binary_sha256=digest(binary), os='Linux', toolchain='rustc 1.89.0 synthetic',
            recipe=['cargo','install','--path','crates/fsm-cli','--locked','--root','/synthetic'])
        write(consumer/'build-receipt.json',receipt)
        names = [name for name, _ in discover(None)]
        identity = dict(source_commit=CANDIDATE,dirty=False,binary_sha256=digest(binary),identity_matches=True,
            build_provenance_verified=True,build_receipt_sha256=digest(consumer/'build-receipt.json'))
        evidence = Evidence(consumer/'reports',identity,names,names,None)
        for name in names:
            evidence.record(name, [(True,'Synthetic auxiliary assertion; not execution proof.')], None, None, 0.1)
        diagnostic = evidence.directory/'diagnostics.txt'; diagnostic.write_text('Synthetic auxiliary fixture.')
        evidence.report['artifacts'].append(dict(path=diagnostic.name,sha256=digest(diagnostic))); evidence.finish()
        report_path = evidence.directory/'report.json'
        producer = dict(schema='fsm.podman-installed-check/1',passed=True,scenario='full',complete_matrix=True,
            native_handler_execution=True,source_commit=CANDIDATE,consumer_exit=0,authority_removed=True,cells=149,
            operator_uid=1000,memory_max=1073741824,memory_swap_max=0,build_receipt_sha256=digest(consumer/'build-receipt.json'),
            report_sha256=digest(report_path),binary_sha256=digest(binary),authority_sha256=digest(helper))
        write(consumer/'producer.json',producer)
        for number in range(149): write(consumer/f'installed-native-{number}/retirement.json',dict(cleaned=True))
        write(consumer/'container-inspect.json',[dict(Id='original-synthetic-container',HostConfig=dict(
            Memory=1073741824,MemorySwap=1073741824,PidMode='private',CgroupMode='private'))])
        write(consumer/'container-retirement.json',dict(container_id='original-synthetic-container',removed=True,exit_code=0))
        index['consumer'] = dict(artifact_id=100,job_id=100,run_id=100,archive_sha256='b'*64,
            report_path=report_path.relative_to(consumer).as_posix(),report_sha256=digest(report_path))
        manual = root/'manual';manual.mkdir();bundles[200]=manual
        material = manual/'synthetic-observations.txt';material.write_bytes(PROTOCOL)
        def references(record,names):
            for name in names: record.update({name+'_reference':material.name,name+'_sha256':digest(material)})
        report = json.loads((Path(__file__).resolve().parents[1]/'manual/report-template.json').read_text())
        report.update(verdict='passed',executed=True,campaign_id='synthetic-campaign',incomplete_reasons=[])
        report['candidate'].update(code_sha=CANDIDATE,source_fixture_revision=CANDIDATE,binary_sha256=digest(binary),
            build_receipt=dict(reference='receipt.json',sha256=digest(consumer/'build-receipt.json')))
        write(manual/'receipt.json',receipt)
        report['protocol'].update(committed_revision=CANDIDATE,briefs_sha256=hashlib.sha256(PROTOCOL).hexdigest(),rubric_sha256=hashlib.sha256(PROTOCOL).hexdigest())
        report['model']=dict(name='synthetic fixture model',exact_version='synthetic')
        report['host']=dict(name='synthetic fixture host',version='synthetic',os='Linux',transport='stdio')
        report['operator']['identity']='synthetic fixture operator';references(report['operator'],('setup',))
        for identifier in sorted(VERIFY.SESSIONS):
            row=dict(attempt_id=identifier,campaign_id=report['campaign_id'],required_session_id=identifier,
                brief=identifier.rsplit('-',1)[0],candidate_code_sha=CANDIDATE,fixture_revision=CANDIDATE,
                binary_sha256=digest(binary),model=report['model'],host=report['host'],brief_sha256=report['protocol']['briefs_sha256'],
                rubric_sha256=report['protocol']['rubric_sha256'],verdict='passed',coached=False,aborted=False,safety_failure=False,
                cleanup_verified=True,human_reviewer_identity=REVIEWER,human_reviewed_at='2026-10-11T00:11:00+00:00',
                started_at='2026-10-11T00:00:00+00:00',ended_at='2026-10-11T00:10:00+00:00',
                model_call_count=20,elapsed_seconds=600,criterion_results={name:True for name in VERIFY.CRITERIA})
            references(row,('table','starting_store','starting_resource','original_transcript','public_redacted_transcript',
                'final_answer','original_journal','external_observations','cleanup','native_closure'))
            report['attempts'].append(row)
        report['desktop'].update(verdict='passed',version='synthetic',candidate_code_sha=CANDIDATE,binary_sha256=digest(binary),
            human_reviewer_identity=REVIEWER,human_reviewed_at='synthetic review timestamp')
        references(report['desktop'],('configuration','connect_initialize_list','capture'))
        report['human_review'].update(verdict='passed',reviewer_identity=REVIEWER,reviewed_at='synthetic review timestamp')
        references(report['human_review'],('conclusion',));write(manual/'report.json',report)
        index['manual']=dict(artifact_id=200,job_id=200,run_id=200,archive_sha256='b'*64,report_path='report.json',report_sha256=digest(manual/'report.json'))
        review=dict(candidate=CANDIDATE,reviewer_identity=REVIEWER,independent_of_implementing_author=True,
            verdict='passed',unresolved_findings=[],reviewed_at='synthetic review timestamp',concerns=[],
            clean_setup_executed=True,interrupted_recovery_executed=True)
        for name in VERIFY.CONCERNS:
            row=dict(name=name,verdict='passed',executed=True,conclusion='Synthetic auxiliary judgment, not independent review.')
            references(row,('evidence',));review['concerns'].append(row)
        references(review,('clean_setup_runbook','interrupted_recovery_runbook','conclusion'));index['operational_review']=review
        github=SyntheticGitHub(bundles)
        github.comment=dict(user=dict(login=REVIEWER,type='User'),commit_id=CANDIDATE,body=json.dumps(dict(
            schema='fsm.candidate-approval/1',candidate=CANDIDATE,evidence_sha256=VERIFY.approval_subject(index),
            verdict='passed',unresolved_findings=[])))
        return index,github

    def validate(self,index,github,reviewers=None,**kwargs):
        def original_blob(arguments):
            return PROTOCOL if arguments[1].split(':',1)[1].startswith('acceptance/manual/') else CONTRACT
        with patch.object(VERIFY,'candidate_checkout'),patch.object(VERIFY,'git',side_effect=original_blob), \
             patch.object(VERIFY.NATIVE,'verify_original_sources'), \
             patch.object(VERIFY.NATIVE,'verify_axis',side_effect=lambda root,*_:VERIFY.read_json(root/'native.json')):
            return VERIFY.validate_index(index,github,{REVIEWER} if reviewers is None else reviewers,**kwargs)

    def test_complete_synthetic_bundle_exercises_pre_tag_and_publication_paths(self):
        with Scratch('candidate-synthetic-complete-control') as scratch:
            index,github=self.fixture(Path(scratch.path));result=self.validate(index,github)
            self.assertTrue(result['candidate_complete']);self.assertFalse(result['publication_ready'])
            result=self.validate(index,github,release_run=800)
            self.assertTrue(result['publication_ready']);self.assertEqual(github.release_gates,[(800,CANDIDATE)])

    def test_each_top_field_axis_binary_and_contract_is_required(self):
        with Scratch('candidate-synthetic-missing-proof-controls') as scratch:
            index,github=self.fixture(Path(scratch.path))
            for key in index:
                broken=copy.deepcopy(index);del broken[key]
                with self.subTest(field=key),self.assertRaises(ValueError):self.validate(broken,github)
            for collection in ('native_axes','binaries','contracts'):
                for key in range(len(index[collection])) if isinstance(index[collection],list) else index[collection]:
                    broken=copy.deepcopy(index)
                    if isinstance(broken[collection],list):broken[collection].pop(key)
                    else:del broken[collection][key]
                    with self.subTest(collection=collection,key=key),self.assertRaises(ValueError):self.validate(broken,github)

    def test_substituted_binary_report_or_contract_and_duplicate_axes_refuse(self):
        with Scratch('candidate-synthetic-substitution-controls') as scratch:
            index,github=self.fixture(Path(scratch.path))
            changes=[lambda value:value['binaries'].update({'baseline/linux/stable':'c'*64}),
                lambda value:value['consumer'].update(report_sha256='c'*64),lambda value:value['native_axes'][0].update(record_sha256='c'*64),
                lambda value:value['contracts'].update({'docs/SPEC.md':'c'*64}),lambda value:value['native_axes'].__setitem__(1,value['native_axes'][0])]
            for change in changes:
                broken=copy.deepcopy(index);change(broken)
                with self.assertRaises(ValueError):self.validate(broken,github)
            (github.bundles[100]/'fsm-installed-binary').write_bytes(b'Substituted executable.')
            with self.assertRaisesRegex(ValueError,'executable differs'):self.validate(index,github)

    def test_approval_must_bind_original_index_and_independently_trusted_identity(self):
        with Scratch('candidate-synthetic-human-approval-controls') as scratch:
            index,github=self.fixture(Path(scratch.path))
            for reviewers in (set(),{'someone-else'}):
                with self.assertRaisesRegex(ValueError,'not trusted'):self.validate(index,github,reviewers)
            original=copy.deepcopy(github.comment)
            for key,value in [('commit_id','c'*40),('body','{}'),('user',dict(login=REVIEWER,type='Bot'))]:
                github.comment={**original,key:value}
                with self.assertRaises(ValueError):self.validate(index,github)

    def test_manual_missing_sessions_coaching_limits_skips_and_stale_identity_refuse(self):
        with Scratch('candidate-synthetic-live-session-controls') as scratch:
            index,github=self.fixture(Path(scratch.path));root=github.bundles[200];original=VERIFY.read_json(root/'report.json')
            changes=[lambda value:value['attempts'].pop(),lambda value:value['attempts'][0].update(coached=True),
                lambda value:value['attempts'][0].update(model_call_count=41),lambda value:value['attempts'][0].update(model_call_count=True),
                lambda value:value['attempts'][0].update(elapsed_seconds=901),lambda value:value['attempts'][0]['criterion_results'].update(safety=False),
                lambda value:value['attempts'][0].update(verdict='skipped'),lambda value:value['candidate'].update(code_sha='c'*40),
                lambda value:value['desktop'].update(verdict='unexecuted'),lambda value:value['human_review'].update(unresolved_findings=['unresolved'])]
            for change in changes:
                report=copy.deepcopy(original);change(report);write(root/'report.json',report)
                reference={**index['manual'],'report_sha256':digest(root/'report.json')}
                with self.assertRaises(ValueError),patch.object(VERIFY,'git',return_value=PROTOCOL),patch.object(VERIFY.NATIVE,'verify_original_sources'):
                    VERIFY.manual_report(root,reference,CANDIDATE,index['binaries'],REVIEWER)

    def test_consumer_required_scenario_and_zero_assertion_refuse(self):
        with Scratch('candidate-synthetic-consumer-inventory-controls') as scratch:
            index,github=self.fixture(Path(scratch.path));root=github.bundles[100]
            path=root/index['consumer']['report_path'];original=VERIFY.read_json(path)
            for change in (lambda value:value['scenarios'].pop(),lambda value:value['scenarios'][0].update(assertions=[]),
                           lambda value:value['scenarios'][0].update(verdict='skipped')):
                report=copy.deepcopy(original);change(report);write(path,report)
                reference={**index['consumer'],'report_sha256':digest(path)}
                with self.assertRaises(ValueError):VERIFY.validate_consumer(root,reference,CANDIDATE,index['binaries']['consumer/linux/1.89.0'])

    def test_evidence_only_commit_cannot_change_code_or_acceptance_protocol(self):
        for name in ('crates/fsm-cli/src/main.rs','acceptance/manual/briefs.md','docs/SPEC.md'):
            def response(arguments):
                if arguments[:2]==['rev-parse','HEAD']:return CANDIDATE.encode()
                if arguments[0]=='diff':return name.encode()+b'\0'
                return b''
            with patch.object(VERIFY,'git',side_effect=response),patch.object(VERIFY.NATIVE,'verify_original_sources'),self.assertRaisesRegex(ValueError,'implementation or protocol'):
                VERIFY.candidate_checkout(CANDIDATE,'b'*40)
        with patch.object(VERIFY,'git',return_value=b'dirty'),self.assertRaisesRegex(ValueError,'checkout differs'):
            VERIFY.candidate_checkout(CANDIDATE)
        with patch.object(VERIFY,'git',side_effect=[CANDIDATE.encode(),b' M implementation.py']),self.assertRaisesRegex(ValueError,'dirty'):
            VERIFY.candidate_checkout(CANDIDATE)
        with patch.object(VERIFY,'git',side_effect=[CANDIDATE.encode(),b'']), \
             patch.object(VERIFY.NATIVE,'verify_original_sources',side_effect=ValueError('actual source bytes differ')), \
             self.assertRaisesRegex(ValueError,'actual source bytes differ'):
            VERIFY.candidate_checkout(CANDIDATE)
        def evidence_only(arguments):
            if arguments[:2]==['rev-parse','HEAD']:return ('b'*40).encode()
            if arguments[0]=='diff':return b'docs/plans/STATUS.md\0acceptance/candidate-index.json\0'
            return b''
        with patch.object(VERIFY,'git',side_effect=evidence_only),patch.object(VERIFY.NATIVE,'verify_original_sources'):
            VERIFY.candidate_checkout(CANDIDATE,'b'*40)

    def test_archive_traversal_links_duplicates_and_original_digest_corruption_refuse(self):
        with Scratch('candidate-synthetic-archive-controls') as scratch:
            root=Path(scratch.path)
            for number,name in enumerate(('../escape','/absolute','a/../escape','back\\slash')):
                archive=root/f'{number}.zip'
                with zipfile.ZipFile(archive,'w') as output:output.writestr(name,'synthetic')
                with self.assertRaises(ValueError):VERIFY.extract_archive(archive,root/f'extracted-{number}')
            archive=root/'link.zip'
            with zipfile.ZipFile(archive,'w') as output:
                member=zipfile.ZipInfo('linked');member.external_attr=0o120777<<16;output.writestr(member,'target')
            with self.assertRaises(ValueError):VERIFY.extract_archive(archive,root/'linked')
            material=root/'original';material.write_text('synthetic')
            with self.assertRaisesRegex(ValueError,'digest mismatch'):VERIFY.checked(root,'original','c'*64)

    def test_actual_ci_metadata_refuses_failed_stale_foreign_or_untrusted_producers(self):
        github=VERIFY.GitHubEvidence('owner/repository',Path('/unused'))
        original=dict(repository=dict(full_name='owner/repository'),head_sha=CANDIDATE,path='.github/workflows/operational-acceptance.yml')
        for key,value in [('head_sha','c'*40),('path','.github/workflows/anything.yml'),('repository',dict(full_name='foreign/repo'))]:
            with patch.object(github,'api',return_value={**original,key:value}),self.assertRaises(ValueError):github.run(1,CANDIDATE,'operational-acceptance.yml')
        reference=dict(run_id=1,job_id=2,artifact_id=3,archive_sha256='b'*64)
        for status in ('failure','cancelled','skipped',None):
            with patch.object(github,'run',return_value=dict(status='completed',conclusion=status)),self.assertRaises(ValueError):
                github.artifact(reference,CANDIDATE,'operational-acceptance.yml')

    def test_downloaded_archive_and_actual_job_and_artifact_metadata_are_all_bound(self):
        with Scratch('candidate-synthetic-api-archive-controls') as scratch:
            root=Path(scratch.path);archive=root/'fixture.zip'
            with zipfile.ZipFile(archive,'w') as output:output.writestr('observation.txt','Synthetic transport control; not real proof.')
            content=archive.read_bytes();expected=digest(archive)
            reference=dict(run_id=1,job_id=2,artifact_id=3,archive_sha256=expected)
            job=dict(run_id=1,status='completed',conclusion='success',name='expected-job')
            metadata=dict(workflow_run=dict(id=1,head_sha=CANDIDATE),expired=False,digest='sha256:'+expected)
            for number,change in enumerate([None,lambda job,metadata:job.update(run_id=9),lambda job,metadata:job.update(name='other-job'),
                lambda job,metadata:job.update(conclusion='cancelled'),lambda job,metadata:job.update(status='in_progress'),
                lambda job,metadata:metadata.update(expired=True),lambda job,metadata:metadata.update(digest='sha256:'+'c'*64),
                lambda job,metadata:metadata['workflow_run'].update(id=9),lambda job,metadata:metadata['workflow_run'].update(head_sha='c'*40)]):
                cache=root/f'cache-{number}';cache.mkdir();github=VERIFY.GitHubEvidence('owner/repository',cache)
                current_job,current_metadata=copy.deepcopy(job),copy.deepcopy(metadata)
                if change:change(current_job,current_metadata)
                with patch.object(github,'run',return_value=dict(status='completed',conclusion='success')), \
                     patch.object(github,'api',side_effect=[current_job,current_metadata]), \
                     patch.object(VERIFY.subprocess,'run',side_effect=lambda *_args,stdout,**_kwargs:stdout.write(content)) as download:
                    if change:
                        with self.assertRaises(ValueError):github.artifact(reference,CANDIDATE,'operational-acceptance.yml','expected-job')
                        download.assert_not_called()
                    else:
                        destination=github.artifact(reference,CANDIDATE,'operational-acceptance.yml','expected-job')
                        self.assertEqual((destination/'observation.txt').read_text(),'Synthetic transport control; not real proof.')
            cache=root/'corrupt-cache';cache.mkdir();github=VERIFY.GitHubEvidence('owner/repository',cache)
            with patch.object(github,'run',return_value=dict(status='completed',conclusion='success')), \
                 patch.object(github,'api',side_effect=[job,metadata]), \
                 patch.object(VERIFY.subprocess,'run',side_effect=lambda *_args,stdout,**_kwargs:stdout.write(b'Corrupt original archive.')):
                with self.assertRaisesRegex(ValueError,'archive digest differs'):github.artifact(reference,CANDIDATE,'operational-acceptance.yml','expected-job')

    def test_each_required_manual_reference_identity_and_criterion_refuses_removal(self):
        with Scratch('candidate-synthetic-manual-field-controls') as scratch:
            index,github=self.fixture(Path(scratch.path));root=github.bundles[200];original=VERIFY.read_json(root/'report.json')
            paths=[('candidate',key) for key in ('code_sha','source_fixture_revision','binary_sha256','build_receipt')]
            paths += [('protocol',key) for key in original['protocol']]
            paths += [('operator',key) for key in original['operator']]
            paths += [('desktop',key) for key in original['desktop']]
            paths += [('human_review',key) for key in original['human_review']]
            paths += [('attempts',0,key) for key in original['attempts'][0] if not key.startswith('native_closure_')]
            paths += [('attempts',3,key) for key in ('native_closure_reference','native_closure_sha256')]
            paths += [('attempts',0,'criterion_results',key) for key in VERIFY.CRITERIA]
            for parts in paths:
                report=copy.deepcopy(original);parent=report
                for part in parts[:-1]:parent=parent[part]
                del parent[parts[-1]];write(root/'report.json',report)
                reference={**index['manual'],'report_sha256':digest(root/'report.json')}
                with self.subTest(parts=parts),patch.object(VERIFY,'git',return_value=PROTOCOL), \
                     patch.object(VERIFY.NATIVE,'verify_original_sources'),self.assertRaises((ValueError,KeyError)):
                    VERIFY.manual_report(root,reference,CANDIDATE,index['binaries'],REVIEWER)

    def test_required_repository_and_release_jobs_must_really_pass(self):
        github=VERIFY.GitHubEvidence('owner/repository',Path('/unused'))
        repository_jobs={f'gate ({os_name}, {toolchain})' for os_name in ('ubuntu-latest','macos-latest','windows-latest') for toolchain in ('stable','1.89')}
        repository_jobs |= {'zero-deps','native-containment (stable)','native-containment (1.89.0)'}
        release_jobs={f'verify ({os_name}, {toolchain})' for os_name in ('ubuntu-latest','macos-latest','windows-latest') for toolchain in ('stable','1.89')}
        release_jobs |= {'version','git-dep','fuzz-smoke'}
        for function,names in ((github.gate,repository_jobs),(github.release_gate,release_jobs)):
            jobs=[dict(name=name,status='completed',conclusion='success') for name in sorted(names)]
            run=dict(status='completed',conclusion='success',event='push')
            with patch.object(github,'run',return_value=run),patch.object(github,'api',return_value=dict(total_count=len(jobs),jobs=jobs)):
                function(1,CANDIDATE)
            for index in range(len(jobs)):
                for conclusion in ('skipped','failure','cancelled',None):
                    broken=copy.deepcopy(jobs);broken[index]['conclusion']=conclusion
                    with patch.object(github,'run',return_value=run),patch.object(github,'api',return_value=dict(total_count=len(broken),jobs=broken)),self.assertRaises(ValueError):function(1,CANDIDATE)
                missing=copy.deepcopy(jobs);missing.pop(index)
                with patch.object(github,'run',return_value=run),patch.object(github,'api',return_value=dict(total_count=len(missing),jobs=missing)),self.assertRaises(ValueError):function(1,CANDIDATE)

    def test_every_independent_review_record_and_concern_is_required_even_after_approval(self):
        with Scratch('candidate-synthetic-independent-review-controls') as scratch:
            index,github=self.fixture(Path(scratch.path))
            paths=[('operational_review',key) for key in index['operational_review']]
            paths += [('operational_review','concerns',0,key) for key in index['operational_review']['concerns'][0]]
            for parts in paths:
                broken=copy.deepcopy(index);parent=broken
                for part in parts[:-1]:parent=parent[part]
                del parent[parts[-1]]
                approval=json.loads(github.comment['body']);approval['evidence_sha256']=VERIFY.approval_subject(broken)
                github.comment['body']=json.dumps(approval)
                with self.subTest(parts=parts),self.assertRaises((ValueError,KeyError)):self.validate(broken,github)
            for number in range(5):
                broken=copy.deepcopy(index);broken['operational_review']['concerns'].pop(number)
                approval=json.loads(github.comment['body']);approval['evidence_sha256']=VERIFY.approval_subject(broken)
                github.comment['body']=json.dumps(approval)
                with self.assertRaisesRegex(ValueError,'five operational'):self.validate(broken,github)

    def test_duplicate_json_keys_and_missing_original_artifacts_refuse(self):
        with Scratch('candidate-synthetic-json-controls') as scratch:
            root=Path(scratch.path);path=root/'duplicate.json';path.write_text('{"candidate":"original","candidate":"substituted"}')
            with self.assertRaisesRegex(ValueError,'duplicate JSON key'):VERIFY.read_json(path)
            with self.assertRaisesRegex(ValueError,'missing original'):VERIFY.checked(root,'unretained.txt','b'*64)


if __name__=='__main__':unittest.main()
