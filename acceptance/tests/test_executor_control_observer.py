"""Synthetic local-control stubs reject faulty facts; these are not native proof."""
import json
from pathlib import Path
import socket
import sys
import tempfile
import threading
import unittest

from acceptance.suite.executor_control import PHASES, REPORT_CAP, observe_owner, validate_observation
from acceptance.suite.fsm import task_cache


def identity(device=1, inode=2, incarnation="a" * 64):
    return dict(format="fsm.executor-control-endpoint/1", incarnation=incarnation,
                store_device=device, store_inode=inode)


def report(original):
    return {**original, "format": "fsm.executor-observation-report/1", "phase": "draining",
        "admission_closed": True, "timed_out": False, "unresolved_run_ids": [1],
        "unclaimed_reservations": 0, "inventory_complete": True, "helpers_retired": False,
        "writer_released": False, "preparation_phases": {key: 0 for key in PHASES}}


class ControlSchemaFaultTests(unittest.TestCase):
    def test_original_draining_snapshot_passes_and_unpublished_phases_stay_null(self):
        original = identity()
        value = report(original)
        self.assertEqual(validate_observation(value, original), value)
        value["preparation_phases"] = None
        value["unclaimed_reservations"] = None
        self.assertIsNone(validate_observation(value, original)["preparation_phases"])

    def test_foreign_incarnation_physical_store_unknown_fields_and_nonboolean_facts_refuse(self):
        original = identity()
        for field, changed in (("incarnation", "b" * 64), ("store_device", 99), ("store_inode", True),
            ("extra", 0), ("admission_closed", 1), ("writer_released", None), ("phase", [])):
            value = report(original)
            value[field] = changed
            with self.assertRaises(ValueError):
                validate_observation(value, original)

    def test_inventory_bounds_and_phase_sum_are_checked_without_inventing_zeros(self):
        original = identity()
        for changed in ([True], [-1], [1 << 64], [1] * 4097):
            value = report(original)
            value["unresolved_run_ids"] = changed
            with self.assertRaises(ValueError):
                validate_observation(value, original)
        for changed in ({}, {**report(original)["preparation_phases"], "queued": 1},
                        {**report(original)["preparation_phases"], "queued": True}):
            value = report(original)
            value["preparation_phases"] = changed
            with self.assertRaises(ValueError):
                validate_observation(value, original)


class ControlWireFaultTests(unittest.TestCase):
    def setUp(self):
        self.cache = tempfile.TemporaryDirectory(prefix="o", dir=task_cache())
        self.addCleanup(self.cache.cleanup)
        self.root = Path(self.cache.name)
        self.store = self.root / "data"
        self.store.mkdir()
        physical = self.store.stat()
        self.original = identity(physical.st_dev, physical.st_ino)

    def endpoint(self, original, listening=True):
        directory = self.root / ("c-" + original["incarnation"][:16])
        directory.mkdir(mode=0o700)
        path = directory / "identity"
        path.write_text(json.dumps(original))
        path.chmod(0o600)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        listener.bind(str(directory / "s"))
        (directory / "s").chmod(0o600)
        if listening:
            listener.listen(4)
            listener.settimeout(2)
            self.addCleanup(listener.close)
        else:
            listener.close()
        return listener, directory

    def exchange(self, encoded):
        listener, _directory = self.endpoint(self.original)
        requests = []
        errors = []

        def serve():
            try:
                with listener.accept()[0] as connection:
                    connection.settimeout(2)
                    request = bytearray()
                    while b"\n" not in request:
                        chunk = connection.recv(1024)
                        if not chunk:
                            return
                        request.extend(chunk)
                    requests.append(json.loads(request))
                    try:
                        connection.sendall(encoded)
                    except (BrokenPipeError, ConnectionResetError):
                        pass
            except Exception as error:
                errors.append(error)

        worker = threading.Thread(target=serve)
        worker.start()
        try:
            return observe_owner(self.store, self.root), requests
        finally:
            worker.join(timeout=3)
            self.assertFalse(worker.is_alive())
            self.assertEqual(errors, [])

    def test_actual_wire_keeps_original_identity_and_preserves_refused_sibling_files(self):
        if sys.platform != "linux":
            with self.assertRaises(ValueError):
                observe_owner(self.store, self.root)
            return
        stale = identity(self.original["store_device"], self.original["store_inode"], "b" * 64)
        _listener, directory = self.endpoint(stale, listening=False)
        encoded = json.dumps(report(self.original)).encode() + b"\n"
        observed, requests = self.exchange(encoded)
        self.assertEqual(observed, report(self.original))
        self.assertEqual(requests, [{**self.original, "format": "fsm.executor-observe/2"}])
        self.assertEqual(json.loads((directory / "identity").read_text()), stale)
        self.assertTrue((directory / "s").exists())

    def test_actual_report_exact_byte_limit_passes_and_limit_plus_one_refuses(self):
        if sys.platform != "linux":
            with self.assertRaises(ValueError):
                observe_owner(self.store, self.root)
            return
        encoded = json.dumps(report(self.original)).encode()
        observed, _requests = self.exchange(encoded.ljust(REPORT_CAP, b" ") + b"\n")
        self.assertEqual(observed, report(self.original))

    def test_actual_oversize_wire_cannot_become_a_control_fact(self):
        if sys.platform != "linux":
            with self.assertRaises(ValueError):
                observe_owner(self.store, self.root)
            return
        encoded = json.dumps(report(self.original)).encode()
        with self.assertRaisesRegex(ValueError, "byte bound"):
            self.exchange(encoded.ljust(REPORT_CAP + 1, b" ") + b"\n")

    def test_two_connected_owners_refuse_before_any_observation_request(self):
        if sys.platform != "linux":
            with self.assertRaises(ValueError):
                observe_owner(self.store, self.root)
            return
        first, _directory = self.endpoint(self.original)
        second, _directory = self.endpoint(identity(self.original["store_device"], self.original["store_inode"], "b" * 64))
        with self.assertRaisesRegex(ValueError, "unique connected"):
            observe_owner(self.store, self.root)
        for listener in (first, second):
            with listener.accept()[0] as connection:
                connection.settimeout(1)
                self.assertEqual(connection.recv(1024), b"")
