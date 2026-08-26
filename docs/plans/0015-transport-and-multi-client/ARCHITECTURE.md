# Architecture — Plan 0015

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Fixtures first: commit the raw request/response byte fixtures your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version. Blocking threads over `std::net`, never an async runtime.
7. When a golden fails, fix the code to match the fixture — unless it contradicts this document, the MCP `2025-06-18` specification, or RFC 9112.
8. **You are writing a network-facing parser with no dependencies.** Every loop reads bounded input, every length is checked before it is trusted, every allocation is capped, and every `unwrap` on external input is a defect. Read `CONTRIBUTING.md`'s safety rules for the write path before starting; they apply with more force here than anywhere else in the workspace.

## 0000 — Orientation: the five facts that shape this plan

- **The transport and the protocol are already separable.** `serve_session_with(store, clock, executor, refresh, input, output)` takes a `BufRead` and a `Write`. The MCP loop does not know it is talking to a pipe. That is why this plan adds a transport rather than rewriting a server.
- **Plan 0012 made the server able to speak first.** `Notifier` holds `Arc<Mutex<Box<dyn Write + Send>>>`; the change feed and the request path already share one output safely. Over HTTP that same abstraction writes into an SSE stream instead of stdout, and **nothing above the transport changes**. If this plan finds itself editing notification logic, it has gone wrong.
- **Plan 0013 made the server able to ask.** `request_and_await` reads inbound responses. Over HTTP a client's response arrives as a POST body rather than a stdin line, which is a routing difference and not a protocol one.
- **Plan 0014 built degraded mode.** An unopenable store already produces a server that starts and explains itself. Lock contention is a second reason a store may be unavailable, and §0072 reuses that machinery rather than inventing a parallel one.
- **The store is single-writer, and one process can hold it.** `ensure_writable()` plus the advisory `LOCK` file. The consequence people usually call a limitation is, for this plan, the design: one process holds the lock and every client talks to that process.

## 0069 — An HTTP/1.1 server

New module `crates/fsm-cli/src/http/`, declared in `crates/fsm-cli/src/lib.rs`.

**`server.rs` (task `6901`).** A blocking `TcpListener` accept loop spawning one thread per connection, matching the workspace's blocking posture. `HTTP/1.1` keep-alive is supported because an SSE stream requires holding a connection open anyway. A connection cap (`MAX_CONNECTIONS = 64`) refuses beyond it with `503` rather than spawning without bound; the accept loop never allocates per-connection state before the cap check.

**`request.rs` (task `6902`).** Hand-rolled parsing with every bound stated as a constant:

| Bound | Value | Refusal |
|---|---|---|
| request line | 8 KiB | `414` |
| header count | 64 | `431` |
| single header | 8 KiB | `431` |
| total headers | 32 KiB | `431` |
| body (`Content-Length`) | 16 MiB, matching `JsonLimits::DEFAULT.max_bytes` | `413` |
| read idle | 30 s | `408` |

Only `Content-Length` bodies are accepted; a `Transfer-Encoding: chunked` request is `411 Length Required` with a message saying so. Header names are compared ASCII-case-insensitively and header values are not un-folded — obsolete line folding is rejected, per RFC 9112's recommendation. A request with both `Content-Length` and `Transfer-Encoding` is refused outright as a request-smuggling shape, not reconciled.

**`response.rs` (task `6903`).** Status line, headers, and body writing, plus the streaming writer SSE needs: no `Content-Length`, `Content-Type: text/event-stream`, `Cache-Control: no-cache`, `Connection: keep-alive`, and an explicit flush per event. Every response carries `Content-Type` and, where a body is present, `Content-Length`; the streaming writer is the only exception and it is the reason chunked encoding is not needed on the response side either.

## 0070 — The Streamable HTTP binding

One endpoint path, default `/mcp`, configured by `fsm serve --http <addr> [--http-path <path>]`.

