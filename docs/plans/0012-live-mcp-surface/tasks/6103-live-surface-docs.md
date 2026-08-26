---
id: live-surface-docs
title: "Live Surface Docs"
workstream: "0061"
kind: task
depends_on:
  - notification-ordering-suite
gated: false
touches:
  - docs/EMBEDDING.md
  - README.md
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/tests/mcp_prompts.rs
status: planned
merged_as: ""
---
# Live Surface Docs

The README currently promises live watching that does not exist; after this plan it can say something true, and the `instructions` string is where a model finds out it may subscribe instead of poll.

**Steps:**

1. Add a *Watching a store live* section to `docs/EMBEDDING.md` covering: the two instance resource URIs; `resources/subscribe` and its per-session, 64-URI nature; what triggers `resources/updated` and what triggers `list_changed`; the 250 ms poll interval and how to think about the resulting latency; the `logging` capability and the fact that embedded executor ticks now reach the client; progress tokens; and the cancellation limit.
2. State the cancellation limit in the same paragraph that advertises the capability, not in a footnote: **a single tool call is not interruptible mid-step**; cancellation applies before dispatch and at coarse loop boundaries; engine operations are bounded by the evaluation budget and are short by construction. A capability that overpromises is worse than one that is absent.
3. Correct `README.md`'s read-only pairing paragraph. It currently says `fsm serve --read-only` "lets the model watch its acks and transitions arrive live", which was not true — the session re-read the journal once per incoming request. Replace it with what is now true: the model subscribes to an instance and is notified when it advances.
4. Add one guarantee row to `README.md`: *live subscriptions — a subscribed resource notifies on change, from a poll loop that takes no lock and perturbs no writer*.
5. Add **one sentence** to the MCP `instructions` string in `crates/fsm-cli/src/mcp/prompts.rs` telling a model it may subscribe to `fsm://instance/{id}` rather than polling `instance_get`. One sentence, because instructions are read on every session and length has a cost — and this is the only transcript-moving edit in the task.
6. Update the `instructions` assertion in `crates/fsm-cli/tests/mcp_prompts.rs` and any transcript golden that embeds the string, in this commit. This is the plan's third and final golden move, and like the other two it belongs to the task that causes it.
7. Do not describe HTTP, sessions, or authentication. Plan 0015 owns those, and documenting them early would leave a doc that is wrong until it lands.

**Tests:**

- `cargo test -p fsm-cli --test mcp_prompts` passes with the updated `instructions` assertion.
- A documentation test asserts `docs/EMBEDDING.md` contains the cancellation limit sentence, so the honest caveat cannot be quietly dropped.
- A documentation test asserts `README.md` no longer contains the superseded "watch its acks and transitions arrive live" phrasing.
- A documentation test asserts `docs/EMBEDDING.md` names the subscription cap and the poll interval, since both are numbers a user will otherwise have to read the source to learn.
- The `instructions` string grows by exactly one sentence — assert a length bound, so a later edit cannot bloat what every session pays for.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose.
- Every transcript golden embedding `instructions` is updated and passes.

- **Done when:** EMBEDDING documents the live surface including the honest cancellation limit, README's read-only paragraph is corrected and carries the new guarantee row, `instructions` gains exactly one sentence with its goldens updated, `cargo test -p fsm-cli --test mcp_prompts --test policy` passes, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
