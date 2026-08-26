---
id: transport-docs
title: "Transport Docs"
workstream: "0073"
kind: task
depends_on:
  - transport-conformance-suite
gated: false
touches:
  - docs/EMBEDDING.md
  - docs/RELEASE.md
  - docs/API-POLICY.md
  - README.md
  - crates/fsm-cli/tests/transport_doc.rs
status: planned
merged_as: ""
---
# Transport Docs

The security section is the most important prose in this plan, because a reader who infers a security model from a flag list will infer one this binary does not have.

**Steps:**

1. Add a *Serving over HTTP* section to `docs/EMBEDDING.md`: the two transports and when to choose each, every flag, the session lifecycle including the `404`-means-re-initialize rule, the SSE stream and its one-per-session limit, resumability and its bounds, and the multi-client story with the serialized-writer explanation.
2. Write a **Security** subsection that states the boundary without hedging, as a list a reader cannot skim past: loopback by default; `Origin` validated on every request in every configuration; a static bearer token compared in constant time; **no TLS in this binary**; remote exposure only behind a reverse proxy that terminates TLS; and the token read from a file or the environment, never an argument, because arguments are visible in `ps`.
3. State the **session-id construction and its limit** in the same subsection: ids are `sha256` over a start-time seed, a counter, the pid, and the clock; the seed is `/dev/urandom` where readable and process-seeded `RandomState` entropy where it is not; this is not a CSPRNG, because std has no RNG and the workspace forbids both dependencies and `unsafe`. Say that the session id is defence in depth and that the primary controls are the loopback default, `Origin` validation, and the token.
4. State the **OAuth deviation** explicitly: the specification recommends OAuth 2.1 resource-server behaviour for HTTP transports; this binary has zero dependencies and no TLS, and a partial OAuth implementation over cleartext would be worse than an honest static token. Say what closing it would require — a TLS implementation or a mandated proxy, token introspection, and discovery metadata — so the gap is a documented decision rather than an omission.
5. Document the multi-client deployment shapes and how they relate to plan 0008's three run modes: one HTTP server as the writer with many clients; an executor plus a read-only HTTP server; and the contention degradation from `7202` with its message and remedy.
6. In `docs/API-POLICY.md`, record that the HTTP endpoint path, the headers, the session semantics, and the status codes are a **compatibility surface** under the same policy as the tool schemas — a client depends on them exactly as it depends on a tool's input schema.
7. Add the HTTP setup snippet to `README.md` beside the existing stdio ones, and one honest non-claim: the HTTP transport has no TLS and is loopback-first; exposing it to a network is a deployment decision requiring a proxy.
8. Add a **Manual acceptance** row to `docs/RELEASE.md`: connect a real MCP client over the HTTP transport and complete an initialize-through-teardown session including one SSE notification. The existing list already carries the Claude Desktop and MCP Inspector GUI passes for stdio; a second transport needs its own, because a conformance suite driving a socket is not the same as a client that has to like what it sees.
9. Create `crates/fsm-cli/tests/transport_doc.rs` pinning the docs to the code, in the style of the existing `executor_doc.rs`.

**Tests:**

- `crates/fsm-cli/tests/transport_doc.rs`: every HTTP-related flag in `args.rs` appears in the EMBEDDING HTTP section.
- Every status code the transport can return appears in the documented table — asserted against the constant list `6903` defines, so a new code cannot ship undocumented.
- A documentation test asserts EMBEDDING contains the exact phrase stating there is no TLS in this binary.
- A documentation test asserts EMBEDDING states the session-id construction is not a CSPRNG and names the reason, so the honest caveat cannot be trimmed.
- A documentation test asserts EMBEDDING contains the OAuth deviation paragraph, including what closing it would require.
- A documentation test asserts EMBEDDING states the token is never read from a command-line argument.
- `docs/API-POLICY.md` names the HTTP surface as a compatibility surface.
- `README.md` contains the HTTP snippet and the no-TLS non-claim.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose.
- `docs/RELEASE.md` names the HTTP-transport manual-acceptance pass.
- `cargo doc --workspace --no-deps` is warning-free under `RUSTDOCFLAGS=-D warnings`.

- **Done when:** EMBEDDING documents the transport, the session lifecycle, resumability, the multi-client shapes, and an unhedged security boundary including the OAuth deviation; API-POLICY names the HTTP compatibility surface; README carries the snippet and the non-claim; `cargo test -p fsm-cli --test transport_doc --test policy` passes; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
