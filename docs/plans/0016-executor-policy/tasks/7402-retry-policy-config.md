---
id: retry-policy-config
title: "Retry Policy Config"
workstream: "0074"
kind: task
depends_on: []
gated: false
touches:
  - docs/EMBEDDING.md
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/tests/config.rs
status: done
merged_as: ""
---
# Retry Policy Config

Retry belongs in the operator's table rather than in a machine definition, because it is an infrastructure concern — and every existing table must keep its exact behaviour, which means the default is one attempt.

**Steps:**

1. Add an optional `retry` object to `HandlerSpec` in `crates/fsm-execute/src/config.rs`: `{attempts, backoff_ms, max_backoff_ms, on}`. Add `"retry"` to the handler's closed `HANDLER_KEYS` set, so a misspelling is refused rather than silently ignored — the same reasoning that refuses `on_okay` today.
2. `attempts` is the **total** including the first, an integer from 1 to 16. Absent `retry` means `attempts: 1`, which is exactly today's behaviour, so no committed table changes meaning. Above 16 is `exec/config`: a table that would retry sixty times has a typo in it.
3. `backoff_ms` defaults to 1000 and `max_backoff_ms` to 60000; both are positive integers with `max_backoff_ms >= backoff_ms`, violations reported as `exec/config` with the offending handler index in `details`.
4. `on` is a closed set of failure classes — `"nonzero_exit"`, `"timeout"`, `"spawn"`, and `"mcp_error"` — defaulting to all of them when absent. A class outside the set is `exec/config` with the valid list in the hint.
5. **`"cancelled"` is not a valid class and must be refused explicitly** with a hint saying so. A handler killed because its instance was cancelled must never be restarted; that is the one kill that means stop, and a table author who tries to make it retryable deserves an error rather than silence.
6. Validate the whole block at startup with the rest of the table, before any store is opened, so `fsm execute --check` catches it.
7. Add the plan's new codes to `crates/fsm-execute/src/error.rs`'s `ALL_CODES` so no later task edits that file: `exec/retries_exhausted`, `exec/mcp_protocol`, `exec/mcp_tool`, `exec/inflight_deferred`.

**Tests:**

- `crates/fsm-execute/tests/config.rs`: a handler with a full valid `retry` block parses with every field; one with no `retry` yields `attempts: 1` and today's behaviour.
- Defaults apply individually: a `retry` with only `attempts` gets the documented `backoff_ms` and `max_backoff_ms`.
- `attempts` of 0 and of 17 are each `exec/config` with the handler index in `details`; 1 and 16 are accepted.
- `max_backoff_ms` below `backoff_ms` is `exec/config`.
- An unknown class in `on` is `exec/config` with the valid list in the hint.
- `"cancelled"` in `on` is `exec/config` with the explanatory hint — pin the message, since this is the rule most likely to be requested as a feature later.
- A misspelled `retries` key is refused by the closed key set.
- `fsm execute --check` reports every one of these before opening any store.
- Every committed example handler table still validates and still yields one attempt.
- `ALL_CODES` entries are unique, non-empty, and prefixed `exec/`.

- **Done when:** `cargo test -p fsm-execute --test config` passes every case above, an absent `retry` preserves today's behaviour exactly, `"cancelled"` is refused with its explanation, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `retry` is `{attempts, backoff_ms, max_backoff_ms, on}` inside the handler's closed key set, so `retries` and `attemps` are refused rather than silently ignored — the same reasoning that already refuses `on_okay`. Absent means `attempts: 1`, which is exactly today's behaviour, and a test walks every committed example table asserting none of them changed meaning.

`attempts` is the **total** including the first, 1 through 16: a table that would retry sixty times has a typo in it, and refusing the typo is worth more than serving the one operator who meant it. `on` is a closed set of four classes, and **`cancelled` is refused by name** with the reason in the message — a handler killed because its instance was cancelled must never be restarted, and that is the rule most likely to be requested as a feature later, so its words are pinned.

This plan's four codes are registered here so no later task edits `error.rs`, and all four are documented in EMBEDDING's executor table in the same commit, because the doc test requires it.

**Corrections.**

- *The crate's own closed-set count moved from ten to fourteen.* It is asserted in two places in `error.rs`; both say what the number means now.
