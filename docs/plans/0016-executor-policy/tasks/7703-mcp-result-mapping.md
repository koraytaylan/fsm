---
id: mcp-result-mapping
title: "MCP Result Mapping"
workstream: "0077"
kind: task
depends_on:
  - mcp-client-runner
  - exhaustion-and-dead-letters
gated: false
touches:
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/tests/mcp_result.rs
  - crates/fsm-execute/tests/support/mcp_stub.rs
status: done
merged_as: ""
---
# MCP Result Mapping

The store fingerprints an ack over its `result`, so a mapping that is not deterministic and bounded turns a retry into a `req/request_id_conflict` instead of a replay.

**Steps:**

1. In `crates/fsm-execute/src/mcp_client.rs`, implement the mapping exactly as the architecture table states: a result with `isError` absent or false → ack `ok` with `{"structured": <structuredContent or content>}`; `isError: true` → ack `failed` with `{"error": "mcp/tool_error", "structured": …}`; a JSON-RPC error → ack `failed` with `{"error": "mcp/rpc_error", "code", "message"}`; a timeout, spawn failure, or protocol violation → ack `failed` with the corresponding `exec/*` error.
2. Prefer `structuredContent` when the server provides it and fall back to `content` otherwise, so a tool that returns typed data does not have its result flattened to rendered text.
3. Make the mapped result **deterministic**: no timestamps, no pids, no durations, no elapsed values anywhere in it. The store fingerprints the ack over this object, and a re-issue whose content differs is a conflict rather than a replay — the exact failure plan 0008's `rid.rs` comment warns about.
4. Make it **bounded**: apply the same `ACK_OUTPUT_CAP` truncation and whole-value SHA-256 digest the process kind uses when a result exceeds the cap, so a chatty tool cannot push an ack past `MAX_PAYLOAD_BYTES` and fail to journal.
5. Truncate on a UTF-8 character boundary and render lossily where needed, reusing `BoundedBytes`'s existing conversion rather than writing a second one — an ack must never fail to journal because of what a tool returned.
6. In `crates/fsm-execute/src/service.rs`, classify an `mcp` handler's failure into the retry classes: `mcp/tool_error` and `mcp/rpc_error` are the `"mcp_error"` class, while timeout and spawn keep their existing classes, so `retry.on` behaves predictably across both kinds.
7. Feed the result through the same ack-and-advance pipeline the process kind uses, so `on_ok`/`on_failed`, exhaustion, and dead-lettering all work identically for both kinds with no second code path.

**Tests:**

- `crates/fsm-execute/tests/mcp_result.rs`: a successful call with `structuredContent` acks `ok` carrying it; one with only `content` acks `ok` carrying that.
- `isError: true` acks `failed` with `mcp/tool_error` and preserves the returned content.
- A JSON-RPC error acks `failed` with `mcp/rpc_error`, the code, and the message.
- Timeout, spawn failure, and protocol violation each ack `failed` with the documented `exec/*` error.
- **Determinism:** the same server response produces a byte-identical ack `result` across two runs — assert directly, and assert the object contains no timestamp, pid, or duration.
- An oversized result is truncated at the cap with a digest of the whole value, and the resulting ack is under `MAX_PAYLOAD_BYTES`.
- Invalid UTF-8 in a result survives as a lossy string plus the true digest, and the ack still journals.
- A `mcp_error` failure with `retry.on` including `"mcp_error"` retries; without it, it acks immediately.
- `on_ok` and `on_failed` fire identically for both handler kinds — assert against a process handler with the same advance configuration.
- Exhaustion and dead-lettering work for an `mcp` handler exactly as for a `process` one.

- **Done when:** `cargo test -p fsm-execute --test mcp_result` passes every case above, the mapped result is deterministic and bounded, both handler kinds share one ack-and-advance path, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** The mapping table in full, with two decisions worth naming.

**Typed when it fits, digested when it does not.** A result inside `ACK_OUTPUT_CAP` is journaled as it came — an object stays an object — because step 2's reason applies to the journal as much as to the wire: a tool that returns typed data should not have it flattened into rendered text. Only a result that must be truncated becomes a prefix string, and then `structured_sha256` is present to say so, which is the same signal `stdout_sha256` gives for a handler's capture. The truncation goes through `BoundedBytes` rather than a second implementation.

**What the UTF-8 hazard actually is.** The step says "invalid UTF-8 in a result survives as a lossy string", but a result arrives through this workspace's JSON parser, so its canonical bytes are always valid UTF-8 — a tool cannot get an invalid byte in there. The real hazard is the cap falling **inside a multi-byte character**, which a naive cut renders as a replacement character: a permanent record acquiring a character the tool never sent. That is what the row asserts, against a result of six thousand two-byte characters, and it is what `without_partial_tail` already handled.

Determinism is asserted twice: two runs of the same script produce byte-identical canonical acks, and a scan over the rendered ack refuses the words a varying field would carry (`elapsed`, `duration`, `pid`, `started`, `finished`, `ms`) — because a new field is where this property would break, not the ones already there.

**Where the classification lives.** Step 6 names `service.rs`, and `service.rs` is where the decision is *taken* — but the classifier is `RunOutcome::failure_class`, beside the outcome it classifies, so there is exactly one function saying what kind of failure a run was for both handler kinds. `mcp/tool_error` and `mcp/rpc_error` are the `"mcp_error"` class; a protocol violation is **not** retried, because a broken server produces the same broken exchange next time; timeout and spawn keep the classes they already had.

Steps 1 through 5 partly landed in `7702`, because `RunOutcome::succeeded` and `failure_class` had to be right the moment the variant existed rather than knowingly wrong for one commit. This task added the cap, the digest, the boundary-safe truncation, and every row.

The end-to-end half proves there is no second code path: one row drives the identical machine to the identical terminal state through an exit status and through a tool's own error flag, and another exhausts an `mcp_error` policy and finds the effect in the dead-letter report with `class: "mcp_error"` — the same derivation `7502` built, with nothing added for the new kind.
