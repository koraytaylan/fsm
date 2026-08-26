---
id: 0015
title: "Transport And Multi-Client"
status: planned
---
# Scope — Plan 0015

> One user, one machine, one process. That is the whole deployment story today.

## Why this plan

`fsm serve` speaks stdio and nothing else. `run_with_mode` locks `stdin`/`stdout` and runs one session for the life of the process. Three consequences follow, and they compound:

- **No shared store.** A workflow store cannot be reached from a second machine, from a browser-based client, or by a colleague. The engine's durable, auditable journal — the artefact whose entire value is being a shared source of truth about what happened — is reachable only by processes that can be spawned as children on one host.
- **No second client.** Two stdio clients pointed at one data directory each spawn their own `fsm serve`, and the second one **dies at startup**: `serve_dir_with` fails to take the writer lock, writes one line to stderr, and returns `Err`. The user sees a server that never appeared. Plan 0008 documented the paired workaround — run the executor plus a read-only serve — but that is a workaround for a two-process case, not an answer for two clients.
- **No authentication and nothing that would need it.** There is nothing to authenticate to, which is fine while the only transport is a subprocess pipe and becomes the first question anybody asks the moment there is a socket.

This plan adds the second transport the MCP specification defines — **Streamable HTTP** — hand-rolled over `std::net`, because this workspace has zero dependencies and intends to keep them. That constraint shapes the security posture rather than being defeated by it: there is no TLS implementation here and there will not be one, so the server **binds loopback by default**, validates `Origin` on every request, and authenticates with a static bearer token. Exposing it beyond loopback is an operator decision made behind a reverse proxy that terminates TLS, and the documentation says exactly that rather than implying a security property the code does not have.

The plan's second half is the one that pays off immediately even for a single user: once requests arrive over a socket, **one process serves every client**, and that process is the single writer. The single-writer constraint stops being a limitation clients trip over and becomes the serialization point they share. Two clients, a browser and a terminal, or a person and a scheduled job can all work against one store because they are all talking to the thing that holds the lock.

And the failure that has nothing to do with HTTP gets fixed alongside it: a `serve` that cannot take the writer lock should **retry, then degrade to read-only and say so** — reusing plan 0014's degraded machinery — instead of exiting before the client connects.

## In scope

- **0069 — An HTTP/1.1 server.** A blocking, thread-per-connection server over `std::net::TcpListener`: request line and header parsing, `Content-Length` bodies, response writing including chunked-free streaming, and the bounds that keep a hand-rolled parser safe — header count, header size, body size, and read timeouts.
- **0070 — The Streamable HTTP binding.** The single MCP endpoint over POST and GET; session lifecycle with `Mcp-Session-Id` assigned at `initialize` and required afterwards; `MCP-Protocol-Version` negotiation on every subsequent request; server-sent events for server-initiated messages so plan 0012's notifications and plan 0013's elicitation work over HTTP exactly as they do over stdio; `DELETE` to terminate; and bounded stream resumability via `Last-Event-ID`.
- **0071 — Security.** Loopback-by-default binding with an explicit opt-out flag that names the risk; mandatory `Origin` validation against an allow-list; a static bearer token compared in constant time, sourced from a file or the environment and never from a command-line argument; and the request limits that make an unauthenticated stranger's traffic cheap to refuse.
- **0072 — One process, many clients.** The serialized writer that lets concurrent sessions share one `Store` without interleaving; per-session state — subscriptions, logging levels, elicitation counters — kept apart; and the lock-contention degradation that replaces exiting at startup.
- **0073 — Proof and docs.** A conformance suite driving the transport the way a real client does, including malformed and hostile input, and the documentation of the deployment shapes and the honest security boundary.

## Out of scope

TLS, in any form. There is no cryptographic transport library here and writing one would be indefensible; the answer is a reverse proxy and the docs say so. OAuth 2.1 resource-server behaviour — token introspection, discovery documents, dynamic client registration — which the specification recommends for HTTP servers and which is out of reach for a zero-dependency binary without TLS. The static-token deviation is documented as a deviation, with what would be required to close it. Multi-**process** or multi-host writers: the store is single-writer by design and this plan makes one process the writer for many clients, which is a different thing. HTTP/2, HTTP/3, chunked request bodies, and compression. Any change to engine semantics, the journal, or the tool surface.
