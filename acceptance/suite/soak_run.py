"""Explicit installed operational calibration/smoke/sustained entry point.

Calibration observations are not a candidate resource pass; their numeric
budgets must be frozen in the committed named-host profile before validation.
Incomplete work, unavailable observations and failed cleanup never become a
pass, including when duration or count has already reached its floor.
"""
import argparse
import hashlib
from itertools import islice
import json
import os
from pathlib import Path
import platform
import stat

from . import fsm
from .evidence import candidate
from .metrics import calibration_for, validate_sample, METRICS
from .native_fixture import require_disposable_runner
from .run import Report
from .soak import RunState, schedule
from .soak_installed import run_block
from .soak_native_metrics import queue_bytes

WORKLOAD_FILES=('soak.py','soak_run.py','soak_installed.py','soak_fixtures.py','soak_journal.py',
                'soak_workload.py','soak_resources.py','soak_socket_queues.py','soak_native_metrics.py',
                'metrics.py','native_fixture.py','executor_scenarios.py','executor_lifecycle.py',
                'executor_control.py','fsm.py','mcp.py')


def workload_digest(repository,profiles,seed,profile='smoke'):
    inputs={}
    for name in WORKLOAD_FILES:
        path=Path(repository)/'acceptance/suite'/name
        inputs['suite/'+name]=hashlib.sha256(path.read_bytes()).hexdigest()
    for name in ('executor_handler.py','executor_workflow.json'):
        path=Path(repository)/'acceptance/fixtures'/name
        inputs['fixtures/'+name]=hashlib.sha256(path.read_bytes()).hexdigest()
    encoded=json.dumps(dict(files=inputs,profiles=profiles,profile=profile,seed=seed),sort_keys=True,separators=(',',':')).encode()
    return hashlib.sha256(encoded).hexdigest()


def disk_size(root):
    if not root.exists():raise ValueError('required disk observation is unavailable')
    size=0;count=0
    for directory,children,files in os.walk(root,followlinks=False):
        for name in [*children,*files]:
            count+=1
            if count>100000:raise ValueError('owned disk inventory exceeds its bound')
            metadata=(Path(directory)/name).lstat()
            if stat.S_ISLNK(metadata.st_mode):raise ValueError('owned disk inventory contains a symlink')
            if stat.S_ISREG(metadata.st_mode):size+=metadata.st_size
            elif not stat.S_ISDIR(metadata.st_mode):raise ValueError('owned disk inventory contains an unexpected stream')
            if size>16*1024**3:raise ValueError('owned disk inventory exceeds sixteen GiB')
    return size


