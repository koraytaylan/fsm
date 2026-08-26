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
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/tests/degraded_gating.rs
status: planned
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
