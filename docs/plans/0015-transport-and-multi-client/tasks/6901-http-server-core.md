---
id: http-server-core
title: "HTTP Server Core"
workstream: "0069"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/http/mod.rs
  - crates/fsm-cli/src/http/server.rs
  - crates/fsm-cli/src/http/request.rs
  - crates/fsm-cli/src/http/response.rs
  - crates/fsm-cli/src/http/session.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/src/http/sse.rs
  - crates/fsm-cli/src/http/security.rs
  - crates/fsm-cli/src/http/writer.rs
  - crates/fsm-cli/src/lib.rs
  - crates/fsm-cli/tests/http_server.rs
status: planned
merged_as: ""
---
# HTTP Server Core

This is the first network-facing code in the workspace, so the accept loop is written to be boring: blocking threads, a hard connection cap checked before any allocation, and no state that a stranger can grow.

**Steps:**

1. Create `crates/fsm-cli/src/http/mod.rs` declaring **every** module this plan adds — `server`, `request`, `response`, `session`, `endpoint`, `sse`, `security`, `writer` — and declare `pub mod http;` in `crates/fsm-cli/src/lib.rs`.
2. **Create all eight module files here**, carrying only the public type and function skeletons the architecture names, with `unimplemented!()` bodies; `server.rs` alone lands complete in this task. A module cannot be declared without its file, and scaffolding them together is what lets each later task stay inside its own single file — the pattern plan 0008's `3601-crate-scaffold-and-skeleton` established.
3. Implement the blocking accept loop over `std::net::TcpListener` in `server.rs`, spawning one thread per connection. This matches the workspace's blocking posture; there is no async runtime here and there will not be one.
4. Enforce `MAX_CONNECTIONS = 64` **before** spawning or allocating per-connection state, refusing beyond it with a minimal `503` and closing. A cap checked after allocation is not a cap.
5. Support HTTP/1.1 keep-alive, since an SSE stream holds a connection open anyway, with a per-connection request cap so one client cannot pin a thread indefinitely by pipelining.
6. Set read and write timeouts on every accepted socket at accept time, so a connection that stops talking costs one thread for a bounded period and no more.
7. Expose `pub fn serve_http(addr: SocketAddr, handler: Arc<dyn Handler>, stop: Arc<AtomicBool>) -> io::Result<()>` with a stop flag, so tests can start and stop a server in-process without leaking threads across cases.
8. Handle a panicking connection thread without taking down the listener: catch it at the thread boundary, log it, close that connection. The existing panic hook aborts on genuine bugs, and this is the one place where isolating a connection is the right call — write the reasoning down.
9. Bind and report the actual port when the requested port is 0, so tests can bind ephemerally and connect deterministically.

**Tests:**

- `crates/fsm-cli/tests/http_server.rs`: the server binds an ephemeral port, reports it, and serves a trivial handler over a real loopback socket.
- Keep-alive: two sequential requests on one connection both succeed.
- The per-connection request cap closes the connection after N requests.
- The 65th concurrent connection receives `503` and the first 64 keep working.
- A socket that connects and sends nothing is closed after the read timeout, and the server keeps serving.
- A handler that panics closes only that connection; a subsequent request on a new connection succeeds.
- The stop flag ends the accept loop and every thread joins, with no leaked thread across 20 start/stop cycles.
- The listener is not left in a broken state by a client that resets the connection mid-response.
- All eight modules are declared and the crate compiles with the shells in place; `cargo doc --workspace --no-deps` is warning-free under `RUSTDOCFLAGS=-D warnings`.

- **Done when:** `cargo test -p fsm-cli --test http_server` passes every case above including the connection cap, the timeouts, and panic isolation, no thread leaks across repeated start/stop, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
