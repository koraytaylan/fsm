"""Repeated real installed work in one host/store across each seeded block.

Each case waits on independently recorded fixture/journal progress; a read,
poll or control request cannot be used to repair an expected engine advance.
The caller owns duration/profile completion, frozen calibration and reporting.
"""
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import time

from . import fsm
from .executor_control import observe_owner, OwnerUnavailable
from .executor_lifecycle import _restart_host, process_observation
from .executor_scenarios import _fixture_rows, _wait_for_files, _retire_execution_owner, read_journal_prefix
from .mcp import StdioClient
from .native_fixture import DisposableAuthority, privileged, require_disposable_runner
from .soak_fixtures import block_inputs, resource_names, completed_ledger
from .soak_journal import observe_cycle_journal
from .soak_native_metrics import stage_sampler, sample_native, queue_bytes
from .soak_workload import observe_completed_cycle


def birth(process):
    with Path('/proc',str(process.pid),'stat').open('rb') as stream: raw=stream.read(4097)
    fields=raw.rpartition(b') ')[2].split()
    if len(raw)>4096 or len(fields)<20 or not fields[19].isdigit():
        raise ValueError('installed host birth identity is unavailable')
    return dict(pid=process.pid,pid_starttime=fields[19].decode('ascii'))


class InstalledBlock:
    def __init__(self, report, store, native, table, transport, kind, block_number):
        self.report,self.store,self.native,self.table=report,store,native,table
        self.transport,self.kind,self.block_number=transport,kind,block_number
        self.stack=None;self.client=None;self.host=None;self.archives=[]

    def start(self):
        self.stack=ExitStack()
        self.client,self.host=self.stack.enter_context(_restart_host(self.store,self.table,self.transport))
        if self.client is not None:self.client.initialize()
        self.identity=birth(self.host)
        def ready():
            try:return observe_owner(self.store)['phase']=='running'
            except (FileNotFoundError,OwnerUnavailable):return False
        _wait_for_files(ready,self.host,10)
        self.warmed=self.quiescent()

    def stop(self):
        if self.stack is None:return
        try:
            if self.host.poll() is None:
                _retire_execution_owner(self.report,self.host,self.store,self.native.namespace,self.transport)
        finally:
            self.stack.close();self.stack=None

    def call(self,name,arguments):
        if self.client is not None:return self.client.structured(name,arguments)
        instance=arguments.get('instance_id')
        request=arguments.get('request_id')
        if name=='instance_create':
            command=['instance','new',arguments['machine'],f'--request-id={request}']
        elif name=='instance_send':
            command=['instance','send',instance,arguments['event']['name'],f'--request-id={request}']
        elif name=='instance_get':command=['instance','show',instance]
        elif name=='instance_cancel':command=['instance','cancel',instance,f"--reason={arguments['reason']}",f'--request-id={request}']
        elif name=='effect_ack':command=['instance','ack',instance,arguments['effect_id'],'--outcome=ok',f'--request-id={request}']
        else:raise ValueError('unsupported installed workload action')
        deadline=time.monotonic()+3
        while True:
            result=fsm.run(*command,'--json',data_dir=str(self.store),timeout=10)
            if result.code==0:return result.json()
            try:value=json.loads(result.text)
            except json.JSONDecodeError:result.ok()
            error=value.get('error',value) if isinstance(value,dict) else value
            if (not isinstance(error,dict) or error.get('code')!='store/lock'
                or error.get('retryable') is not True or time.monotonic()>=deadline):
                result.ok()
            time.sleep(0.01)

    def create(self,case,index,suffix=''):
        label=f'soak-{self.block_number}-{index}{suffix}'
        machine='soak_'+case.replace('-','_')
        return self.call('instance_create',dict(machine=machine,request_id=label+'-create'))['instance_id']

    def resource_observations(self,resource):
        rows={name:[row for row in _fixture_rows(self.native.resource/(name+'.jsonl'))
                    if row.get('resource')==resource or name=='results']
              for name in ('entries','results','timings','trace')}
        runs={row['run'] for row in rows['entries']}
        rows['results']=[row for row in rows['results'] if row['run'] in runs]
        path=self.native.resource/(hashlib.sha256(resource.encode()).hexdigest()+'.json')
        rows['state']=json.loads(path.read_text())
        return rows

    def census(self):
        value=sample_native(self.native,self.identity)
        descendants=_fixture_rows(self.native.resource/'descendants.jsonl')
        value['descendants']=[process_observation(row) for row in descendants]
        members={(row['host']['identity']['pid'],row['host']['identity']['pid_starttime'])
                 for row in value['processes']}
        if any(row['alive'] and (row['pid'],row['pid_starttime']) not in members
               for row in value['descendants']):
            raise ValueError('an original live descendant escaped the original domain census')
        return value

    def quiescent(self):
        deadline=time.monotonic()+10
        while True:
            value=self.census()
            if (not value['processes'] and queue_bytes(value)==0
                and not any(row['alive'] for row in value['descendants'])):return value
            if self.host.poll() is not None or time.monotonic()>=deadline:
                raise ValueError('original resources did not reach live-host quiescence')
            time.sleep(0.01)

    def run(self,entry):
        case,index=entry['case'],entry['index']
        instance=self.create(case,index)
        started=time.monotonic_ns()
        triggered=self.call('instance_send',dict(instance_id=instance,
            event={'name':'manual' if case=='manual' else 'start'},request_id=f'soak-{index}-start'))
        extra=None
        if case=='contention':
            extra=self.create('contention-right',index,'-right')
            self.call('instance_send',dict(instance_id=extra,event={'name':'start'},request_id=f'soak-{index}-right-start'))
        resource='soak-'+case
        if case=='manual':
            self.report.equal(triggered['leaf'],'manual_pause','manual work pauses without a native handler')
            self.report.equal(len(triggered['effects_pending']),1,'one manual effect remains available to the operator')
            dispatched=time.monotonic_ns()
            self.call('effect_ack',dict(instance_id=instance,effect_id=triggered['effects_pending'][0],
                outcome='ok',request_id=f'soak-{index}-manual-ack'))
            self.call('instance_cancel',dict(instance_id=instance,reason='completed manual fixture',request_id=f'soak-{index}-manual-cancel'))
            active=self.census()
        else:
            entered=_wait_for_files(lambda:self.resource_observations(resource)['timings'],self.host,10)
            dispatched=entered[0]['monotonic_ns']
            active=self.census()
        control_started=time.monotonic_ns()
        control=observe_owner(self.store)
        control_latency=time.monotonic_ns()-control_started
        self.report.equal(control['phase'],'running','control remains responsive during active fixture work')
        if case=='cancel':
            self.call('instance_cancel',dict(instance_id=instance,reason='active soak cancellation',request_id=f'soak-{index}-cancel'))
        elif case=='churn':
            with StdioClient([fsm.FSM,'serve',f'--data-dir={self.store}','--read-only']) as observer:
                observer.initialize();self.report.true(bool(observer.tools()),'a fresh read-only client attaches during native work')
            self.report.true(self.host.poll() is None,'client churn leaves the original execution owner alive')
        quiet_id=self.client._next_id if self.client is not None else None
        if case not in ('manual','cancel','deadline'):
            (self.native.resource/('release-'+resource)).write_text('release bounded real work')
            if extra:(self.native.resource/'release-soak-contention-right').write_text('release contending real work')
        instances=[instance]+([extra] if extra else [])
        _wait_for_files(lambda:all(not observe_cycle_journal(read_journal_prefix(self.store),item,
            'contention' if item==extra else case,self.kind) for item in instances),self.host,30)
        records=read_journal_prefix(self.store)
        completed=time.monotonic_ns()
        if self.client is not None:self.report.equal(self.client._next_id,quiet_id,'native settlements advance without a progress request')
        observations=self.resource_observations(resource)
        if case in ('cancel','deadline'):
            self._interrupted(case,observations,records,instance)
        elif case=='manual':
            self.report.equal(observations['entries'],[],'manual work never enters a native operation')
        else:
            observe_completed_cycle(completed_ledger(case),observations,started,completed)
            if extra:
                observe_completed_cycle(completed_ledger('contention','soak-contention-right'),
                    self.resource_observations('soak-contention-right'),started,completed)
        right=None;reopened=None
        final=self.verify_terminal(instance,case,triggered['effects_pending'])
        if extra:
            right=self.call('instance_get',dict(instance_id=extra))
            self.report.equal(right['status'],'completed','the second contending instance completes')
            self.report.equal(right['leaf'],'completed','the second contending instance reaches its authored leaf')
            self.report.equal(right['effects_pending'],[],'the second contending instance leaves no pending work')
        quiet=self.quiescent()
        if case=='noise':self._noise(records,instance,observations)
        before_restart=None
        if case in ('archive','reopen','replacement'):
            before_restart=self.identity
            self.stop()
            if case=='archive':self.archive(index)
            self.start()
            self.report.true(self.identity!=before_restart,'replacement has a fresh physical host birth')
            reopened=self.call('instance_get',dict(instance_id=instance))
            for key in ('instance_id','leaf','status','context','effects_pending'):
                self.report.equal(reopened.get(key),final.get(key),f'reopening preserves the completed instance {key}')
            quiet=self.quiescent()
        records=[row for row in records if row['body'].get('instance_id') in instances]
        return dict(schedule=entry,instance=instance,extra_instance=extra,journal=records,
            physical=observations,active=active,quiescent=quiet,warmed=self.warmed,
            started_ns=started,completed_ns=time.monotonic_ns(),physical_completed_ns=completed,
            scheduler_lag_ns=dispatched-started,control_latency_ns=control_latency,
            control=control,final=final,extra_final=right,reopened=reopened,
            original_host=before_restart,host=self.identity)

    def verify_terminal(self,instance,case,original_pending):
        final=self.call('instance_get',dict(instance_id=instance))
        self.report.equal(final['status'],'cancelled' if case in ('cancel','manual') else 'completed','the installed instance reaches its authored disposition')
        terminal='prerequisite_failed' if case=='deadline' else 'compensated' if case=='compensate' else 'completed'
        if case not in ('cancel','manual'):self.report.equal(final['leaf'],terminal,'the installed instance reaches its authored leaf')
        # SPEC execution settlement consumes interrupted ownership without an
        # acknowledgement; cancellation preserves the original pending effect.
        if case=='cancel':self.report.equal(len(original_pending),1,'cancellation retains one originally emitted effect')
        self.report.equal(final['effects_pending'],original_pending if case=='cancel' else [],
                          'the terminal projection preserves the authored pending-effect disposition')
        return final

    def _interrupted(self,case,observed,records,instance):
        self.report.equal(len(observed['entries']),1,'one original handler enters interrupted work')
        self.report.equal(observed['results'],[],'interruption retains an absent physical result')
        self.report.equal([row['kind'] for row in observed['trace']],['start'],'interruption never invents a fixture end or mutation')
        self.report.equal([row['phase'] for row in observed['timings']],['entered'],'interruption never invents a finish clock')
        self.report.true(process_observation(observed['entries'][0])['alive'] is False,'the exact original handler is dead after interruption')
        self.report.equal(observed['state'],dict(suspended=False,items=[]),'interrupted validation leaves its external resource untouched')
        claim=next(row for row in records if row['kind']=='execution_claimed' and row['body'].get('instance_id')==instance)
        allocation=claim['body']['domain']['allocation']
        closure=json.loads(privileged('cat',str(self.native.directory/f'closed-{allocation}.json')))
        self.report.equal(closure['domain'],claim['body']['domain'],'original closure binds the interrupted domain before completion')
        if case=='deadline':
            descendants=[row for row in _fixture_rows(self.native.resource/'descendants.jsonl') if row['resource']=='soak-deadline']
            self.report.equal(len(descendants),1,'deadline work owns one real retained-stream descendant')
            self.report.true(process_observation(descendants[0])['alive'] is False,'deadline closure retires the original descendant')

    def _noise(self,records,instance,observed):
        original=next(row for row in observed['entries'] if row['operation']=='validate')
        rows=[row for row in _fixture_rows(self.native.resource/'noise.jsonl') if row.get('run')==original['run']]
        self.report.equal(len(rows),1,'one original handler emits the bounded output flood')
        self.report.equal(set(rows[0]),{'run','bytes','descriptor','pid','pid_starttime'},'physical noise retains its original closed schema')
        for key in ('pid','pid_starttime'):
            self.report.equal(rows[0][key],original[key],'the physical flood binds its original handler birth')
        self.report.equal(rows[0]['descriptor'],2,'the original flood writes its declared stderr stream')
        self.report.equal(rows[0]['bytes'],131072,'the physical flood writes its entire authored byte count')
        stopped=next(row for row in records if row['kind']=='execution_stopped' and row['body'].get('instance_id')==instance)
        candidate=stopped['body']['outcome']['result']
        self.report.equal(candidate.get('stderr'),'n'*4096,'the candidate retains the exact bounded stderr prefix')
        self.report.equal(candidate.get('stderr_sha256'),hashlib.sha256(b'n'*131072).hexdigest(),'the candidate identifies the whole bounded flood')

    def archive(self,index):
        parent=self.store.parent/'archives';parent.mkdir(exist_ok=True)
        path=parent/f'{self.native.namespace}-{index}'
        path.mkdir()
        preview=fsm.run_json('journal','archive',f'--to={path}','--dry-run',data_dir=str(self.store))
        sealed=fsm.run_json('journal','archive',f'--to={path}',f"--before-seq={preview['sealed_through_seq']}",data_dir=str(self.store))
        self.report.equal(sealed['sealed_through_seq'],preview['sealed_through_seq'],'archive preserves its independently previewed boundary')
        verdict=fsm.run_json('journal','verify',f'--with-archive={path}',data_dir=str(self.store))
        self.report.equal(verdict['seal']['verdict'],'prefix_walked','the installed verifier reads the original archived prefix')
        self.report.equal(verdict['health'],'Ok','the installed verifier validates the archived chain')
        current=json.loads((path/'MANIFEST').read_text())
        prior=[json.loads((item/'MANIFEST').read_text()) for item in parent.iterdir() if item!=path]
        previous=max(prior,key=lambda row:row['sealed_through_seq']) if prior else None
        self.report.equal(current['first_prev_hash'],previous['sealed_last_hash'] if previous else '0'*64,
                          'each original archive continues its retained predecessor')
        self.archives.append(path)


