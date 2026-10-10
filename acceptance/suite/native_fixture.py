"""Explicit disposable-CI provisioning for the independent installed client.

This supplies protected authority/catalogue installation and fixture resource
access; it never decides a workflow outcome or imports an engine interpreter.
Privileged operations are forbidden outside an explicitly opted-in disposable
GitHub Linux runner; there is no local or direct-child fallback.
"""
from __future__ import annotations

import json
import hashlib
import os
from pathlib import Path
import platform
import subprocess
import time
import uuid

from .fsm import task_cache

AUTHORITY = Path("/usr/libexec/fsm-containment-authority")
BASE = Path("/var/lib/fsm-containment")
BROKER_OWNER = """import os,re,select,subprocess,sys,time
namespace=sys.argv[1]
if not re.fullmatch('[a-f0-9]{32}',namespace): raise ValueError('invalid fixture namespace')
os.umask(0o077)
child=subprocess.Popen(['/usr/libexec/fsm-containment-authority','serve',namespace,'1'],stdin=subprocess.DEVNULL)
explicit=False
try:
 deadline=time.monotonic()+120
 while child.poll() is None and time.monotonic()<deadline:
  ready,_,_=select.select([sys.stdin.buffer],[],[],0.05)
  if ready:
   explicit=os.read(sys.stdin.fileno(),1)==b'Q'
   break
finally:
 if child.poll() is None:
  child.terminate()
  try: child.wait(timeout=5)
  except subprocess.TimeoutExpired:
   child.kill(); child.wait(timeout=5)
if not explicit or child.returncode not in (0,-15): raise RuntimeError('broker owner interrupted or failed')
print('FSM_ACCEPTANCE_BROKER_RETIRED',flush=True)
"""


def require_disposable_runner() -> None:
    if (os.environ.get("GITHUB_ACTIONS") != "true"
        or os.environ.get("FSM_ACCEPTANCE_DISPOSABLE_NATIVE") != "1"
        or platform.system() != "Linux" or os.geteuid() == 0):
        raise RuntimeError("installed native acceptance requires an opted-in disposable Linux CI operator")


def privileged(*arguments: str) -> str:
    require_disposable_runner()
    result = subprocess.run(["sudo", "-n", *map(str, arguments)], capture_output=True,
                            text=True, timeout=15)
    if len(result.stdout.encode()) + len(result.stderr.encode()) > 1_048_576:
        raise RuntimeError("native provisioning diagnostic bound exceeded")
    if result.returncode:
        raise RuntimeError(f"native provisioning exited {result.returncode}: {result.stderr[-65536:]}{result.stdout[-65536:]}")
    return result.stdout


