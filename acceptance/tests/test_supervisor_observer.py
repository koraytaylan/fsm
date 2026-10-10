"""Labelled protocol/owner stubs and faulty receipts, never native proof."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest

from acceptance.suite.executor_supervisor import supervisor_observation
from acceptance.suite.executor_lifecycle import original_interrupted_completion, original_completion
from acceptance.suite.fsm import Scratch
from acceptance.suite.native_fixture import BROKER_OWNER

NAMESPACE = "a" * 32


def record(phase="dead"):
    epoch = 1 if phase == "dead" else 2
    value = dict(format="fsm.acceptance-supervisor/1", namespace=NAMESPACE, phase=phase,
        process=dict(pid=123 if epoch == 1 else 456, pid_starttime="100" if epoch == 1 else "200"),
        returncode=-9 if epoch == 1 else None,
        route=dict(format="fsm.native-broker-route/1", epoch=epoch,
            configuration=dict(format="fsm.native-broker-config/1", operator=1000,
                authority=dict(device=5, inode=6), boot="11111111-1111-1111-1111-111111111111"),
            socket=dict(device=5, inode=epoch)))
    return rehash(dict(value=value, path=f"/usr/libexec/fsm-acceptance-{NAMESPACE}/supervisor-{phase}.json",
        uid=0, mode=0o444, device=5, inode=9, sha256=""))


def rehash(observation):
    observation["sha256"] = hashlib.sha256(json.dumps(observation["value"], sort_keys=True,
        separators=(",", ":")).encode()).hexdigest()
    return observation


class SupervisorSchemaTests(unittest.TestCase):
    def test_original_forced_death_then_distinct_epoch_and_identity_passes(self):
        dead = supervisor_observation(record(), NAMESPACE, "dead")
        self.assertEqual(dead["returncode"], -9)
        restarted = supervisor_observation(record("restarted"), NAMESPACE, "restarted", dead)
        self.assertEqual(restarted["route"]["epoch"], 2)

    def test_protection_hash_path_namespace_and_closed_fields_refuse(self):
        for key, value in (("uid", 1000), ("uid", False), ("mode", 0o644), ("inode", True),
            ("sha256", "0" * 64), ("path", "/other/receipt")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                supervisor_observation({**record(), key: value}, NAMESPACE, "dead")
        for changes in ({"namespace": "b" * 32}, {"phase": "restarted"}, {"extra": True}):
            observation=record(); observation["value"].update(changes)
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                supervisor_observation(rehash(observation), NAMESPACE, "dead")

    def test_exit_code_epoch_and_birth_identity_cannot_be_substituted(self):
        changes = [dict(returncode=0), dict(returncode=-15), dict(process=dict(pid=True,pid_starttime="100")),
            dict(process=dict(pid=123,pid_starttime="001")), dict(process=dict(pid=123,pid_starttime="x"))]
        for change in changes:
            observation=record(); observation["value"].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                supervisor_observation(rehash(observation), NAMESPACE, "dead")
        for epoch in (True, 0, 2, 4097):
            observation=record(); observation["value"]["route"]["epoch"]=epoch
            with self.subTest(epoch=epoch), self.assertRaises(ValueError):
                supervisor_observation(rehash(observation), NAMESPACE, "dead")

    def test_successor_cannot_reuse_original_identity_socket_or_foreign_configuration(self):
        dead=supervisor_observation(record(),NAMESPACE,"dead")
        for name in ("process", "socket", "configuration"):
            observation=record("restarted")
            if name=="process": observation["value"][name]=dead[name]
            elif name=="socket": observation["value"]["route"][name]=dead["route"][name]
            else: observation["value"]["route"][name]["authority"]["inode"]+=1
            with self.subTest(name=name), self.assertRaises(ValueError):
                supervisor_observation(rehash(observation),NAMESPACE,"restarted",dead)
        with self.assertRaises(ValueError):
            supervisor_observation(record("restarted"),NAMESPACE,"restarted")

    def test_attested_no_candidate_is_interruption_evidence_and_not_a_cancellation_payload(self):
        claim=dict(hash="f"*64,body=dict(attempt=1,domain=dict(allocation=1),effect_id="fixture/1/0",
            handler_fingerprint="sha256:"+"a"*64,instance_id="fixture",retry=dict(attempts=1),run_id=1))
        response=dict(format="fsm.native-response/1",ok=True,result=dict(format="fsm.native-run-result/3",
            claim=claim["body"],journal_claim="sha256:"+claim["hash"],failure_class=None,candidate=None))
        encoded=json.dumps(response,sort_keys=True,separators=(",",":"),ensure_ascii=False).encode()
        attestation=dict(format="fsm.native-result-attestation/1",domain=claim["body"]["domain"],run_id=1,
            journal_claim=response["result"]["journal_claim"],response_hash="sha256:"+hashlib.sha256(b"fsm:native-response:1\n"+encoded).hexdigest())
        self.assertIsNone(original_interrupted_completion(response,attestation,claim))
        with self.assertRaises(ValueError): original_completion(response,attestation,claim)
        for changed in ({**attestation,"response_hash":"sha256:"+"0"*64}, {**attestation,"run_id":2}):
            with self.assertRaises(ValueError): original_interrupted_completion(response,changed,claim)


STUB = '''
import json,os,time
from pathlib import Path
directory=Path(os.environ['FSM_STUB_BROKER_DIRECTORY'])
counter=directory/'epoch'
epoch=int(counter.read_text())+1 if counter.exists() else 1
counter.write_text(str(epoch))
(directory/'broker/route.json').write_text(json.dumps(dict(format='fsm.native-broker-route/1',epoch=epoch,
    configuration=dict(label='synthetic unprivileged stub'),socket=dict(device=1,inode=epoch))))
(directory/'pid').write_text(str(os.getpid()))
time.sleep(10)
'''


class OwnedBrokerStubTests(unittest.TestCase):
    def wait_marker(self, path, owner):
        deadline=time.monotonic()+3
        while not path.exists():
            self.assertIsNone(owner.poll())
            self.assertLess(time.monotonic(),deadline)
            time.sleep(0.01)
        return path

    def start(self, scratch):
        base=Path(scratch.path)/"authority"
        directory=base/NAMESPACE/"authority-1"
        (directory/"broker").mkdir(parents=True)
        stage=Path(scratch.path)/("stage-"+NAMESPACE);stage.mkdir()
        stub=Path(scratch.path)/"broker.py";stub.write_text(f"#!{sys.executable}\n"+STUB);stub.chmod(0o755)
        code=BROKER_OWNER.replace("/usr/libexec/fsm-containment-authority",str(stub)).replace(
            "/var/lib/fsm-containment",str(base)).replace("/usr/libexec/fsm-acceptance-",str(Path(scratch.path)/"stage-"))
        owner=subprocess.Popen([sys.executable,"-c",code,NAMESPACE],stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,stderr=subprocess.PIPE,env={**os.environ,"FSM_STUB_BROKER_DIRECTORY":str(directory)})
        self.wait_marker(directory/"pid",owner)
        return owner,directory,stage

    def test_owned_actual_child_kill_and_reap_then_new_epoch_child_and_explicit_retirement(self):
        with Scratch("owned-supervisor-stub") as scratch:
            owner,directory,stage=self.start(scratch)
            try:
                original_pid=int((directory/"pid").read_text())
                owner.stdin.write(b"K");owner.stdin.flush()
                dead=json.loads(self.wait_marker(stage/"supervisor-dead.json",owner).read_text())
                self.assertEqual(dead["process"]["pid"],original_pid)
                self.assertEqual(dead["returncode"],-9)
                with self.assertRaises(ProcessLookupError): os.kill(original_pid,0)
                self.assertIsNone(owner.poll())
                owner.stdin.write(b"R");owner.stdin.flush()
                restarted=json.loads(self.wait_marker(stage/"supervisor-restarted.json",owner).read_text())
                self.assertEqual(restarted["route"]["epoch"],2)
                self.assertNotEqual(restarted["process"],dead["process"])
                output,error=owner.communicate(b"Q",timeout=5)
                self.assertEqual(owner.returncode,0,error.decode())
                self.assertIn(b"FSM_ACCEPTANCE_BROKER_RETIRED",output)
                with self.assertRaises(ProcessLookupError): os.kill(restarted["process"]["pid"],0)
            finally:
                if owner.poll() is None: owner.communicate(b"Q",timeout=5)


    def test_restart_without_forced_death_refuses_and_reaps_the_owned_child(self):
        with Scratch("invalid-supervisor-stub") as scratch:
            owner,directory,_=self.start(scratch)
            pid=int((directory/"pid").read_text())
            output,error=owner.communicate(b"R",timeout=5)
            self.assertNotEqual(owner.returncode,0)
            self.assertIn(b"invalid owned broker control sequence",error)
            self.assertNotIn(b"FSM_ACCEPTANCE_BROKER_RETIRED",output)
            with self.assertRaises(ProcessLookupError): os.kill(pid,0)

    def test_dead_broker_without_replacement_cannot_report_successful_retirement(self):
        with Scratch("dead-supervisor-stub") as scratch:
            owner,_,stage=self.start(scratch)
            try:
                owner.stdin.write(b"K");owner.stdin.flush()
                self.wait_marker(stage/"supervisor-dead.json",owner)
                output,_=owner.communicate(b"Q",timeout=5)
                self.assertNotEqual(owner.returncode,0)
                self.assertNotIn(b"FSM_ACCEPTANCE_BROKER_RETIRED",output)
            finally:
                if owner.poll() is None: owner.communicate(b"Q",timeout=5)


class SupervisorFixtureBoundTests(unittest.TestCase):
    def test_release_wait_exact_sixty_seconds_is_bounded_and_plus_one_refuses_before_entry(self):
        fixture = Path(__file__).resolve().parents[1] / "fixtures/executor_handler.py"
        for bound, expected in ((60, 0), (61, 2)):
            with self.subTest(bound=bound), Scratch("supervisor-wait-bound") as scratch:
                root = Path(scratch.dir("resource"))
                release = root / "release"
                release.write_text("already released")
                result = subprocess.run([sys.executable, str(fixture), "operation", "--root", str(root),
                    "--run", "labelled-bound-stub", "--resource", "supplier", "--operation", "validate",
                    "--release", str(release), "--wait-seconds", str(bound)], capture_output=True, timeout=3)
                self.assertEqual(result.returncode, expected, result.stderr.decode())
                self.assertEqual(len(list(root.glob("*.ready"))), 1 if expected == 0 else 0)


if __name__ == "__main__":
    unittest.main()
