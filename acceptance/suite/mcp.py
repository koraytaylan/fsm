"""A small MCP client: newline-delimited JSON-RPC over stdio, and Streamable
HTTP with server-sent events.

Deliberately written against the *protocol*, not against this repository's
internals — no `fsm` code is imported, nothing is shared with the engine's own
test harness. A suite that drove the server through the server's own helpers
would agree with it by construction, which is the failure the manual host
checks existed to catch.

Standard library only, to match the workspace's zero-dependency rule.
"""

from __future__ import annotations

import http.client
import json
import subprocess
import threading
import queue
import socket
import time
import urllib.parse

PROTOCOL_VERSION = "2025-06-18"
CLIENT_INFO = {"name": "fsm-acceptance", "version": "1"}
MAX_FRAME = 1_048_576
MAX_QUEUED_FRAMES = 128


class McpError(RuntimeError):
    """A JSON-RPC error the server returned, or a protocol violation."""


class FrameReader:
    """One bounded reader per stream, retained across calls and timeouts."""

    def __init__(self, frames):
        self.queue = queue.Queue(maxsize=MAX_QUEUED_FRAMES)
        self.stopped = threading.Event()
        self.error = None

        def pump():
            try:
                for frame in frames():
                    if self.stopped.is_set():
                        break
                    if not isinstance(frame, dict):
                        raise McpError("the server sent a non-object frame")
                    try:
                        self.queue.put_nowait(frame)
                    except queue.Full:
                        raise McpError("the client notification queue overflowed") from None
            except Exception as error:
                if not self.stopped.is_set():
                    self.error = McpError(str(error))
            finally:
                self.stopped.set()

        self.worker = threading.Thread(target=pump, daemon=True)
        self.worker.start()

    def receive(self, timeout):
        deadline = time.monotonic() + timeout
        while True:
            if self.error is not None:
                raise self.error
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise queue.Empty
            try:
                return self.queue.get(timeout=min(remaining, 0.05))
            except queue.Empty:
                if self.error is not None:
                    raise self.error
                if self.stopped.is_set() and self.queue.empty():
                    raise McpError("the server closed its output") from None

    def join(self):
        self.stopped.set()
        self.worker.join(timeout=2)
        if self.worker.is_alive():
            raise McpError("the client stream reader did not retire")


class StdioClient:
    """One `fsm serve` child, spoken to over its stdin and stdout."""

    def __init__(self, argv: list[str], env: dict[str, str] | None = None) -> None:
        self.argv = argv
        self._next_id = 0
        self._notifications: list[dict] = []
        self.process = subprocess.Popen(
            argv,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=env,
        )
        self._stderr = bytearray()

        def errors():
            while chunk := self.process.stderr.buffer.read1(4096):
                self._stderr.extend(chunk[:max(0, 65_536 - len(self._stderr))])

        self._error_worker = threading.Thread(target=errors, daemon=True)
        self._error_worker.start()

        def frames():
            while line := self.process.stdout.readline(MAX_FRAME + 1):
                if len(line.encode("utf-8")) > MAX_FRAME:
                    raise McpError("the server frame exceeded the client bound")
                if line.strip():
                    yield json.loads(line)

        self._reader = FrameReader(frames)

    def __enter__(self) -> "StdioClient":
        return self

    def __exit__(self, *_exc) -> None:
        self.close()

    def close(self) -> None:
        if self.process.poll() is None:
            try:
                self.process.stdin.close()
            except OSError:
                pass
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
        self._reader.join()
        self._error_worker.join(timeout=2)
        for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
            if not pipe.closed:
                pipe.close()
        if self._error_worker.is_alive():
            raise McpError("the client diagnostic reader did not retire")

    def _send(self, message: dict) -> None:
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def _read_until_id(self, want: int, timeout: float = 30) -> dict:
        """Read frames until the answer to `want` arrives.

        Notifications are kept rather than discarded: a test that asserts one
        arrived needs them, and a client that silently dropped anything it did
        not ask for would be a worse client than a real host.
        """
        deadline = time.monotonic() + timeout
        while True:
            try:
                frame = self._reader.receive(deadline - time.monotonic())
            except queue.Empty:
                raise McpError(f"no reply to {want} arrived within {timeout}s") from None
            except McpError as error:
                detail = bytes(self._stderr).decode("utf-8", errors="replace")
                raise McpError(f"{error}; stderr:\n{detail}") from None
            if "id" not in frame:
                if len(self._notifications) >= MAX_QUEUED_FRAMES:
                    raise McpError("the retained notification bound was exceeded")
                self._notifications.append(frame)
                continue
            if frame["id"] != want:
                raise McpError(f"expected a reply to {want}, got {frame['id']}")
            return frame

    def request(self, method: str, params: dict | None = None, *, timeout: float = 30) -> dict:
        self._next_id += 1
        message = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)
        frame = self._read_until_id(self._next_id, timeout)
        if "error" in frame:
            raise McpError(f"{method}: {json.dumps(frame['error'])}")
        return frame.get("result", {})

    def notify(self, method: str, params: dict | None = None) -> None:
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)

    @property
    def notifications(self) -> list[dict]:
        return list(self._notifications)

    def drain(self, timeout: float = 0.5) -> list[dict]:
        """Collect whatever the server pushed without being asked."""
        collected: list[dict] = []
        deadline = time.monotonic() + timeout
        try:
            while True:
                frame = self._reader.receive(deadline - time.monotonic())
                if "id" in frame:
                    raise McpError("an unsolicited response arrived while draining notifications")
                if len(self._notifications) + len(collected) >= MAX_QUEUED_FRAMES:
                    raise McpError("the retained notification bound was exceeded")
                collected.append(frame)
        except queue.Empty:
            pass
        self._notifications.extend(collected)
        return collected

    def initialize(self, version: str = PROTOCOL_VERSION) -> dict:
        result = self.request(
            "initialize",
            {
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": CLIENT_INFO,
            },
        )
        self.notify("notifications/initialized")
        return result

    def tools(self) -> list[dict]:
        return self.request("tools/list").get("tools", [])

    def call(self, name: str, arguments: dict | None = None) -> dict:
        """Call a tool and return its structured result.

        A tool that reports `isError` raises: an acceptance run that read a
        refusal as a success would pass while the workflow it claims to drive
        went nowhere.
        """
        result = self.request(
            "tools/call", {"name": name, "arguments": arguments or {}}
        )
        if result.get("isError"):
            raise McpError(f"{name} refused: {json.dumps(result)}")
        return result

    def structured(self, name: str, arguments: dict | None = None) -> dict:
        result = self.call(name, arguments)
        if "structuredContent" in result:
            return result["structuredContent"]
        for block in result.get("content", []):
            if block.get("type") == "text":
                try:
                    return json.loads(block["text"])
                except json.JSONDecodeError:
                    continue
        raise McpError(f"{name} returned no structured result: {json.dumps(result)}")

    def try_call(self, name: str, arguments: dict | None = None) -> dict:
        """Call a tool that is *expected* to refuse, and return the refusal."""
        return self.request("tools/call", {"name": name, "arguments": arguments or {}})


