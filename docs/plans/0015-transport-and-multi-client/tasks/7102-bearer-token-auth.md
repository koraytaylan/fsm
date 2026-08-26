---
id: bearer-token-auth
title: "Bearer Token Auth"
workstream: "0071"
kind: task
depends_on:
  - origin-and-binding-policy
gated: false
touches:
  - crates/fsm-cli/src/http/security.rs
  - crates/fsm-cli/tests/http_auth.rs
status: planned
merged_as: ""
---
# Bearer Token Auth

A static token is a deliberate deviation from the specification's OAuth recommendation, chosen because a partial OAuth implementation over cleartext would be worse than an honest one — so the parts that are implemented have to be right.

**Steps:**

1. In `crates/fsm-cli/src/http/security.rs`, read the token from `--http-token-file <path>` or the `FSM_HTTP_TOKEN` environment variable. **Never** from a command-line argument: an argument is visible in `ps` to every user on the host, and offering the option at all would invite its use.
2. Trim exactly one trailing newline from a token file and nothing else — a token is bytes, and stripping whitespace could silently accept a different secret than the one on disk. Refuse an empty token at startup.
3. Compare in **constant time** over the full length: accumulate a difference across every byte of both the expected and supplied values and compare once at the end, with no early return on the first mismatch and no length-based short-circuit before the accumulation.
4. Reply `401` with a `WWW-Authenticate: Bearer` header and **no detail** about why — not "wrong token", not "no token", not a length hint. A stranger learns that credentials are required.
5. Run authentication **after** `Origin` validation and **before** session lookup, body parsing beyond the length check, and any store access, so an unauthenticated request is cheap to refuse and can never reach the engine.
6. Disable authentication when no token is configured **and** the bind is loopback, with a startup line saying so plainly. A non-loopback bind with no token is a **startup refusal**, not a warning — a warning is something a person scrolls past.
7. Accept the header case-insensitively for the scheme (`Bearer`) and exactly for the token; reject a token containing whitespace or control bytes.

**Tests:**

- `crates/fsm-cli/tests/http_auth.rs`: with a token configured, a correct `Authorization: Bearer <token>` succeeds and a wrong one is `401` with `WWW-Authenticate: Bearer`.
- A missing `Authorization` header is `401` with the same body as a wrong token — assert byte equality, so the two cases are indistinguishable to a caller.
- Loopback with no token configured serves without authentication and logs the startup line.
- Non-loopback with no token **refuses to start**, with the documented message.
- A token file with a single trailing newline yields the token without it; a file with leading whitespace yields the token including it; an empty file refuses at startup.
- `FSM_HTTP_TOKEN` is honoured, and the token never appears in any log line, error body, or startup line — assert by scanning all output for the token value.
- The scheme is matched case-insensitively (`bearer`, `Bearer`, `BEARER`) and the token exactly.
- Ordering: a request with a bad `Origin` **and** a bad token returns `403`, proving origin validation runs first.
- A `401` creates no session and does not touch the store.
- The constant-time comparison has no early return — assert by reading the implementation in review, and pin the behaviour with a test over equal-length and differing-length inputs.

- **Done when:** `cargo test -p fsm-cli --test http_auth` passes every case above, the token is never accepted from an argument and never appears in output, comparison is constant time, a tokenless remote bind refuses to start, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
