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
import urllib.parse

PROTOCOL_VERSION = "2025-06-18"
CLIENT_INFO = {"name": "fsm-acceptance", "version": "1"}


class McpError(RuntimeError):
    """A JSON-RPC error the server returned, or a protocol violation."""


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

    def _send(self, message: dict) -> None:
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def _read_until_id(self, want: int) -> dict:
        """Read frames until the answer to `want` arrives.

        Notifications are kept rather than discarded: a test that asserts one
        arrived needs them, and a client that silently dropped anything it did
        not ask for would be a worse client than a real host.
        """
        while True:
            line = self.process.stdout.readline()
            if not line:
                stderr = self.process.stderr.read() if self.process.stderr else ""
                raise McpError(f"the server closed its output; stderr:\n{stderr}")
            line = line.strip()
            if not line:
                continue
            frame = json.loads(line)
            if "id" not in frame:
                self._notifications.append(frame)
                continue
            if frame["id"] != want:
                raise McpError(f"expected a reply to {want}, got {frame['id']}")
            return frame

    def request(self, method: str, params: dict | None = None) -> dict:
        self._next_id += 1
        message = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)
        frame = self._read_until_id(self._next_id)
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
        found: queue.Queue = queue.Queue()

        def pump() -> None:
            while True:
                line = self.process.stdout.readline()
                if not line:
                    return
                line = line.strip()
                if line:
                    found.put(json.loads(line))

        worker = threading.Thread(target=pump, daemon=True)
        worker.start()
        try:
            while True:
                collected.append(found.get(timeout=timeout))
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

    def __enter__(self) -> "HttpClient":
        return self

    def __exit__(self, *_exc) -> None:
        self.close()

    def _connection(self) -> http.client.HTTPConnection:
        return http.client.HTTPConnection(self.host, self.port, timeout=30)

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

    def post(self, message: dict) -> tuple[int, dict[str, str], str]:
        connection = self._connection()
        try:
            connection.request(
                "POST", self.path, json.dumps(message), self._headers()
            )
            response = connection.getresponse()
            body = response.read().decode()
            headers = {k.lower(): v for k, v in response.getheaders()}
            if "mcp-session-id" in headers and not self.session:
                self.session = headers["mcp-session-id"]
            return response.status, headers, body
        finally:
            connection.close()

    def request(self, method: str, params: dict | None = None) -> dict:
        self._next_id += 1
        message = {"jsonrpc": "2.0", "id": self._next_id, "method": method}
        if params is not None:
            message["params"] = params
        status, _headers, body = self.post(message)
        if status >= 400:
            raise McpError(f"{method}: HTTP {status}: {body}")
        frame = _first_json_frame(body)
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
        self._stream_connection = self._connection()
        self._stream_connection.request("GET", self.path, headers=self._headers(True))
        self._stream = self._stream_connection.getresponse()
        if self._stream.status != 200:
            raise McpError(f"the event stream was refused: HTTP {self._stream.status}")

    def await_event(self, timeout: float = 20.0) -> dict:
        """Read one server-sent event off the open stream.

        Times out rather than blocking forever: a notification that never
        arrives is the failure this exists to catch, and a hung suite reports
        it as nothing at all.
        """
        if self._stream is None:
            raise McpError("no stream is open")
        found: queue.Queue = queue.Queue()

        def pump() -> None:
            data: list[str] = []
            while True:
                raw = self._stream.readline()
                if not raw:
                    return
                line = raw.decode().rstrip("\r\n")
                if line == "":
                    if data:
                        found.put("\n".join(data))
                        data = []
                    continue
                if line.startswith("data:"):
                    data.append(line[5:].lstrip())

        threading.Thread(target=pump, daemon=True).start()
        try:
            return json.loads(found.get(timeout=timeout))
        except queue.Empty:
            raise McpError(f"no server-sent event arrived within {timeout}s") from None

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
            try:
                self._stream_connection.close()
            except OSError:
                pass
            self._stream_connection = None
            self._stream = None


def _first_json_frame(body: str) -> dict | None:
    """The first JSON object in a body, whether it is plain JSON or SSE."""
    body = body.strip()
    if not body:
        return None
    if body.startswith("{"):
        try:
            return json.loads(body)
        except json.JSONDecodeError:
            pass
    for line in body.splitlines():
        line = line.strip()
        if line.startswith("data:"):
            line = line[5:].strip()
        if line.startswith("{"):
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                continue
    return None


def url_for(host: str, port: int, path: str) -> str:
    return urllib.parse.urlunparse(("http", f"{host}:{port}", path, "", "", ""))
