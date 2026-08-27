---
id: lock-contention-degradation
title: "Lock Contention Degradation"
workstream: "0072"
kind: task
depends_on:
  - serialized-writer
gated: false
touches:
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/tests/lock_degradation.rs
status: done
merged_as: ""
---
# Lock Contention Degradation

A second stdio client dies at startup today, with one line on a stderr nobody reads — and this fix helps stdio users immediately, whether or not they ever use HTTP.

**Steps:**

1. In `crates/fsm-cli/src/mcp/serve.rs`, stop exiting when `Store::open` fails because the writer lock is held. Retry with backoff for a bounded window — 5 attempts over roughly 2 seconds — since the executor takes and releases the writer per tick and a brief collision is expected rather than fatal.
2. After the window, **start read-only** and report it: a startup line, an error-level log notification through plan 0012, and an `instructions` note naming the state in the same style as the existing read-only and embedded notes.
3. Distinguish this from plan 0014's degraded mode in the message. A degraded store is *unhealthy*; a contended store is *healthy and busy*, and the remedies are completely different — one is "run repair after a human looks", the other is "stop the other writer or use the paired deployment".
4. Refuse mutating tools with a message that says another process holds the writer and names the holder when it is discoverable from the lock, reusing plan 0014's gating shape rather than a second mechanism.
5. Reuse plan 0014's `StoreSlot` rather than adding a third store state. Contended and unhealthy are two reasons for the same slot to be unavailable, and one enum with two reasons is easier to reason about than two enums.
6. Do **not** retry forever and do not upgrade later. A server that silently became a writer halfway through a session would surprise both writers; a client that wants the writer restarts.
7. Apply this to stdio and HTTP alike — it lives in the shared serve path, so the fix is one change benefiting both transports.

**Tests:**

- `crates/fsm-cli/tests/lock_degradation.rs`: with another process holding the writer, `serve` **starts** rather than exiting, and completes `initialize`.
- The retry window is respected: a lock released after one second yields a full writer session, not a read-only one.
- After the window, the session is read-only, the startup line says so, and an error-level notification carries the reason.
- `instructions` carries the contention note, distinct in wording from plan 0014's degraded note — assert both strings differ and each names its own remedy.
- Mutating tools are refused with a message naming the contention and, where discoverable, the holder.
- Read tools work normally throughout.
- The session does **not** upgrade to a writer when the lock is later released — assert it stays read-only for its lifetime.
- A healthy uncontended start produces a byte-identical transcript to the pre-change build.
- The same behaviour holds over HTTP: an HTTP server started against a contended store serves reads and refuses writes with the same message.
- The exit code is 0 on a clean disconnect from a contended session.

- **Done when:** `cargo test -p fsm-cli --test lock_degradation` passes every case above, a contended start degrades instead of exiting on both transports, the contention message is distinct from the unhealthy-store message, uncontended transcripts are unchanged, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** A writable open now retries five times over roughly two seconds — the executor takes and releases the writer once a tick, so a collision at startup is expected rather than fatal — and then starts **read-only** instead of exiting. A client used to see a server that never appeared; it now sees one that says what happened.

Contended and degraded travel through one slot with two reasons, because "unavailable" has two of them and only one is a fault. The words differ because the remedies are completely different: a degraded store is unhealthy and the note names `store_doctor`; a contended one is **healthy and busy** and the note says to stop the other writer or use the paired deployment. The suite asserts the two notes differ and that each names only its own remedy.

It does not upgrade later. A session that silently became the writer halfway through would surprise both writers, so a client that wants the writer restarts. And it lives in the shared serve path, so stdio and HTTP get it together.

**Corrections.**

- *`Unavailable` replaced two parameters rather than adding a third.* The reason and the detail belong together, and `serve_session_degraded` was already carrying them apart.
- *Both variants are boxed.* An enum is as large as its largest variant, and a `Store` beside an `ErrorObj` is a size difference clippy is right about.
