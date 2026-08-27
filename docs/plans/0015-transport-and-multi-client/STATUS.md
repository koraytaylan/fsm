# Plan 0015 — Transport And Multi-Client — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** a second transport — Streamable HTTP over a hand-rolled HTTP/1.1 server — so one process can serve many clients against one store, with sessions, server-sent events, an honest security boundary, and a `serve` that degrades instead of exiting when another process holds the writer.
- **Root cause:** the only transport is a subprocess pipe, so a durable auditable journal whose whole value is being a shared source of truth is reachable only by child processes on one host; a second stdio client dies at startup because the writer lock is held; and there is nothing to authenticate to, which stops being fine the moment there is a socket.
- **Approach:** keep the protocol loop untouched by adding a transport beneath it — `serve_session_with` already takes a `BufRead` and a `Write`, plan 0012's `Notifier` already multiplexes output, and plan 0013 already routes inbound responses, so HTTP is a routing layer rather than a rewrite; turn single-writer from a limitation clients trip over into the serialization point they share, with one process holding the lock and a mutex around calls that are short by construction; and choose a security posture the zero-dependency constraint can actually deliver — loopback by default, mandatory `Origin` validation, a constant-time static bearer token, no TLS and no pretence of it — documenting the OAuth deviation rather than half-implementing it.
- **Progress:** 7/14 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Two clients, or a person and a scheduled job, work against one store at the same time — and the server that cannot take the writer explains itself instead of disappearing.

_Task frontmatter is authoritative; this file is the roll-up._
