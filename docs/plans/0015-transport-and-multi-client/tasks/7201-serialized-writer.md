---
id: serialized-writer
title: "Serialized Writer"
workstream: "0072"
kind: task
depends_on:
  - sse-stream-endpoint
gated: false
touches:
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/src/http/writer.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/tests/http_multi_client.rs
status: done
merged_as: ""
---
# Serialized Writer

Single-writer stops being the limitation clients trip over and becomes the serialization point they share: one process holds the lock, and a mutex around calls that are short by construction is the whole concurrency design.

**Steps:**

1. Create `crates/fsm-cli/src/http/writer.rs` holding one `Store` behind a `Mutex`, with a guard type every session's dispatch takes for the duration of one tool call, **and route `endpoint.rs`'s call into the protocol handler through that guard**. `7002` wired the endpoint against a direct store handle; replacing that call site is part of this task, because a guard nothing calls guards nothing.
2. **Read-only tools take the same lock.** A read that observed a half-applied macrostep would be a worse bug than a slow read, and there is no half-applied state to observe *only because* the lock is held across the whole call. Write that reasoning down; it is the sentence that stops someone "optimising" reads out of the lock.
3. Justify the mutex rather than a work queue in the module doc: engine operations are bounded by the evaluation budget and short by construction, so a queue would add latency and a second failure mode for no gain.
4. Make the long calls provably not hold the lock: plan 0014's `journal_verify` and `journal_replay` read through `Store::open_read_only`, which takes no lock at all. State that connection explicitly — it is why the mutex is affordable, and a future task that routes them through the shared handle would break the argument.
5. Recover a poisoned mutex rather than propagating the panic: a panicking call must not make the store unreachable for every other session. The existing panic hook aborts on genuine engine bugs, so a poisoned lock here means the panic was already fatal or was isolated at the connection boundary by `6901`.
6. Keep per-session state out of this module entirely. Subscriptions, logging levels, cancellation sets, and elicitation counters live in `7001`'s `Session`; this module owns exactly one thing.
7. Preserve idempotency semantics across sessions: two sessions using the same `request_id` for the same content get the same replay, and for different content get the same refusal. That falls out of the store's existing keying, and the test pins it because it is the property that makes multi-client safe rather than merely possible.

**Tests:**

- `crates/fsm-cli/tests/http_multi_client.rs`: two concurrent sessions each create an instance and send events; both succeed and the journal is coherent and verifies clean.
- Serialization: 8 threads across 4 sessions each send 50 events to one instance; every event is applied exactly once, the seq sequence is gapless, and no record is interleaved.
- A read tool called concurrently with a write never observes a partially applied macrostep — assert configuration and context are always mutually consistent across many samples.
- `journal_verify` running concurrently with writes does **not** block them — assert writes complete during a long verify, proving the read-only path takes no lock.
- Cross-session idempotency: the same `request_id` with the same content replays in the second session; with different content it is refused.
- A panicking tool call in one session leaves the store usable for another session.
- Per-session state is untouched by this module: two sessions' subscriptions remain independent under concurrent load.
- Throughput sanity: the measured serialized rate is reported in the commit message against the append-latency numbers `crates/fsm-store/tests/append_latency.rs` already records.

- **Done when:** `cargo test -p fsm-cli --test http_multi_client` passes every case above, concurrent sessions produce a gapless coherent journal, verify does not block writes, a panicking call does not poison the store for others, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** One `Store` behind one mutex, and `endpoint.rs`'s call site replaced so every session's dispatch goes through it — a guard nothing calls guards nothing. The module doc carries the three arguments the design rests on: a mutex rather than a queue, because engine operations are bounded by the evaluation budget and a queue would add latency and a second failure mode for nothing; reads take the lock too, because the reason there is no half-applied macrostep to observe is that the lock is held across the whole call; and that is affordable precisely because the two calls whose cost grows with the journal read through `open_read_only` and never pass through here.

Each of those is a test rather than a claim. Eight threads across four sessions apply 400 events with a gapless journal that verifies clean; two hundred reads taken during a hundred writes always find the leaf and the counter agreeing; and writes complete *during* a `journal_verify`, which is the proof that the long reads take no lock.

Cross-session idempotency is pinned because it is what makes many clients safe rather than merely possible: the same key and content replays in another session, and the same key with different content is refused.
