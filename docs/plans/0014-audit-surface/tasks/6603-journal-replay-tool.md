---
id: journal-replay-tool
title: "Journal Replay Tool"
workstream: "0066"
kind: task
depends_on:
  - journal-verify-tool
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/tests/audit_replay.rs
status: planned
merged_as: ""
---
# Journal Replay Tool

Replay determinism is a headline property of this engine, and the operation that demonstrates it has been reachable only from a terminal.

**Steps:**

1. Add `journal_replay(to_seq?)` to the registry, folding the journal through the pure engine and returning `{replayed_records, state_root, matches, first_divergence_seq?}`.
2. `matches` is true when every journaled `state_hash` reproduced. `first_divergence_seq` is present only when it did not, naming the earliest record whose recomputed hash differed — the earliest, not the last, because a divergence propagates and only the first one is a clue.
3. Report the recomputed `state_root` explicitly, so a caller can compare two runs, two machines, or a store against a backup. That comparison is most of what this tool is for.
4. Distinguish replay from verification in the description, since a reader will otherwise assume they are the same: **verification** checks the bytes and the chain; **replay** re-executes the engine and checks that the recorded outcomes are the outcomes the engine produces today. A store can verify clean and still fail replay if the engine's semantics drifted, and that is precisely the failure this tool catches.
5. Wire plan 0012's progress reporting and cancellation at the record loop, exactly as `6602` did, sharing the same 256-record cadence so the two tools feel alike.
6. Honour `to_seq` so a caller can replay a prefix, which is what a bisection over a divergence needs.
7. Keep it out of `MUTATING_TOOLS`, read through `Store::open_read_only`, write nothing, and take no lock.

**Tests:**

- `crates/fsm-cli/tests/audit_replay.rs`: a healthy store replays with `matches: true`, `replayed_records` equal to the journal length, and a `state_root` matching an independent fold.
- A store whose journaled `state_hash` was tampered at seq N replays with `matches: false` and `first_divergence_seq: N`.
- With two tampered records, `first_divergence_seq` names the **earlier** one.
- `to_seq` bounds the replay and the reported `state_root` is the root at that seq.
- A call with a `progressToken` reports progress; without one it emits none.
- A cancelled call returns `req/cancelled` at a record boundary.
- The tool writes nothing — assert the journal length and mtime are unchanged — and takes no lock, verified by replaying while a writable `Store` is open.
- Replay and verify disagree correctly on a store that is byte-clean but semantically divergent: verify reports `Ok`, replay reports `matches: false`. This is the case that justifies having both tools, so it must be pinned.
- The tool is absent from `MUTATING_TOOLS` and works on a read-only server.

- **Done when:** `cargo test -p fsm-cli --test audit_replay` passes every case above including the verify-clean-but-replay-divergent case, progress and cancellation are wired, nothing is written and no lock is taken, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