def run_block(report,store,entries,consume):
    require_disposable_runner()
    if len(entries)!=12 or len({(row['transport'],row['handler_kind']) for row in entries})!=1:
        raise ValueError('one complete twelve-case host block is required')
    from .soak import CASES
    if {row['case'] for row in entries}!=set(CASES) or [row['index'] for row in entries]!=list(range(entries[0]['index'],entries[0]['index']+12)):
        raise ValueError('a host block must retain every case and its exact schedule indexes')
    fixture=Path(fsm.REPO)/'acceptance/fixtures/executor_handler.py'
    workflow=fixture.with_name('executor_workflow.json')
    kind,transport=entries[0]['handler_kind'],entries[0]['transport']
    with DisposableAuthority(fixture,resources=resource_names(),record_limit=1024,broker_seconds=300) as native:
        inputs=block_inputs(kind,native.resource,native.handler,workflow)
        for case,machine in inputs['machines'].items():
            path=native.cache/(case+'.json');path.write_text(json.dumps(machine))
            fsm.run('machine','add',str(path),data_dir=str(store)).ok()
        table=native.approve(store,inputs['table'])
        stage_sampler(native)
        block=InstalledBlock(report,store,native,table,transport,kind,entries[0]['index']//12)
        try:
            block.start()
            for entry in entries:
                observation=block.run(entry)
                if consume(observation) is False:break
            block.stop()
            archives=list((store.parent/'archives').glob('*/MANIFEST'))
            arguments=[]
            if archives:
                latest=max(archives,key=lambda path:json.loads(path.read_text())['sealed_through_seq'])
                arguments=[f'--with-archive={latest.parent}']
            verification=fsm.run_json('journal','verify',*arguments,data_dir=str(store))
            replay=fsm.run_json('journal','replay',data_dir=str(store))
            report.equal(verification['health'],'Ok','the installed verifier validates the completed host block')
            report.true(replay['agreement'] is True,'the installed replay reproduces the completed host block')
            (store.parent/f'block-{entries[0]["index"]//12:08}-verification.json').write_text(
                json.dumps(dict(verification=verification,replay=replay),sort_keys=True))
        except Exception as failure:
            (native.cache/'workload-failure.json').write_text(json.dumps(dict(error=str(failure)),sort_keys=True))
            raise
        finally:
            if block.host is not None:
                for name in ('stdout','stderr'):
                    value=getattr(block.host,'acceptance_'+name,None)
                    if value is not None:
                        (native.cache/('owner-'+name+'.log')).write_bytes(bytes(value)[-65536:])
            block.stop()
    report.true(native.cleaned,'every original domain closes before owned block fixture cleanup')
