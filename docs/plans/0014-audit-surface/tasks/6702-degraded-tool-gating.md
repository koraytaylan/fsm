---
id: degraded-tool-gating
title: "Degraded Tool Gating"
workstream: "0067"
kind: task
depends_on:
  - degraded-serve-mode
  - store-doctor-tool
gated: false
touches:
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-core/src/error.rs
  - docs/SPEC.md
  - crates/fsm-store/src/journal_io/load.rs
  - crates/fsm-store/src/journal_io/mod.rs
  - crates/fsm-cli/src/mcp/tools/handlers/audit.rs
  - crates/fsm-cli/tests/naive_caller/session_outcomes.rs
  - crates/fsm-cli/tests/naive_caller/one_step_elicit.rs
  - crates/fsm-cli/tests/naive_caller/tool_outcomes.rs
  - crates/fsm-cli/tests/naive_caller/main.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/tests/degraded_gating.rs
status: done
merged_as: ""
---
# Degraded Tool Gating

A caller that stumbles into a refusal should learn exactly what it would have learned by asking `store_doctor` — the health, the blast radius, and the remedy — because an error that only says "unavailable" makes a model retry instead of diagnose.

**Steps:**

1. In `crates/fsm-cli/src/mcp/tools/dispatch.rs`, add the degraded gate beside the existing read-only gate, and name the allowed set once as a constant — `DEGRADED_TOOLS = ["store_doctor", "journal_verify", "journal_replay"]` — rather than as three match arms, mirroring how `MUTATING_TOOLS` is structured and for the same reason.
2. Answer the three allowed tools from a **read-only classification** rather than a healthy open. This is possible precisely because classification does not require one, which is why `6604` was required to work without a healthy store.
3. Refuse every other tool with a structured tool error carrying the health, the blast radius, and the remedy — the same three facts `store_doctor` returns, from the same source, so the two can never disagree.
4. **Allow `machine_create` with `dry_run: true`.** Validating a definition needs no store, and it is the authoring path — refusing it would block the model at the moment it is most useful. This mirrors the ruling plan 0008 made for read-only mode; route the dry-run branch before the degraded gate exactly as that plan routes it before the mutating gate, and do not invent a different rule.
5. Order the gates deliberately and comment the order: dry-run bypass first, then degraded, then read-only. A degraded store is a stronger constraint than read-only mode, so it must be checked first or a read-only degraded server would report the wrong reason.
6. Keep `tools/list` unchanged in degraded mode. A shrinking tool list would make a client cache a surface that reappears later, and the refusals are self-describing.

**Tests:**

- `crates/fsm-cli/tests/degraded_gating.rs`: `store_doctor`, `journal_verify`, and `journal_replay` all succeed against a degraded server and report the store's real health.
- Every other tool is refused with an error carrying the health, blast radius, and remedy.
- The refusal's three facts are byte-identical to the corresponding fields of `store_doctor`'s result — assert directly.
- `machine_create` with `dry_run: true` succeeds on a degraded server; without `dry_run` it is refused.
- A read-only **and** degraded server reports the degraded reason, not the read-only one.
- `tools/list` is identical in degraded and healthy modes.
- The allowed set is read from `DEGRADED_TOOLS`, asserted by iterating the registry against the constant rather than a literal list.
- A tool refused in degraded mode writes nothing and claims no `request_id`.
- Gate ordering is pinned: a test that makes both gates applicable asserts the degraded reason wins.

- **Done when:** `cargo test -p fsm-cli --test degraded_gating` passes every case above, the three diagnostic tools answer from a classification, refusals carry the same facts as `store_doctor`, dry-run authoring still works, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `DEGRADED_TOOLS` names the three that answer, beside `MUTATING_TOOLS` and for the same reason. `dispatch_degraded` takes a **path** rather than a store — there is no store — and routes in the order step 5 asks for: the dry-run bypass first, so authoring still works when it is most useful; then the degraded gate, which is the stronger constraint and so must win over read-only; then everything else is refused.

A refusal carries `store/degraded` with the health, the message, the blast radius and the remedy — read from `doctor_report`, the same function `store_doctor` returns, so the two cannot disagree. That equality is asserted field by field for **every** store-backed tool, not sampled. The hint names `store_doctor`, or the exact repair command when the table prescribes one.

`tools/list` is byte-identical to a healthy server's, asserted by canonical bytes: a shrinking list would have a client cache a surface that reappears when the store is repaired.

**Corrections.**

- *`journal_replay` could not answer for a torn store, which is half the point of allowing it.* `load_records` refuses a journal whose final record is half-written — correct for an *open*, wrong for a diagnosis. `load_intact_prefix` loads the authoritative prefix and stops at the tear, so replay reports how much of the journal is whole. "I cannot answer at all" is a worse answer than "twelve records replayed".
- *A torn tail still opens read-only, so it is not degraded.* The both-gates-apply test needs a store nothing can open, which is interior damage — the fixture makes one by breaking canonicality on top of the tear.
- *A dry-run create runs against `Store::open_memory`.* The registry's handler takes a `&mut Store` and a definition is checked against the engine rather than the store, so a scratch in-memory store is what lets the same handler answer with nothing on disk.
- *One more file split.* Driving the degraded refusal put `tool_outcomes.rs` over the thousand-line ceiling; the outcomes whose *setup* is a session or a directory now live in `session_outcomes.rs`.
- *`serve.rs` is at exactly 1 000 lines.* The next task touching it has to split it first; noting it here so nobody discovers it as a surprise.
