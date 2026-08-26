---
id: http-request-parsing
title: "HTTP Request Parsing"
workstream: "0069"
kind: task
depends_on:
  - http-server-core
gated: false
touches:
  - crates/fsm-cli/src/http/request.rs
  - crates/fsm-cli/tests/http_request_parse.rs
  - fuzz/fuzz_targets/http_request.rs
  - fuzz/Cargo.toml
status: planned
merged_as: ""
---
# HTTP Request Parsing

A hand-rolled parser on a socket is where a zero-dependency project earns or loses its safety claim, so every length is checked before it is trusted and every refusal has a documented status code.

**Steps:**

1. In `crates/fsm-cli/src/http/request.rs`, parse the request line and headers with each bound as a named constant and its refusal code: request line 8 KiB → `414`; header count 64 → `431`; single header 8 KiB → `431`; total headers 32 KiB → `431`; body 16 MiB, matching `JsonLimits::DEFAULT.max_bytes` → `413`; read idle 30 s → `408`.
2. Accept **only** `Content-Length` bodies. A `Transfer-Encoding: chunked` request is `411 Length Required` with a message naming the limitation. Read exactly `Content-Length` bytes and no more; a body shorter than declared times out as `408`, and a longer one is not read.
3. Refuse a request carrying **both** `Content-Length` and `Transfer-Encoding` outright as a request-smuggling shape — `400`, never reconciled. Reconciling those two headers is how smuggling bugs happen, and there is no legitimate client that sends both here.
4. Refuse obsolete line folding (a header continuation line beginning with space or tab) with `400`, per RFC 9112's recommendation, rather than un-folding it.
5. Compare header names ASCII-case-insensitively; preserve values verbatim; refuse a duplicate `Content-Length` or `Host`.
6. Refuse any byte that is not permitted in a header name or value rather than sanitising it. A parser that repairs input is a parser two parties can disagree about.
7. Never `unwrap` on external input, never index without a bounds check, never allocate a buffer from a client-supplied length before checking it against the cap. `CONTRIBUTING.md`'s write-path safety rules apply here with more force than anywhere else in the workspace.
8. Add `fuzz/fuzz_targets/http_request.rs` over the parser, registered in `fuzz/Cargo.toml` beside the existing `jsonrpc_loop` target. This is the workspace's first network-facing parser and it gets the treatment the others got.

**Tests:**

- `crates/fsm-cli/tests/http_request_parse.rs`: a well-formed POST with a `Content-Length` body parses into method, path, headers, and body.
- Each of the six bounds produces its documented status code, driven by a raw byte fixture rather than a constructed struct.
- `Transfer-Encoding: chunked` produces `411`.
- Both `Content-Length` and `Transfer-Encoding` produces `400`.
- An obsolete folded header produces `400`.
- Duplicate `Content-Length` and duplicate `Host` each produce `400`.
- A body shorter than `Content-Length` times out as `408` without blocking forever.
- Invalid bytes in a header name and in a header value each produce `400` and are not sanitised.
- Header name matching is case-insensitive; values are preserved byte-for-byte including internal spacing.
- The fuzz target builds and runs its seed corpus through `crates/fsm-cli/tests/isolated_fuzz_targets.rs`.
- **No input panics:** a table-driven case list of at least 40 malformed requests, each asserted to produce a status code rather than an unwind.

- **Done when:** `cargo test -p fsm-cli --test http_request_parse --test isolated_fuzz_targets` passes, every bound and every malformed shape produces its documented code with no panic, the fuzz target is registered with a seed corpus, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
