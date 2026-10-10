"""Disposable-CI resource census of one host and its original native domains.

The protected namespace supplies domain identities; a fixture PID is never
authority to inspect an unrelated process. Samplers are staged immutably next
to the handler, and no privileged local execution or process-tree fallback is
available. Heap RSS and kernel capture queues use separate observed units.
"""
import hashlib
import json
from pathlib import Path

from .native_fixture import privileged, require_disposable_runner, sys_executable

ROOT_SAMPLE = r'''import json,os,re,stat,sys
from pathlib import Path
namespace,host_encoded,uid_encoded=sys.argv[1:]
if os.geteuid()!=0 or not re.fullmatch('[a-f0-9]{32}',namespace): raise ValueError('invalid protected census namespace')
stage=Path('/usr/libexec')/('fsm-acceptance-'+namespace)
sys.path.insert(0,str(stage))
from observation.soak_resources import observe_live_host,observe_pipe_queues
from observation.soak_socket_queues import observe_unix_queues
base=Path('/var/lib/fsm-containment')/namespace/'authority-1'
def protected(path):
 metadata=path.lstat()
 if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid!=0 or metadata.st_mode&0o022: raise ValueError('domain census requires protected original records')
 with path.open('rb') as stream: raw=stream.read(65537)
 if len(raw)>65536: raise ValueError('domain census record exceeds bound')
 return json.loads(raw)
host=json.loads(host_encoded);uid=int(uid_encoded)
if type(host.get('pid')) is not int or not 0<host['pid']<1<<31: raise ValueError('invalid original host PID')
status=Path('/proc',str(host['pid']),'status').read_text()
owners=[line.split()[1:] for line in status.splitlines() if line.startswith('Uid:')]
if len(owners)!=1 or len(owners[0])!=4 or any(int(value)!=uid for value in owners[0]): raise ValueError('original host owner differs')
def process(identity):
 return dict(host=observe_live_host(identity),pipes=observe_pipe_queues(identity),unix=observe_unix_queues(identity))
owner=process(host)
counter=protected(base/'counter.json');count=counter.get('last_allocation')
if counter.get('format')!='fsm.native-allocation-counter/1' or counter.get('namespace')!=namespace or type(counter.get('generation')) is not int or counter['generation']!=1: raise ValueError('original allocation counter identity differs')
if type(count) is not int or not 0<=count<=64: raise ValueError('original domain census exceeds allocation bound')
domains=[];processes=[];seen=set()
for allocation in range(1,count+1):
 binding=protected(base/('binding-'+str(allocation)+'.json'))
 if binding.get('format')!='fsm.native-claim-binding/1': raise ValueError('original binding format differs')
 domain=binding.get('claim',{}).get('domain',{})
 if domain.get('namespace')!=namespace or type(domain.get('generation')) is not int or domain['generation']!=1 or type(domain.get('allocation')) is not int or domain['allocation']!=allocation: raise ValueError('original census binding identity differs')
 closed_path=base/('closed-'+str(allocation)+'.json')
 closed=protected(closed_path) if closed_path.exists() else None
 if closed is not None and (closed.get('format')!='fsm.native-domain-closed/1' or closed.get('domain')!=domain): raise ValueError('original census closure differs')
 group=Path('/sys/fs/cgroup/system.slice')/('fsm-containment-'+namespace+'-1-'+str(allocation)+'.service')
 members=[]
 try:
  metadata=group.stat()
 except FileNotFoundError:
  if closed is None: raise ValueError('unclosed original native domain is unavailable')
 else:
  if domain.get('cgroup')!={'device':metadata.st_dev,'inode':metadata.st_ino}: raise ValueError('original native cgroup identity differs')
  with (group/'cgroup.procs').open('rb') as stream: raw=stream.read(4097)
  if len(raw)>4096: raise ValueError('original native member census exceeds bound')
  members=[int(value) for value in raw.split()]
  if len(members)>64 or len(set(members))!=len(members) or any(not 0<pid<1<<31 for pid in members): raise ValueError('native member census is malformed')
  for pid in members:
   if pid in seen: raise ValueError('original native process occurs in multiple domains')
   seen.add(pid)
   with Path('/proc',str(pid),'stat').open('rb') as stream: raw=stream.read(4097)
   fields=raw.rpartition(b') ')[2].split()
   if len(raw)>4096 or len(fields)<20 or not fields[19].isdigit(): raise ValueError('original native process birth is unavailable')
   identity=dict(pid=pid,pid_starttime=fields[19].decode('ascii'))
   value=process(identity);value.update(allocation=allocation)
   processes.append(value)
 domains.append(dict(binding=binding,closed=closed,members=members))
size=0;files=0
for root in (stage,base.parent,Path('/dev/shm')/('fsm-acceptance-'+namespace)):
 for directory,children,names in os.walk(root,followlinks=False):
  for name in children+names:
   metadata=(Path(directory)/name).lstat();files+=1
   if files>10000 or stat.S_ISLNK(metadata.st_mode): raise ValueError('owned native disk inventory differs')
   if stat.S_ISREG(metadata.st_mode): size+=metadata.st_size
   if size>16777216: raise ValueError('owned native disk inventory exceeds sixteen MiB')
print(json.dumps(dict(namespace=namespace,owner=owner,domains=domains,processes=processes,disk_bytes=size),sort_keys=True))
'''


def stage_sampler(native):
    require_disposable_runner()
    package = native.stage / 'observation'
    privileged('mkdir','-m','0755',str(package))
    initializer = native.cache / 'observation-init.py'
    initializer.write_text('"""Immutable scoped syscall observations."""\n')
    sources = {'__init__.py':initializer}
    for name in ('soak.py','soak_resources.py','soak_socket_queues.py'):
        sources[name] = Path(__file__).with_name(name)
    receipts = []
    for name, source in sources.items():
        target = package / name
        privileged('install','-m','0444',str(source),str(target))
        before, after = source.read_bytes(), target.read_bytes()
        if before != after:
            raise ValueError('protected sampler source changed during staging')
        receipts.append(dict(name=name,sha256=hashlib.sha256(after).hexdigest()))
    (native.cache/'sampler-provisioning.json').write_text(json.dumps(receipts,sort_keys=True))


def sample_native(native, identity):
    require_disposable_runner()
    import os
    raw = privileged('env',f'TMPDIR={native.cache}','PYTHONDONTWRITEBYTECODE=1',
                     sys_executable(),'-B','-c',ROOT_SAMPLE,native.namespace,
                     json.dumps(identity,sort_keys=True),str(os.geteuid()))
    value = json.loads(raw)
    if value.get('namespace') != native.namespace:
        raise ValueError('native metric census changed its original namespace')
    return value


def queue_bytes(census):
    """Deduplicate independently observed kernel queues across owned processes."""
    pipes, sockets = {}, {}
    for row in [census['owner'],*census['processes']]:
        for item in row['pipes']['pipes']:
            key = (item['device'],item['inode'])
            pipes[key] = max(pipes.get(key,0),item['queued_bytes'])
        for item in row['unix']['endpoints']:
            if item['queued_bytes'] is not None:
                sockets[item['inode']] = max(sockets.get(item['inode'],0),item['queued_bytes'])
    return sum(pipes.values())+sum(sockets.values())
