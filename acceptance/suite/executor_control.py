"""Independent bounded local control observations, never native closure proof."""
import json
import os
from pathlib import Path
import re
import socket
import stat
import sys
import time

from .executor_scenarios import _unique_object, _invalid_constant

PHASES = {"queued", "preparing", "prepared", "cleaning", "unknown_allocation",
          "uncertain_preparation", "uncertain_cleanup", "uncertain_domain", "claim_uncertain", "closed"}
REPORT_CAP = 131_072


def validate_observation(value: dict, identity: dict) -> dict:
    """Validate the published closed observation schema and exact original owner."""
    fields = {"format", "incarnation", "store_device", "store_inode", "phase", "admission_closed",
              "timed_out", "unresolved_run_ids", "unclaimed_reservations", "inventory_complete",
              "helpers_retired", "writer_released", "preparation_phases"}
    booleans = ("admission_closed", "timed_out", "inventory_complete", "helpers_retired", "writer_released")
    if (not isinstance(value, dict) or set(value) != fields
        or value["format"] != "fsm.executor-observation-report/1"
        or any(type(value[key]) is not type(identity[key]) or value[key] != identity[key]
               for key in ("incarnation", "store_device", "store_inode"))
        or not isinstance(value["phase"], str)
        or value["phase"] not in {"running", "draining", "stopping", "stopped", "uncertain"}
        or any(type(value[key]) is not bool for key in booleans)
        or (value["phase"] == "stopped" and value["admission_closed"] is not True)):
        raise ValueError("control observation schema or original identity differs")
    ids = value["unresolved_run_ids"]
    reservations = value["unclaimed_reservations"]
    phases = value["preparation_phases"]
    if (not isinstance(ids, list) or len(ids) > 4096
        or any(type(run) is not int or not 0 <= run < 1 << 64 for run in ids)
        or (reservations is not None and (type(reservations) is not int or not 0 <= reservations <= 4096))
        or (phases is not None and (not isinstance(phases, dict) or set(phases) != PHASES
            or any(type(count) is not int or not 0 <= count <= 4096 for count in phases.values())
            or sum(phases.values()) != reservations))):
        raise ValueError("control observation inventory is unverified or exceeds its bounds")
    return value


def _private(path: Path, kind, mode: int) -> None:
    metadata = path.lstat()
    if not kind(metadata.st_mode) or metadata.st_uid != os.geteuid() or stat.S_IMODE(metadata.st_mode) != mode:
        raise ValueError("control observation needs an original owner-only path")


def observe_owner(store: Path, root: Path | None = None) -> dict:
    """Observe one connected original physical-store owner within two seconds."""
    if sys.platform != "linux":
        raise ValueError("native local control observation requires Linux")
    if root is None:
        root = Path(os.environ["HOME"]) / ".cache/fsm/control"
    deadline = time.monotonic() + 2

    def remaining():
        budget = deadline - time.monotonic()
        if budget <= 0:
            raise TimeoutError("control observation deadline; no cleanup is confirmed")
        return budget

    _private(root, stat.S_ISDIR, 0o700)
    physical = store.stat()
    connections = []
    try:
        for index, directory in enumerate(root.iterdir()):
            remaining()
            if index >= 64:
                raise ValueError("control observation discovery exceeds its entry bound")
            if not re.fullmatch(r"c-[0-9a-f]{16}", directory.name):
                continue
            _private(directory, stat.S_ISDIR, 0o700)
            path = directory / "identity"
            _private(path, stat.S_ISREG, 0o600)
            with path.open("rb") as stream:
                encoded = stream.read(1025)
            if len(encoded) > 1024:
                raise ValueError("control observation identity exceeds its byte bound")
            identity = json.loads(encoded, object_pairs_hook=_unique_object, parse_constant=_invalid_constant)
            if (not isinstance(identity, dict)
                or set(identity) != {"format", "incarnation", "store_device", "store_inode"}
                or identity["format"] != "fsm.executor-control-endpoint/1"
                or not isinstance(identity["incarnation"], str)
                or not re.fullmatch(r"[0-9a-f]{64}", identity["incarnation"])
                or directory.name != "c-" + identity["incarnation"][:16]
                or any(type(identity[key]) is not int or not 0 <= identity[key] < 1 << 64
                       for key in ("store_device", "store_inode"))):
                raise ValueError("control observation original identity is malformed")
            if (identity["store_device"], identity["store_inode"]) != (physical.st_dev, physical.st_ino):
                continue
            _private(directory / "s", stat.S_ISSOCK, 0o600)
            connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                connection.settimeout(remaining())
                connection.connect(str(directory / "s"))
            except ConnectionRefusedError:
                connection.close()
                continue
            except BaseException:
                connection.close()
                raise
            connections.append((connection, identity))
        if len(connections) != 1:
            raise ValueError("control observation has no unique connected original owner")
        connection, identity = connections[0]
        request = {**identity, "format": "fsm.executor-observe/2"}
        connection.settimeout(remaining())
        connection.sendall(json.dumps(request, sort_keys=True, separators=(",", ":")).encode() + b"\n")
        encoded = bytearray()
        while b"\n" not in encoded:
            connection.settimeout(remaining())
            chunk = connection.recv(min(4096, REPORT_CAP + 2 - len(encoded)))
            if not chunk:
                raise ValueError("control observation ended without a complete report")
            encoded.extend(chunk)
            if len(encoded) > REPORT_CAP + 1:
                raise ValueError("control observation report exceeds its byte bound")
        if encoded.count(b"\n") != 1 or not encoded.endswith(b"\n"):
            raise ValueError("control observation returned extra frames")
        value = json.loads(encoded[:-1], object_pairs_hook=_unique_object, parse_constant=_invalid_constant)
        return validate_observation(value, identity)
    finally:
        for connection, _identity in connections:
            connection.close()
