"""Bounded original MCP capture for a genuine external model client.

The operator provisions the service; this module only relays the client's own
requests, observes a final-answer receipt, and enforces the frozen budgets.
It supplies no definitions, outcomes, prompts or corrective tool responses.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import uuid

MAXIMUM_FRAME = 1_048_576
MAXIMUM_CAPTURE = 64 * 1_048_576


def publish(path, value):
    temporary = path.with_suffix('.pending')
    with temporary.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, sort_keys=True)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


class Budget:
    """Count all post-delivery tool/resource/prompt requests conservatively."""
    def __init__(self, started, maximum_calls=40, maximum_seconds=900):
        self.started = started
        self.maximum_calls = maximum_calls
        self.maximum_seconds = maximum_seconds
        self.calls = 0

    def observe(self, frame, observed):
        if isinstance(frame.get('method'), str) and frame['method'].startswith(
                ('tools/', 'resources/', 'prompts/')):
            self.calls += 1
        if self.calls > self.maximum_calls:
            raise RuntimeError('model call budget exceeded')
        self.finish(observed)

    def finish(self, observed):
        elapsed = observed - self.started
        if elapsed < 0 or elapsed > self.maximum_seconds:
            raise RuntimeError('model elapsed budget exceeded or clock reversed')
        return elapsed


class Session:
    def __init__(self, directory, arguments, environment):
        self.directory = directory
        self.address = 'fsm-live-' + uuid.uuid4().hex
        self.arguments = arguments
        self.environment = environment
        self.lock = threading.Lock()
        self.sequence = 0
        self.bytes = 0
        self.failure = None
        self.budget = None
        self.process = None
        self.connection = None
        self.workers = []
        self.initialization = []

    def capture(self, direction, encoded):
        if len(encoded) > MAXIMUM_FRAME:
            raise RuntimeError('original MCP frame exceeds capture bound')
        observed = time.monotonic()
        with self.lock:
            self.bytes += len(encoded)
            if self.bytes > MAXIMUM_CAPTURE:
                raise RuntimeError('original MCP capture exceeds bound')
            self.sequence += 1
            with (self.directory / 'wire.jsonl').open('a', encoding='utf-8') as stream:
                stream.write(json.dumps(dict(sequence=self.sequence, direction=direction,
                    monotonic=observed, frame=encoded.decode('utf-8'))) + '\n')
        return observed

    def fail(self, error):
        with self.lock:
            if self.failure is None:
                self.failure = str(error)
        if self.connection is not None:
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

    def forward_input(self):
        try:
            with self.connection.makefile('rb') as incoming:
                while encoded := incoming.readline(MAXIMUM_FRAME + 1):
                    observed = self.capture('client-to-server', encoded)
                    frame = json.loads(encoded)
                    self.charge(frame, observed)
                    self.process.stdin.write(encoded)
                    self.process.stdin.flush()
        except Exception as error:
            self.fail(error)
        # Client EOF must not race the separately recorded execution-owner
        # drain: keep the original server input open until owned cleanup.

    def forward_output(self):
        try:
            while encoded := self.process.stdout.readline(MAXIMUM_FRAME + 1):
                self.capture('server-to-client', encoded)
                self.connection.sendall(encoded)
        except Exception as error:
            self.fail(error)

    def begin(self, delivery):
        with self.lock:
            if self.budget is None:
                self.budget = Budget(delivery['monotonic'])
                for frame, observed in self.initialization:
                    if observed >= self.budget.started:
                        self.budget.observe(frame, observed)
                self.initialization.clear()

    def charge(self, frame, observed):
        marker = self.directory / 'brief-delivered.json'
        if self.budget is None and not marker.exists() and frame.get('method') not in {
                'server/discover', 'initialize', 'notifications/initialized', 'tools/list',
                'prompts/list', 'resources/list', 'resources/templates/list'}:
            deadline = time.monotonic() + 10
            while not marker.exists():
                if self.failure is not None or time.monotonic() > deadline:
                    raise RuntimeError('non-initialization request without brief-delivery receipt')
                time.sleep(0.01)
        if self.budget is None and marker.exists():
            self.begin(json.loads(marker.read_text()))
        with self.lock:
            if self.budget is not None:
                self.budget.observe(frame, observed)
            else:
                if len(self.initialization) >= 32:
                    raise RuntimeError('automatic client initialization exceeds bound')
                self.initialization.append((frame, observed))

    def run(self, ready):
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener, \
             (self.directory / 'server-stderr.log').open('wb') as diagnostic:
            listener.bind('\0' + self.address)
            listener.listen(1)
            listener.settimeout(120)
            self.process = subprocess.Popen(self.arguments, env=self.environment,
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=diagnostic,
                start_new_session=True)
            try:
                ready(self.address)
                self.connection, _ = listener.accept()
                self.workers = [threading.Thread(target=function, daemon=True) for function in
                                (self.forward_input, self.forward_output)]
                for worker in self.workers:
                    worker.start()
                marker = self.directory / 'brief-delivered.json'
                deadline = time.monotonic() + 120
                while not marker.exists():
                    if self.failure is not None:
                        raise RuntimeError(self.failure)
                    if time.monotonic() > deadline:
                        raise RuntimeError('client omitted its original brief-delivery receipt')
                    time.sleep(0.05)
                delivery = json.loads(marker.read_text())
                self.begin(delivery)
                result = self.directory / 'client-result.json'
                while not result.exists() and self.failure is None:
                    # Receipt transport gets five seconds; the original final
                    # timestamp still must satisfy the unchanged model budget.
                    if time.monotonic() - self.budget.started > 905:
                        raise RuntimeError('client omitted its timely final-answer receipt')
                    time.sleep(0.05)
                if self.failure is not None:
                    raise RuntimeError(self.failure)
                final = json.loads(result.read_text())
                elapsed = self.budget.finish(final['monotonic'])
                if final.get('has_final_answer') is not True:
                    raise RuntimeError('genuine client did not produce a final model answer')
                return dict(delivery=delivery, final=final, elapsed_seconds=elapsed,
                            model_call_count=self.budget.calls, counting='all-post-delivery-MCP-requests')
            except Exception as error:
                self.fail(error)
                raise

    def close_transport(self):
        """Call after original observations and the owner's confirmed drain."""
        if self.connection is not None:
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.connection.close()
        for worker in self.workers:
            worker.join(timeout=2)
            if worker.is_alive():
                raise RuntimeError('original MCP relay did not retire')
        if self.process is not None and not self.process.stdin.closed:
            self.process.stdin.close()
