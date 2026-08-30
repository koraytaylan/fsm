"""Driving the shipped `fsm` binary, and the scratch stores scenarios need."""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import tempfile
import time

FSM = os.environ.get("FSM_BIN", "fsm")
REPO = os.environ.get("FSM_REPO", "/src")


class CliError(RuntimeError):
    pass


class Result:
    def __init__(self, code: int, out: str, err: str, argv: list[str]) -> None:
        self.code = code
        self.out = out
        self.err = err
        self.argv = argv

    @property
    def text(self) -> str:
        return self.out + self.err

    def json(self) -> dict:
        try:
            return json.loads(self.out)
        except json.JSONDecodeError as error:
            raise CliError(
                f"{' '.join(self.argv)} did not print JSON ({error}):\n{self.text}"
            ) from None

    def ok(self) -> "Result":
        if self.code != 0:
            raise CliError(f"{' '.join(self.argv)} exited {self.code}:\n{self.text}")
        return self

    def failed(self) -> "Result":
        if self.code == 0:
            raise CliError(f"{' '.join(self.argv)} was expected to fail:\n{self.text}")
        return self


def run(*args: str, data_dir: str | None = None, env: dict | None = None,
        timeout: int = 120, cwd: str | None = None) -> Result:
    argv = [FSM, *args]
    if data_dir:
        argv.append(f"--data-dir={data_dir}")
    environment = {**os.environ, "NO_COLOR": "1", **(env or {})}
    completed = subprocess.run(
        argv, capture_output=True, text=True, timeout=timeout,
        env=environment, cwd=cwd,
    )
    return Result(completed.returncode, completed.stdout, completed.stderr, argv)


def run_json(*args: str, **kwargs) -> dict:
    return run(*args, "--json", **kwargs).ok().json()


def example(name: str) -> str:
    return os.path.join(REPO, "examples", name)


def fixture_machine(name: str) -> str:
    return os.path.join(REPO, "crates/fsm-core/tests/fixtures/machines", name)


class Scratch:
    """A throwaway directory, removed when the scenario ends."""

    def __init__(self, tag: str) -> None:
        self.path = tempfile.mkdtemp(prefix=f"fsm-acceptance-{tag}-")

    def __enter__(self) -> "Scratch":
        return self

    def __exit__(self, *_exc) -> None:
        shutil.rmtree(self.path, ignore_errors=True)

    def join(self, *parts: str) -> str:
        path = os.path.join(self.path, *parts)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        return path

    def dir(self, name: str) -> str:
        path = os.path.join(self.path, name)
        os.makedirs(path, exist_ok=True)
        return path

    def write(self, name: str, text: str) -> str:
        path = os.path.join(self.path, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        return path


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class Executing:
    """`fsm execute` in the background, stopped once its work has landed.

    There is no `--once`: the executor is a loop by design, because an effect
    outbox is a thing you watch rather than drain. So the suite runs it the way
    an operator does and waits on the *store* for the outcome, which is also
    the only honest way to assert "unattended".
    """

    def __init__(self, data_dir: str, handlers: str, *extra: str) -> None:
        self.data_dir = data_dir
        self.process = subprocess.Popen(
            [FSM, "execute", f"--handlers={handlers}", f"--data-dir={data_dir}",
             "--poll-interval-ms=50", *extra],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            env={**os.environ, "NO_COLOR": "1"},
        )

    def until(self, predicate, timeout: float = 60.0, poll: float = 0.2) -> bool:
        """Wait for the store to satisfy `predicate`, then stop the executor."""
        deadline = time.monotonic() + timeout
        try:
            while time.monotonic() < deadline:
                if predicate():
                    return True
                if self.process.poll() is not None:
                    out = self.process.stdout.read() if self.process.stdout else ""
                    err = self.process.stderr.read() if self.process.stderr else ""
                    raise CliError(f"the executor exited early:\n{out}{err}")
                time.sleep(poll)
            return False
        finally:
            self.stop()

    def stop(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)


class Serving:
    """`fsm serve --http` in the background, with its port already listening."""

    def __init__(self, data_dir: str, port: int, *extra: str) -> None:
        self.port = port
        self.process = subprocess.Popen(
            [FSM, "serve", f"--http={port}", f"--data-dir={data_dir}", *extra],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            env={**os.environ, "NO_COLOR": "1"},
        )

    def __enter__(self) -> "Serving":
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise CliError(
                    "the server exited before it listened:\n"
                    + (self.process.stderr.read() if self.process.stderr else "")
                )
            try:
                with socket.create_connection(("127.0.0.1", self.port), timeout=1):
                    return self
            except OSError:
                time.sleep(0.1)
        raise CliError(f"the server never listened on {self.port}")

    def __exit__(self, *_exc) -> None:
        self.process.terminate()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=10)