def observed_sample(observation,phase,store,directory):
    census=observation[phase]
    owner=census['owner']['host']['metrics']
    processes=census['processes']
    descendants=sum(row['alive'] for row in census['descendants'])
    archives=store.parent/'archives'
    metrics=dict(work_latency_ns=observation['completed_ns']-observation['started_ns'],
        control_latency_ns=observation['control_latency_ns'],scheduler_lag_ns=observation['scheduler_lag_ns'],
        rss_bytes=owner['rss_bytes']+sum(row['host']['metrics']['rss_bytes'] for row in processes),
        file_descriptors=owner['file_descriptors']+sum(row['host']['metrics']['file_descriptors'] for row in processes),
        active_children=owner['active_children']+len(processes),surviving_descendants=descendants,
        capture_bytes=queue_bytes(census),journal_bytes=disk_size(store/'journal')+(disk_size(archives) if archives.exists() else 0),
        disk_bytes=disk_size(directory)+census['disk_bytes'],cpu_ns=owner['cpu_ns']+sum(row['host']['metrics']['cpu_ns'] for row in processes))
    if set(metrics)!=set(METRICS):raise ValueError('required operational metric inventory differs')
    return dict(phase='active' if phase=='active' else 'quiescent',metrics=metrics,
        equivalent_state=dict(live_hosts=1,handled_pending=0,handler_kind=observation['schedule']['handler_kind'],
                              transport=observation['schedule']['transport'],host=census['owner']['host']['identity']))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile',choices=('smoke','sustained'),required=True)
    parser.add_argument('--mode',choices=('calibrate','validate'),required=True)
    parser.add_argument('--seed',type=int,required=True)
    parser.add_argument('--host',required=True)
    parser.add_argument('--report-dir',type=Path,required=True)
    args=parser.parse_args()
    require_disposable_runner()
    root=args.report_dir.resolve()
    if root.exists() or root==Path('/tmp') or Path('/tmp') in root.parents:
        raise ValueError('operational report needs a fresh owned directory outside /tmp')
    root.mkdir(parents=True)
    identity=candidate(fsm.FSM,fsm.REPO,os.environ.get('FSM_CANDIDATE_REVISION'),
        os.environ.get('FSM_CANDIDATE_SHA256'),os.environ.get('FSM_BUILD_RECEIPT'))
    if not identity['identity_matches'] or not identity['build_provenance_verified']:
        raise ValueError('operational work requires the identified installed build provenance')
    document=json.loads((Path(fsm.REPO)/'acceptance/profiles/operational.json').read_text())
    digest=workload_digest(fsm.REPO,document['profiles'],args.seed,args.profile)
    calibration=calibration_for(args.host,document['calibrations'],digest) if args.mode=='validate' else None
    state=RunState(document['profiles'][args.profile]);entries=schedule(args.seed,state.profile['maximum_cycles'])
    report=Report('operational_'+args.mode);store=root/'store';store.mkdir()
    manifest=dict(schema='fsm.operational-observations/1',mode=args.mode,profile=args.profile,seed=args.seed,
        host=args.host,platform=platform.platform(),machine=platform.machine(),candidate=identity,
        workload_sha256=digest,verdict='incomplete',completed=0,cycles=[],failures=[],task_complete=False)
    path=root/'operational.json';path.write_text(json.dumps(manifest,indent=2,sort_keys=True))
    def consume(observation):
        index=observation['schedule']['index']
        active=observed_sample(observation,'active',store,root)
        quiet=observed_sample(observation,'quiescent',store,root)
        warmed=observed_sample(observation,'warmed',store,root)
        if any(quiet['metrics'][key]!=0 for key in ('active_children','surviving_descendants','capture_bytes')):
            raise ValueError('required original resources remain at quiescence')
        if calibration is not None:
            validate_sample(active,calibration);validate_sample(quiet,calibration,warmed)
        artifact=root/f'cycle-{index:08}.json'
        artifact.write_text(json.dumps(dict(observation=observation,samples=[active,quiet],warmed=warmed),sort_keys=True))
        state.complete(observation['schedule'],observation['completed_ns'])
        manifest['cycles'].append(dict(index=index,case=observation['schedule']['case'],
            path=artifact.name,sha256=hashlib.sha256(artifact.read_bytes()).hexdigest()))
        manifest['completed']=state.completed
        path.write_text(json.dumps(manifest,indent=2,sort_keys=True))
        return state.verdict()=='running'
    try:
        while state.verdict()=='running':
            block=list(islice(entries,12))
            if len(block)!=12:raise ValueError('finite schedule ended before a complete host block')
            run_block(report,store,block,consume)
        if state.verdict()!='passed':raise ValueError('operational run is incomplete: '+str(state.reason))
        manifest.update(verdict='calibration_observed' if args.mode=='calibrate' else 'passed',
                        elapsed_ns=state.last_progress-state.started,cleanup_verified=True,
                        assertions=len(report.checks),native_handler_execution=True)
        return 0
    except Exception as error:
        manifest['failures'].append(str(error));manifest['verdict']='incomplete'
        raise
    finally:
        manifest['assertion_records']=report.checks
        manifest['notes']=report.notes
        path.write_text(json.dumps(manifest,indent=2,sort_keys=True))


if __name__=='__main__':raise SystemExit(main())
