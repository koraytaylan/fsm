"""Install an identified candidate and run operational evidence on disposable CI."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

REPO=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(REPO))
from acceptance.suite.evidence import snapshot, digest, command, verify_source
from acceptance.suite.native_fixture import require_disposable_runner, privileged, AUTHORITY, BASE
from acceptance.suite.fsm import task_cache

BUILD=r'''import json,os,subprocess,sys
from pathlib import Path
from acceptance.suite.evidence import build,verify_source
source,install,evidence=map(Path,sys.argv[1:])
rows=Path('/proc/self/cgroup').read_text().splitlines()
groups=[row[3:] for row in rows if row.startswith('0::')]
if len(groups)!=1: raise ValueError('controlled build requires one cgroup v2 identity')
group=Path('/sys/fs/cgroup')/groups[0].lstrip('/')
limits={name:(group/name).read_text().strip() for name in ('memory.max','memory.swap.max')}
if limits!={'memory.max':'1073741824','memory.swap.max':'0'}: raise ValueError('controlled build limits differ')
(evidence/'build-limits.json').write_text(json.dumps(dict(cgroup=groups[0],limits=limits)))
build(source,install)
subprocess.run(['cargo','test','--locked','-p','fsm-store','--test','archive_operation','--test','seal_safety'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','test','--locked','-p','fsm-cli','--test','sealed_diagnostics'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','test','--locked','-p','fsm-cli','--test','review_regressions','snapshot_divergence'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','clippy','--locked','-p','fsm-cli','--all-targets','--','-D','warnings'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','clippy','--locked','-p','fsm-store','--all-targets','--','-D','warnings'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','test','--locked','-p','fsm-execute','--lib','run::native_owners'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','clippy','--locked','-p','fsm-execute','--all-targets','--','-D','warnings'],
               cwd=source,check=True,timeout=180)
subprocess.run(['cargo','build','--locked','-p','fsm-execute','--bin','fsm-containment-authority'],
               cwd=source,check=True,timeout=300)
verify_source(source,json.loads((source/'source.json').read_text()))
'''


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate',required=True)
    parser.add_argument('--mode',choices=('calibrate','validate'),required=True)
    parser.add_argument('--profile',choices=('smoke','sustained'),required=True)
    parser.add_argument('--seed',type=int,required=True)
    parser.add_argument('--host',required=True)
    args=parser.parse_args()
    require_disposable_runner()
    if not re.fullmatch('[a-f0-9]{40}',args.candidate) or command(['git','rev-parse','HEAD'],str(REPO))!=args.candidate:
        raise ValueError('an exact immutable checkout is required')
    if AUTHORITY.exists() or BASE.exists():raise ValueError('operational installation needs a fresh disposable runner')
    cache=Path(task_cache())/'operational-check';cache.mkdir()
    evidence=cache/'evidence';evidence.mkdir()
    source=cache/'source';install=cache/'install'
    control=dict(schema='fsm.operational-producer/1',candidate=args.candidate,mode=args.mode,
                 profile=args.profile,passed=False,authority_removed=False,task_complete=False)
    installed=None;authority_sha=None
    try:
        manifest=snapshot(REPO,source)
        if manifest['dirty']:raise ValueError('operational snapshot is dirty')
        environment={**os.environ,'CARGO_BUILD_JOBS':'1','CARGO_PROFILE_DEV_STRIP':'debuginfo'}
        for name in ('CARGO_TARGET_DIR','TMPDIR'):
            if not environment.get(name):raise ValueError('controlled build requires explicit '+name)
        settings={name:environment[name] for name in ('PATH','TMPDIR','CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_PROFILE_DEV_STRIP')}
        settings.update(PYTHONDONTWRITEBYTECODE='1',RUSTUP_TOOLCHAIN=os.environ.get('RUSTUP_TOOLCHAIN','stable'),
                        RUSTUP_HOME=os.environ.get('RUSTUP_HOME',str(Path.home()/'.rustup')),
                        CARGO_HOME=os.environ.get('CARGO_HOME',str(Path.home()/'.cargo')))
        with (evidence/'build.log').open('wb') as log:
            subprocess.run(['sudo','-n','systemd-run','--quiet','--wait','--pipe','--collect',
                f'--uid={os.geteuid()}',f'--gid={os.getegid()}',f'--working-directory={source}',
                '--property=MemoryMax=1073741824','--property=MemorySwapMax=0',
                '--property=RuntimeMaxSec=600',*[f'--setenv={name}={value}' for name,value in settings.items()],
                sys.executable,'-B','-c',BUILD,str(source),str(install),str(evidence)],
                stdout=log,stderr=subprocess.STDOUT,check=True,timeout=630)
        binary=install/'bin/fsm';authority=Path(environment['CARGO_TARGET_DIR'])/'debug/fsm-containment-authority'
        authority_sha=digest(authority)
        installer=source/'crates/fsm-execute/tests/lifecycle_platform/authority_install.py'
        installed=json.loads(privileged(sys.executable,str(installer),'install','--source',str(authority),'--sha256',authority_sha))
        privileged('mkdir','-m','0755',str(BASE))
        temporary=cache/'temporary';temporary.mkdir()
        receipt=install/'share/fsm-build.json'
        environment.update(TMPDIR=str(temporary),FSM_BIN=str(binary),FSM_REPO=str(source),
            FSM_BUILD_RECEIPT=str(receipt),FSM_CANDIDATE_REVISION=args.candidate,FSM_CANDIDATE_SHA256=digest(binary))
        for path,name in ((binary,'fsm-installed-binary'),(authority,'fsm-containment-authority-built'),(receipt,'build-receipt.json')):
            shutil.copy2(path,evidence/name)
        with (evidence/'consumer.log').open('wb') as log:
            result=subprocess.run([sys.executable,'-B','-m','acceptance.suite.soak_run',
                '--mode',args.mode,'--profile',args.profile,'--seed',str(args.seed),'--host',args.host,
                '--report-dir',str(evidence/'observations')],cwd=source,env=environment,
                stdout=log,stderr=subprocess.STDOUT,timeout=1860 if args.profile=='smoke' else 36060)
        control['consumer_exit']=result.returncode
        report=json.loads((evidence/'observations/operational.json').read_text())
        expected='calibration_observed' if args.mode=='calibrate' else 'passed'
        if result.returncode or report['verdict']!=expected or report.get('cleanup_verified') is not True:
            raise ValueError('operational workload did not establish its declared verdict and cleanup')
        verify_source(source,manifest)
        privileged(sys.executable,str(installer),'remove','--device',str(installed['device']),
                   '--inode',str(installed['inode']),'--sha256',authority_sha)
        privileged('rmdir',str(BASE));installed=None
        control.update(passed=True,authority_removed=True,verdict=expected,
                       report_sha256=digest(evidence/'observations/operational.json'))
        return 0
    except Exception as error:
        control['error']=str(error)
        print('operational evidence failed: '+str(error),file=sys.stderr)
        return 1
    finally:
        temporary=cache/'temporary'
        if temporary.exists():
            for directory in temporary.glob('installed-native-*'):
                shutil.copytree(directory,evidence/directory.name)
        control['retained_authority']=installed is not None
        (evidence/'producer.json').write_text(json.dumps(control,indent=2,sort_keys=True))
        print('operational evidence: '+str(evidence))


if __name__=='__main__':raise SystemExit(main())