class HttpClient:
    """A Streamable HTTP MCP client: POST for requests, GET for the event stream.

    The transport item this replaces asked for "a real MCP client that has to
    like what it sees", specifically because a conformance suite driving a
    socket is not the same thing. This speaks the transport the way a host
    does — session header, `text/event-stream` accept, a stream held open on
    its own connection while the session goes on being used elsewhere.
    """

    def __init__(self, host: str, port: int, path: str = "/mcp") -> None:
        self.host = host
        self.port = port
        self.path = path
        self.session: str | None = None
        self._next_id = 0
        self._stream: http.client.HTTPResponse | None = None
        self._stream_connection: http.client.HTTPConnection | None = None
        self._stream_reader = None
        self._stream_socket = None
        self._notifications: list[dict] = []

    def __enter__(self) -> "HttpClient":
        return self

    def __exit__(self, *_exc) -> None:
        self.close()

    def _connection(self, timeout: float = 30) -> http.client.HTTPConnection:
        return http.client.HTTPConnection(self.host, self.port, timeout=timeout)

    def _headers(self, streaming: bool = False) -> dict[str, str]:
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream"
            if streaming
            else "application/json",
            "Origin": f"http://{self.host}:{self.port}",
        }
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        return headers

    def post(self, message: dict, *, timeout: float = 30) -> tuple[int, dict[str, str], str]:
        connection = self._connection(timeout)
        try:
            connection.request(
                "POST", self.path, json.dumps(message), self._headers()
            )
            response = connection.getresponse()
            encoded = response.read(MAX_FRAME + 1)
            if len(encoded) > MAX_FRAME:
                raise McpError("the HTTP response exceeded the client frame bound")
            body = encoded.decode()
            headers = {k.lower(): v for k, v in response.getheaders()}
            if "mcp-session-id" in headers and not self.session:
                self.session = headers["mcp-session-id"]
            return response.status, headers, body
        finally:
            connection.close()

    def request(self, method: str, params: dict | None = None, *, timeout: float = 30) -> dict:
        self._next_id += 1
        message = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            message["params"] = params
        status, _headers, body = self.post(message, timeout=timeout)
        if status >= 400:
            raise McpError(f"{method}: HTTP {status}: {body}")
        frames = _json_frames(body)
        frame = None
        for received in frames:
            if "id" not in received and isinstance(received.get("method"), str):
                if len(self._notifications) >= MAX_QUEUED_FRAMES:
                    raise McpError("the HTTP notification history exceeded its frame bound")
                self._notifications.append(received)
            elif frame is None and received.get("id") == self._next_id:
                frame = received
            else:
                raise McpError(f"expected a reply to {self._next_id}, got {received.get('id')}")
        if frame is None:
            raise McpError(f"{method}: no JSON in the answer: {body!r}")
        if "error" in frame:
            raise McpError(f"{method}: {json.dumps(frame['error'])}")
        return frame.get("result", {})

    def notify(self, method: str, params: dict | None = None) -> int:
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        status, _headers, _body = self.post(message)
        return status

    def initialize(self, version: str = PROTOCOL_VERSION) -> dict:
        result = self.request(
            "initialize",
            {
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": CLIENT_INFO,
            },
        )
        self.notify("notifications/initialized")
        return result

    def call(self, name: str, arguments: dict | None = None) -> dict:
        result = self.request(
            "tools/call", {"name": name, "arguments": arguments or {}}
        )
        if result.get("isError"):
            raise McpError(f"{name} refused: {json.dumps(result)}")
        return result

    def structured(self, name: str, arguments: dict | None = None) -> dict:
        """A tool's structured result, preferring `structuredContent`.

        The `content` blocks are a *rendering* for a human reader; a client
        that parsed those as JSON would be reading the wrong half.
        """
        result = self.call(name, arguments)
        if "structuredContent" in result:
            return result["structuredContent"]
        for block in result.get("content", []):
            if block.get("type") == "text":
                try:
                    return json.loads(block["text"])
                except json.JSONDecodeError:
                    continue
        raise McpError(f"{name} returned no structured result: {json.dumps(result)}")

    def open_stream(self) -> None:
        """Hold a GET open for server-sent events, on its own connection."""
        if self._stream_reader is not None:
            raise McpError("a client event stream is already open")
        self._stream_connection = self._connection()
        self._stream_connection.request("GET", self.path, headers=self._headers(True))
        self._stream_socket = self._stream_connection.sock
        self._stream = self._stream_connection.getresponse()
        if self._stream.status != 200:
            status = self._stream.status
            self.close()
            raise McpError(f"the event stream was refused: HTTP {status}")

        def frames():
            data = []
            size = 0
            while raw := self._stream.readline(MAX_FRAME + 1):
                if len(raw) > MAX_FRAME:
                    raise McpError("the event-stream line exceeded the client bound")
                line = raw.decode().rstrip("\r\n")
                if line == "":
                    if data:
                        yield json.loads("\n".join(data))
                    data, size = [], 0
                elif line.startswith("data:"):
                    size += len(raw)
                    if size > MAX_FRAME:
                        raise McpError("the event-stream frame exceeded the client bound")
                    data.append(line[5:].lstrip())
            if data:
                raise McpError("the event stream ended inside a frame")

        self._stream_reader = FrameReader(frames)

    def await_event(self, timeout: float = 20.0) -> dict:
        """Read one server-sent event off the open stream.

        Times out rather than blocking forever: a notification that never
        arrives is the failure this exists to catch, and a hung suite reports
        it as nothing at all.
        """
        if self._stream is None:
            raise McpError("no stream is open")
        try:
            return self._stream_reader.receive(timeout)
        except queue.Empty:
            raise McpError(f"no server-sent event arrived within {timeout}s") from None

    @property
    def notifications(self) -> list[dict]:
        return list(self._notifications)

    def drain(self, timeout: float = 0.5) -> list[dict]:
        """Observe pushed events without a protocol request or workflow poll."""
        if self._stream_reader is None:
            raise McpError("no stream is open")
        collected = []
        deadline = time.monotonic() + timeout
        try:
            while True:
                frame = self._stream_reader.receive(deadline - time.monotonic())
                if "id" in frame:
                    raise McpError("an unsolicited response arrived while draining notifications")
                if len(self._notifications) + len(collected) >= MAX_QUEUED_FRAMES:
                    raise McpError("the retained notification bound was exceeded")
                collected.append(frame)
        except queue.Empty:
            pass
        self._notifications.extend(collected)
        return collected

    def delete_session(self) -> int:
        connection = self._connection()
        try:
            connection.request("DELETE", self.path, headers=self._headers())
            response = connection.getresponse()
            response.read()
            return response.status
        finally:
            connection.close()

    def close(self) -> None:
        if self._stream_connection is not None:
            if self._stream_reader is not None:
                self._stream_reader.stopped.set()
            if self._stream_socket is not None:
                try:
                    self._stream_socket.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            try:
                self._stream_connection.close()
            except OSError:
                pass
            if self._stream_reader is not None:
                self._stream_reader.join()
            if self._stream is not None:
                self._stream.close()
            self._stream_connection = None
            self._stream = None
            self._stream_reader = None
            self._stream_socket = None


def _json_frames(body: str) -> list[dict]:
    """Bounded POST bodies may carry notifications before their SSE response."""
    body = body.strip()
    if not body:
        return []
    if body.startswith("{"):
        try:
            frame = json.loads(body)
            return [frame] if isinstance(frame, dict) else []
        except json.JSONDecodeError:
            pass
    frames = []
    for line in body.splitlines():
        line = line.strip()
        if line.startswith("data:"):
            line = line[5:].strip()
        if line.startswith("{"):
            try:
                frame = json.loads(line)
                if isinstance(frame, dict):
                    frames.append(frame)
                    if len(frames) > MAX_QUEUED_FRAMES:
                        raise McpError("the HTTP response exceeded its frame-count bound")
            except json.JSONDecodeError:
                continue
    return frames


def url_for(host: str, port: int, path: str) -> str:
    return urllib.parse.urlunparse(("http", f"{host}:{port}", path, "", "", ""))
