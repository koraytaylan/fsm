---
id: stream-resumability
title: "Stream Resumability"
workstream: "0070"
kind: task
depends_on:
  - sse-stream-endpoint
gated: false
touches:
  - crates/fsm-cli/src/http/sse.rs
  - crates/fsm-cli/tests/http_resume.rs
status: planned
merged_as: ""
---
# Stream Resumability

A dropped connection must not silently become a gap in what a client believes it was told, so the one behaviour this task refuses is resuming with a hole.

**Steps:**

1. In `crates/fsm-cli/src/http/sse.rs`, maintain a per-session replay buffer of emitted events bounded by **256 events or 1 MiB, whichever is reached first**, evicting oldest-first.
2. On a GET carrying `Last-Event-ID`, replay every buffered event **after** that id, in order, before resuming live delivery. The client's next event id continues the same monotonic sequence.
3. When the requested id has already been **evicted**, respond `409` with a plain-text body telling the client to re-initialize. Silently resuming from the oldest retained event would hand the client a gap it cannot detect, which is the one outcome worse than refusing.
4. When the requested id is **unknown** — never issued on this session — respond `400`. That is a client error rather than an expiry, and distinguishing them tells a client which of the two to fix.
5. Replay from the buffer without re-deriving anything from the journal. The buffer holds the exact bytes already sent; regenerating them could produce different content if the store moved on, and a replayed event must be the event that was originally sent.
6. Account the buffer's byte size as it grows and shrinks so the 1 MiB bound is real rather than nominal, and document that a large `instance_history` notification payload will evict more aggressively than 256 small ones.
7. Free the buffer on session expiry and on `DELETE`, so a disconnected client's buffer does not outlive its session.

**Tests:**

- `crates/fsm-cli/tests/http_resume.rs`: disconnect after event 5, reconnect with `Last-Event-ID: 5`, and receive events 6 onward with no duplicate and no gap.
- Reconnecting with the id of the most recent event replays nothing and resumes live.
- Events emitted **while** disconnected are buffered and delivered on reconnect.
- An evicted id returns `409` with the re-initialize message; an id never issued returns `400`.
- The buffer evicts at 256 events: emit 300, and the oldest 44 are gone while the newest 256 replay.
- The buffer evicts at 1 MiB: emit fewer than 256 large events totalling over 1 MiB and confirm eviction by size.
- Replayed bytes are **identical** to the bytes originally sent, including ids — assert against a recording of the first delivery.
- Event ids continue monotonically across a resume rather than restarting.
- The buffer is freed on `DELETE` and on expiry — assert memory accounting returns to zero.

- **Done when:** `cargo test -p fsm-cli --test http_resume` passes every case above, an evicted id is refused rather than silently resumed, replayed bytes are identical to the originals, both bounds are enforced, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
