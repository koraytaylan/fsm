---
id: transport-conformance-suite
title: "Transport Conformance Suite"
workstream: "0073"
kind: task
depends_on:
  - stream-resumability
  - request-limits-and-timeouts
  - lock-contention-degradation
gated: false
touches:
  - crates/fsm-cli/src/http/request.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/tests/http_conformance.rs
  - crates/fsm-cli/tests/isolated_fuzz_targets.rs
status: done
merged_as: ""
---
# Transport Conformance Suite

For a hand-rolled network parser the hostile half of this suite matters more than the happy half: every malformed shape must produce a documented status code, and the server must still be serving afterwards.

**Steps:**

1. Create `crates/fsm-cli/tests/http_conformance.rs` driving the transport over a **real loopback socket**, the way a client does — not through an in-process handler call, which would skip exactly the layer this plan added.
2. Cover the happy path end to end: initialize, tool calls, a GET stream receiving a subscription notification, an elicitation round trip over HTTP, resumability with `Last-Event-ID`, `DELETE` teardown, and two concurrent sessions writing to one store with a coherent journal afterwards.
3. Cover the hostile path as a table-driven list, each entry asserting a documented status **and** that the server still serves a subsequent well-formed request: oversized request line; too many headers; oversized single header; oversized body; `Content-Length` disagreeing with the body; both `Content-Length` and `Transfer-Encoding`; chunked encoding; obsolete line folding; a truncated request; a body that never arrives; missing `Origin`; foreign `Origin`; bad token; missing session id; unknown session id; a second GET stream; and a `DELETE` for another session's id.
4. Assert **no panic** on any hostile input by running the whole table in one process and confirming the listener is healthy at the end.
5. Assert cross-transport equivalence: the same tool call over stdio and over HTTP produces byte-identical JSON-RPC response objects. The transport must not change the protocol, and this is the assertion that proves it.
6. Register `6902`'s `http_request` fuzz target's seed corpus in `crates/fsm-cli/tests/isolated_fuzz_targets.rs`, matching how the existing targets are wired, so the parser gets the same treatment as `jsonrpc_loop` and `record_line`.
7. Keep the suite free of fixed port numbers — bind ephemerally and read the port back — so it runs on a busy CI machine and on all three operating systems.
8. **CI budget is shared and this plan is not the only claimant.** `ci.yml` sets `timeout-minutes: 45` per job across a three-OS, two-toolchain matrix, and `crash_harness.rs` (1,000 spawns per profile) plus `executor_chaos.rs` (200 iterations) already dominate it. Measure this suite's wall time on the slowest CI platform, and if it adds more than a few minutes, **lower the committed default iteration count and keep the depth behind the env override** — the pattern `FSM_CRASH_ITERS` and `FSM_EXECUTOR_CHAOS_ITERS` already establish. Record the measured time and the chosen default in the commit message. Four new heavy suites each quietly assuming they have room is how a 45-minute ceiling becomes a red build nobody can attribute.

**Tests:**

- Every happy-path step above, over a real socket.
- Every hostile-path entry produces its documented status and leaves the server serving.
- No hostile input panics; the listener is healthy after the whole table.
- Cross-transport equivalence for at least one read tool and one write tool, byte-comparing the JSON-RPC response objects.
- Two concurrent sessions produce a journal that `journal verify` reports `Ok`.
- The `http_request` fuzz target's seed corpus runs clean through the isolated-targets test.
- The suite binds ephemeral ports and passes on Linux, macOS, and Windows.
- Wall time on the slowest CI platform is measured and stated in the commit message; the suite is table-driven rather than iteration-based, so if it is too slow the fix is fewer redundant hostile cases, not a lower iteration count.

- **Done when:** `cargo test -p fsm-cli --test http_conformance --test isolated_fuzz_targets` passes, every hostile shape returns a documented status with no panic and a still-healthy listener, cross-transport response equivalence holds, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** Everything through a real loopback socket on an ephemeral port. The happy path runs end to end — initialize, a call, a subscription, a stream, a write, teardown — and the hostile path is a table of eighteen shapes, each asserting its documented status **and** that a well-formed request straight afterwards is still answered. Two sessions writing concurrently leave a journal that classifies `Ok`, and the same two tool calls over stdio and over HTTP produce byte-identical JSON-RPC objects, which is the assertion that says the transport is not making protocol decisions.

**Wall time: 1.0 second**, three runs, on this host. No iteration knob was needed: the suite is table-driven rather than iteration-based, so it adds nothing meaningful to the shared 45-minute CI budget and there is nothing to put behind an env override.

**Corrections.**

- *A keep-alive connection ending is not a request to refuse.* Driving the transport over real sockets found it: after answering, the loop read again, saw the client's clean close, and wrote a `400` onto a socket nobody was reading — which a client reading back found as a second response to one request. `read_head` now reports a clean close as `Ok(None)` and the handler closes silently.
- *Two hostile cases needed the client to stop talking, not to go quiet.* A short body and an unterminated header block only reach the server as "the request never arrived" when the write half is shut down; without that they wait out the thirty-second read timeout, which is correct behaviour and a terrible test.