**Session ids, and the entropy problem this workspace actually has.** Rust's standard library has **no** random-number API, this workspace has zero dependencies, and `unsafe_code = "forbid"` rules out FFI to `getrandom` or `BCryptGenRandom`. "Draw 128 bits from the OS" is therefore not something this binary can simply do, and a plan that asserted it would fail at the first line of implementation. The construction is:

```
session_id = hex(sha256("fsm:session:1" || seed || counter || pid || nanos))[..32]
```

- `seed` is 32 bytes read from `/dev/urandom` when that path is readable — true OS entropy on Linux and macOS, the primary deployment targets — read **once** at server start, never per session.
- Where it is not readable (Windows), `seed` falls back to two `u64`s from `std::collections::hash_map::RandomState`, which std seeds from the OS per process, hashed together with the process id. This is process-seeded entropy, not a CSPRNG, and §0071's documentation says so plainly rather than implying a property the code does not have.
- `counter` is a monotonic per-process `u64`, `pid` is the process id, and `nanos` is `SystemTime::now()`; each alone is guessable, and each is there so that two sessions never collide even if `seed` were weak.
- The hash is the workspace's own `fsm_core::sha256`, with the domain separation every other hash here uses.

The security argument does not rest on this. The primary controls are the loopback default, mandatory `Origin` validation, and the bearer token — which §0071 makes **mandatory** for any non-loopback bind. A session id is defence in depth, and task `7302` documents both the construction and its limit alongside the OAuth deviation, in the same honest register.

**Sessions (task `7001`, `session.rs`).** `initialize` over POST creates a session and returns `Mcp-Session-Id` — 128 bits from the construction above, rendered as lowercase hex. Every subsequent request MUST carry it; one that does not is `400`, and one naming an unknown or expired session is `404`, which is the code the specification assigns so a client knows to re-initialize rather than retry. A session holds everything plan 0012 and 0013 made per-session: subscriptions, logging level, elicitation counter, cancellation set. An idle session expires after 30 minutes and `DELETE` terminates one explicitly.

**POST (task `7002`, `endpoint.rs`).** The body is exactly one JSON-RPC message — batching stays refused. A **notification or response** gets `202 Accepted` with no body. A **request** gets either `application/json` with the single response, or, when the handling may produce server-initiated messages before its response (an elicitation, a progress-reporting call), `text/event-stream` carrying them followed by the response. The choice is made by the handler, not guessed by the transport, and the default is JSON.

**GET (task `7003`, `sse.rs`).** Opens the server→client stream for a session: plan 0012's notifications and any server request that is not tied to a POST. `Accept: text/event-stream` is required. At most **one** GET stream per session; a second is `409`, because two streams would split notification ordering with nothing to reassemble it. A keep-alive comment line every 15 seconds stops an idle proxy closing the connection.

**Resumability (task `7004`).** Each SSE event carries a monotonic `id` per stream. A reconnecting GET carrying `Last-Event-ID` replays from a **bounded** buffer — the last 256 events or 1 MiB, whichever is smaller — and, when the requested id has already been evicted, responds `409` with a body telling the client to re-initialize rather than silently starting a gap. Silently resuming with a hole is the one behaviour worse than refusing.

## 0071 — Security

**Binding and Origin (task `7101`, `security.rs`).** `--http` binds `127.0.0.1` unless the address explicitly says otherwise, and a non-loopback bind requires `--http-allow-remote`, whose help text names the risk in one sentence: there is no TLS in this binary, and anything but loopback needs a reverse proxy that terminates it. `Origin` is validated on **every** request against an allow-list defaulting to loopback origins; a missing or unlisted `Origin` is `403`. This is the DNS-rebinding defence the specification requires and it is not optional in any configuration.

**Bearer token (task `7102`).** `--http-token-file <path>`, or `FSM_HTTP_TOKEN`, never a command-line argument — an argument is visible in `ps` to every user on the host. The token is compared in **constant time** over the full length, and a mismatch is `401` with a `WWW-Authenticate: Bearer` header and no detail about why. Authentication runs before session lookup, before body parsing beyond the length check, and before any store access. When no token is configured **and** the bind is loopback, authentication is disabled with a startup line saying so; a non-loopback bind without a token is a **startup refusal**, not a warning.

