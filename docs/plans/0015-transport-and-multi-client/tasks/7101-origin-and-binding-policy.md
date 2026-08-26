---
id: origin-and-binding-policy
title: "Origin And Binding Policy"
workstream: "0071"
kind: task
depends_on:
  - http-server-core
gated: false
touches:
  - crates/fsm-cli/src/http/security.rs
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/tests/http_origin.rs
status: planned
merged_as: ""
---
# Origin And Binding Policy

There is no TLS in this binary and there will not be one, so the security posture has to be the one a zero-dependency program can actually deliver — and the defaults have to be the safe ones because most people never change a default.

**Steps:**

1. Create `crates/fsm-cli/src/http/security.rs` and add the flags in `crates/fsm-cli/src/args.rs`: `--http <addr>`, `--http-path <path>`, `--http-allow-remote`, and `--http-origin <origin>` (repeatable).
2. Bind `127.0.0.1` when `--http` names a port without a host. A non-loopback bind **requires** `--http-allow-remote`, and without it the server refuses to start with a message naming the reason.
3. Write `--http-allow-remote`'s help text as one plain sentence naming the risk: this binary has no TLS, so anything but loopback must sit behind a reverse proxy that terminates it. Help text is where an operator actually reads, and a flag that hides its consequence is a trap.
4. Validate `Origin` on **every** request — POST, GET, and DELETE alike — against an allow-list defaulting to loopback origins, extended by `--http-origin`. A missing `Origin` is `403`; an unlisted one is `403`. This is the DNS-rebinding defence the specification requires and it is **not** optional in any configuration, including loopback.
5. Compare origins exactly — scheme, host, and port — with no wildcards, no suffix matching, and no normalisation beyond ASCII-lowercasing the scheme and host. A wildcard origin allow-list is the flaw this check exists to prevent.
6. Run `Origin` validation **before** session lookup and before body parsing beyond the length check, so a rejected request costs almost nothing.
7. Emit a startup line naming the bind address, whether remote access is enabled, and the origin allow-list, so an operator can see the posture without reading the command line they typed.

**Tests:**

- `crates/fsm-cli/tests/http_origin.rs`: `--http 8080` binds loopback; `--http 0.0.0.0:8080` without `--http-allow-remote` refuses to start with the documented message; with the flag it binds.
- A request with a loopback `Origin` succeeds; one with a foreign `Origin` is `403`; one with **no** `Origin` is `403`.
- `--http-origin` extends the allow-list and the named origin then succeeds.
- Origin comparison is exact: a matching host on a different port is refused, and so is a suffix like `evil-localhost`.
- Scheme and host comparison is ASCII-case-insensitive; nothing else is normalised.
- Validation happens on GET and DELETE too, not only POST.
- A `403` for a bad origin does not create a session, does not read the body beyond the length check, and does not touch the store.
- The startup line names the bind address, the remote-access state, and the allow-list.
- The help text for `--http-allow-remote` contains the no-TLS sentence — assert it, so the warning cannot be trimmed later.

- **Done when:** `cargo test -p fsm-cli --test http_origin` passes every case above, loopback is the default and remote requires an explicit flag whose help names the risk, `Origin` is validated on every method with exact matching, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