class DisposableAuthority:
    """One fresh namespace, protected script and isolated external resource.

    Successful teardown requires every allocated domain's immutable closed
    record before removing owned fixture paths; uncertain failures retain the
    original authority and diagnostic files on the disposable runner.
    """

    def __init__(self, handler: Path):
        self.handler_source = handler.resolve()
        self.namespace = uuid.uuid4().hex
        self.directory = BASE / self.namespace / "authority-1"
        self.stage = Path("/usr/libexec") / ("fsm-acceptance-" + self.namespace)
        # DynamicUser makes ordinary system paths read-only; the existing
        # native fixture policy deliberately shares only this /dev/shm resource.
        self.resource = Path("/dev/shm") / ("fsm-acceptance-" + self.namespace)
        self.handler = self.stage / "executor_handler.py"
        self.release = self.resource / "release"
        self.cache = None
        self.process = None
        self.log = None
        self.store = None
        self.identities = {}
        self.cleaned = False

    def _identity(self, path: Path) -> tuple[int, int]:
        fields = privileged("stat", "-c", "%d:%i", str(path)).strip().split(":")
        if len(fields) != 2 or any(not field.isdecimal() for field in fields):
            raise RuntimeError("fixture identity observation failed")
        return tuple(map(int, fields))

    def _remember(self, path: Path) -> None:
        self.identities[str(path)] = self._identity(path)

    def __enter__(self):
        require_disposable_runner()
        if not AUTHORITY.exists():
            raise RuntimeError("the disposable authority must be installed before acceptance")
        self.cache = Path(task_cache()) / ("installed-native-" + self.namespace)
        self.cache.mkdir()
        try:
            privileged("mkdir", "-m", "0755", str(self.stage))
            self._remember(self.stage)
            privileged("install", "-m", "0555", str(self.handler_source), str(self.handler))
            original = self.handler_source.read_bytes()
            staged = self.handler.read_bytes()
            if len(original) > 65_536 or staged != original:
                raise RuntimeError("staged fixture bytes differ from the controlled source")
            (self.cache / "handler-provisioning.json").write_text(json.dumps(dict(
                source_sha256=hashlib.sha256(original).hexdigest(),
                staged_sha256=hashlib.sha256(staged).hexdigest(), staged_path=str(self.handler))))
            privileged("mkdir", "-m", "0777", str(self.resource))
            self._remember(self.resource)
            # Different DynamicUser identities share only these fixture logs.
            for name in ("trace.jsonl", "results.jsonl"):
                privileged("install", "-m", "0666", "/dev/null", str(self.resource / name))
            for name, value in (("sequence.json", 0),
                                (hashlib.sha256(b"supplier").hexdigest() + ".json",
                                 {"suspended": False, "items": []})):
                source = self.cache / name
                source.write_text(json.dumps(value), encoding="utf-8")
                privileged("install", "-m", "0666", str(source), str(self.resource / name))
            privileged("mkdir", "-m", "0777", str(self.resource / "runs"))
            privileged("mkdir", "-m", "0755", str(BASE / self.namespace))
            self._remember(BASE / self.namespace)
        except BaseException:
            self._retire(False)
            raise
        return self

    def approve(self, store: Path, table: dict) -> Path:
        """Root approves the operator table before the installed host loads it."""
        self.store = store.resolve()
        table_path = self.cache / "handlers.json"
        # The authority accepts canonical records, including the input table;
        # fixture tables contain only strings, integers, lists and objects.
        encoded = json.dumps(table, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        if len(encoded.encode()) > 64 * 1024:
            raise ValueError("fixture catalogue exceeds its provisioning bound")
        table_path.write_text(encoded, encoding="utf-8")
        privileged(str(AUTHORITY), "register", self.namespace, "1", str(self.store))
        protected = BASE / self.namespace / "handlers.json"
        privileged("install", "-m", "0600", str(table_path), str(protected))
        privileged(str(AUTHORITY), "catalogue", self.namespace, "1", str(protected))
        privileged(str(AUTHORITY), "provision-broker", self.namespace, "1", str(os.geteuid()))
        self.log = (self.cache / "broker.log").open("wb")
        self.process = subprocess.Popen(
            ["sudo", "-n", sys_executable(), "-c", BROKER_OWNER, self.namespace],
            stdin=subprocess.PIPE, stdout=self.log, stderr=subprocess.STDOUT,
            start_new_session=True, umask=0o077)
        deadline = time.monotonic() + 10
        while not (self.directory / "broker" / "route.json").exists():
            if self.process.poll() is not None or time.monotonic() >= deadline:
                raise RuntimeError("disposable broker did not publish its protected route")
            time.sleep(0.01)
        return table_path

    def _closed_domains(self) -> list[dict]:
        counter = json.loads(privileged("cat", str(self.directory / "counter.json")))
        count = counter["last_allocation"]
        if type(count) is not int or not 0 <= count <= 64:
            raise RuntimeError("fixture allocation inventory exceeds its bound")
        records = []
        for allocation in range(1, count + 1):
            closed = json.loads(privileged("cat", str(self.directory / f"closed-{allocation}.json")))
            domain = closed.get("domain", {})
            if (closed.get("format") != "fsm.native-domain-closed/1"
                or domain.get("namespace") != self.namespace or domain.get("generation") != 1
                or domain.get("allocation") != allocation):
                raise RuntimeError("original fixture domain closure is missing or mismatched")
            records.append(closed)
        return records

    def _stop_broker(self) -> None:
        if self.process is None:
            return
        if self.process.poll() is not None:
            raise RuntimeError("disposable broker exited before explicit retirement")
        # The root owner retires its own unreaped child; the operator never
        # signals a numeric privileged PID or an unverified process group.
        self.process.stdin.write(b"Q")
        self.process.stdin.flush()
        self.process.stdin.close()
        self.process.wait(timeout=15)
        if self.process.returncode != 0:
            raise RuntimeError("disposable broker retirement was not observed")

    def _remove_owned(self) -> None:
        # Fixed code validates inode identity and allowed parents again as root;
        # it never accepts a caller-supplied recursive deletion path.
        code = """import json,os,pathlib,re,shutil,sys
rows=json.loads(sys.argv[1])
for name,identity in rows.items():
 p=pathlib.Path(name); m=p.lstat()
 if p.parent not in (pathlib.Path('/usr/libexec'),pathlib.Path('/dev/shm'),pathlib.Path('/var/lib/fsm-containment')): raise ValueError('fixture parent differs')
 if not re.fullmatch(r'(fsm-acceptance-)?[a-f0-9]{32}',p.name): raise ValueError('fixture name differs')
 if not p.is_dir() or p.is_symlink() or m.st_uid!=0 or [m.st_dev,m.st_ino]!=identity: raise ValueError('fixture identity differs')
for name in rows: shutil.rmtree(name)
"""
        privileged(sys_executable(), "-c", code, json.dumps(self.identities))

    def _capture_records(self) -> None:
        if str(BASE / self.namespace) not in self.identities:
            return
        code = """import hashlib,json,pathlib,re,sys
namespace=sys.argv[1]
if not re.fullmatch('[a-f0-9]{32}',namespace): raise ValueError('invalid fixture namespace')
base=pathlib.Path('/var/lib/fsm-containment')/namespace
rows=[]; size=0
for path in base.rglob('*.json'):
 if len(rows)>=128 or path.is_symlink() or not path.is_file(): raise ValueError('fixture evidence inventory differs')
 with path.open('rb') as stream: data=stream.read(65537)
 size+=len(data)
 if len(data)>65536 or size>524288: raise ValueError('fixture evidence exceeds bound')
 rows.append(dict(path=str(path.relative_to(base)),sha256=hashlib.sha256(data).hexdigest(),value=json.loads(data)))
print(json.dumps(dict(namespace=namespace,records=rows),sort_keys=True))
"""
        encoded = privileged(sys_executable(), "-c", code, self.namespace)
        (self.cache / "authority-records.json").write_text(encoded, encoding="utf-8")

    def _retire(self, successful: bool) -> None:
        if self.cache is None:
            return
        error = None
        closures = []
        try:
            if self.process is not None:
                closures = self._closed_domains()
        except Exception as finding:
            error = finding
        try:
            self._stop_broker()
        except Exception as finding:
            error = error or finding
        if self.log is not None:
            self.log.close()
        try:
            self._capture_records()
        except Exception as finding:
            error = error or finding
        if successful and error is None:
            try:
                self._remove_owned()
                self.cleaned = True
            except Exception as finding:
                error = finding
        retained = dict(namespace=self.namespace, cleaned=self.cleaned,
                        original_closed_domains=closures,
                        error=str(error) if error else None,
                        retained_authority=not self.cleaned)
        (self.cache / "retirement.json").write_text(json.dumps(retained, indent=2), encoding="utf-8")
        if error is not None:
            raise RuntimeError("native fixture retirement incomplete: " + str(error)) from error

    def __exit__(self, exception_type, *_rest):
        self._retire(exception_type is None)


def sys_executable() -> str:
    import sys
    return sys.executable