**Deliberate deviation, documented as one.** The specification recommends OAuth 2.1 resource-server behaviour for HTTP transports. This binary has zero dependencies and no TLS, and a partial OAuth implementation over cleartext would be worse than an honest static token. Task `7302` states the deviation, the reason, and what closing it would require, rather than leaving a reader to infer the security model from the flags.

**Limits (task `7103`).** The §0069 bounds enforced before authentication where they can be, so refusing a stranger's traffic is cheap; plus a per-connection request cap and the read timeout, so a slow-loris connection costs one thread for thirty seconds and no more.

## 0072 — One process, many clients

**Serialized writer (task `7201`, `writer.rs`).** One `Store` behind a `Mutex`, and every session's tool dispatch takes it for the duration of one call. The engine's operations are bounded by the evaluation budget and are short by construction, so a mutex is the right shape and a work queue would add latency and a second failure mode for no gain. Two rules make it safe rather than merely simple:

- **Read-only tools take the same lock.** A read that observed a half-applied macrostep would be a worse bug than a slow read, and there is no half-applied state to observe only because the lock is held across the whole call.
- **A long call must not be able to exist.** The one operation whose cost scales with the store is plan 0014's `journal_verify`/`journal_replay`; both read through `Store::open_read_only`, which takes **no** lock, so they never hold the writer at all. State that connection explicitly — it is why the mutex is affordable.

Per-session state stays per-session: subscriptions, logging level, cancellation set, and elicitation counter live in the session, never in the shared store handle. Two clients subscribing to the same instance each get their own notification on their own stream.

**Lock contention (task `7202`).** `serve` stops exiting when the writer lock is held. It retries with backoff for a bounded window (5 attempts over ~2 seconds), and then **starts read-only**, reporting the fact the way plan 0014 reports a degraded store: a startup line, an error-level log notification, an `instructions` note naming the state, and mutating tools refused with a message that says another process holds the writer and names the holder when it is discoverable. This is a behaviour change to stdio mode as well, and it is the right one: a server that explains itself beats a server that never appears.

## 0073 — Proof and docs

**Conformance (task `7301`).** `crates/fsm-cli/tests/http_conformance.rs` drives the transport over a real loopback socket the way a client does: full initialize-through-teardown over POST; a GET stream receiving a subscription notification; an elicitation round trip over HTTP; resumability with `Last-Event-ID` including the evicted case; and two concurrent sessions writing to one store, asserting both succeed and the journal is coherent.

Then the hostile half, which for a hand-rolled network parser is the more important one: oversized request line, too many headers, oversized single header, oversized body, `Content-Length` disagreeing with the body, both `Content-Length` and `Transfer-Encoding`, chunked encoding, obsolete line folding, a truncated request, a body that never arrives, a missing `Origin`, a foreign `Origin`, a bad token, a missing session id, an unknown session id, a second GET stream, and a `DELETE` for another session's id. **Every one must produce the documented status code and must not panic**; the suite asserts the server is still serving afterwards.

`fuzz/` gains an `http_request` target over the request parser, alongside the existing `jsonrpc_loop` target, and `crates/fsm-cli/tests/isolated_fuzz_targets.rs` gains its seed corpus — this is the first network-facing parser in the workspace and it gets the same treatment the others got.

**Docs (task `7302`).** `docs/EMBEDDING.md` gains a *Serving over HTTP* section: the two transports and when to choose each, the exact flags, the session lifecycle, the SSE stream, resumability limits, and the multi-client story. A **Security** subsection states the boundary without hedging: loopback by default, `Origin` always validated, static bearer token, **no TLS in this binary**, and remote exposure only behind a reverse proxy — plus the OAuth deviation and what closing it would require. `README.md` gains the HTTP setup snippet beside the existing stdio one and one honest non-claim about the security model. `docs/API-POLICY.md` records that the HTTP endpoint shape and headers are a compatibility surface under the same policy as the tool schemas.
