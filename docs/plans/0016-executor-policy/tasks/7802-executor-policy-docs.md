---
id: executor-policy-docs
title: "Executor Policy Docs"
workstream: "0078"
kind: task
depends_on:
  - executor-policy-chaos
gated: false
touches:
  - docs/EMBEDDING.md
  - README.md
  - examples/case_review.handlers.json
  - crates/fsm-cli/tests/executor_doc.rs
  - crates/fsm-execute/tests/config.rs
  - crates/fsm-cli/tests/executor_policy_chaos.rs
status: done
merged_as: ""
---
# Executor Policy Docs

The handler table is the operator's whole interface to this plan, so every new key gets its range, its default, and — for the two decisions people will question — the reason.

**Steps:**

1. Extend `docs/EMBEDDING.md`'s handler-table section with every new key: `retry` and its four fields with ranges and defaults, `kind`, `tool`, `arguments`, `max_inflight`, and `max_inflight_per_instance`. Update the `fsm.handlers/1` field table rather than adding a second one.
2. Document retry semantics: attempts are **journaled** so a restarted executor resumes mid-retry; the backoff formula in full; and the **no-jitter** decision with its reason — determinism is what makes restart equivalence testable, and a single-node executor has no herd to spread. That reason is the first thing a reviewer will ask about.
3. Document exhaustion: it acks `failed` with `exec/retries_exhausted`, the machine's `on_failed` still fires, and a handler with **no** `on_failed` still stalls deliberately — with `fsm execute --list-dead` as the way to find those.
4. Document `"cancelled"` being un-retryable by construction, since it is the one class an operator will try to configure.
5. Document the two caps, their defaults, the round-robin fairness rule, and the fact that deferrals are logged rather than silent.
6. Document the `mcp` handler kind with the security boundary restated in full: a literal rooted `argv[0]`, one fixed tool name, an operator-written argument template, one process and one tool call per effect, and nothing constructed from machine-emitted data. Include the complete result-mapping table.
7. Extend `examples/order_lifecycle.handlers.json` — or add a sibling if that file is pinned by a golden — to demonstrate a `retry` block and an `mcp` handler in the repository's neutral vocabulary, and say which you did in the commit message.
8. Add one sentence to `README.md`'s executor paragraph: effects can call another MCP server's tool, which makes the engine an orchestrator of the ecosystem it belongs to rather than only a member of it. Add `effect_attempted` to `docs/SPEC.md`'s `### Record kinds` table with its body fields and the rule that it changes no logical state.
9. Extend `crates/fsm-cli/tests/executor_doc.rs`, which already asserts every `exec/*` code appears in EMBEDDING, to cover the new keys and codes.

**Tests:**

- `cargo test -p fsm-cli --test executor_doc` passes: every `exec/*` code in the crate's `ALL_CODES`, including the four new ones, appears in EMBEDDING.
- A documentation test asserts every key in the handler table's closed key sets appears in the EMBEDDING field table, asserted against the constants so a new key cannot ship undocumented.
- A documentation test asserts EMBEDDING contains the no-jitter reason and the backoff formula.
- A documentation test asserts EMBEDDING states that `"cancelled"` is not retryable.
- A documentation test asserts EMBEDDING restates the `argv[0]` literal-rooted-path rule for the `mcp` kind.
- `cargo test -p fsm-cli --test spec_appendix` passes with `effect_attempted` documented in `### Record kinds`, via the both-directions assertion plan 0010 added.
- `cargo test -p fsm-cli --test examples` passes with the extended handler-table example validating under `fsm execute --check`.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose and the new example.

- **Done when:** EMBEDDING documents every new key with ranges, defaults, and the two contested reasons; SPEC lists `effect_attempted`; README names the MCP handler capability; the example demonstrates both new features; `cargo test -p fsm-cli --test executor_doc --test spec_appendix --test examples --test policy` passes; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** Every new key joins the one `fsm.handlers/1` field table rather than a second one, and a test asserts the set is complete **against the parser** — a key added to the closed set and left undocumented is a key an operator cannot discover, and this is where that becomes a red test instead of a support question. The ranges and defaults are asserted against the constants the parser enforces, so a widened bound cannot ship with a narrow doc.

Three sections carry the reasoning rather than only the rules: retry (journaled attempts, the formula in full, and the no-jitter decision with its reason), exhaustion (the ordinary ack, the still-firing `on_failed`, the deliberate stall, and `--list-dead` as the way to find one), and the caps with the round-robin and the logged deferral. `"cancelled"` gets its own paragraph because it is the one class an operator will try to configure.

The `mcp` section **restates** the security boundary rather than referring to it, and a test pins that restatement inside the section: the argument is that the rule did not move, and a reader meeting the new kind has to see it there rather than be sent back a page.

**On the example.** `examples/order_lifecycle.handlers.json` is pinned twice over — the README demo runs it and `config.rs` holds it to "no committed table changes meaning" — so the new features went into a sibling, `examples/case_review.handlers.json`, in the repository's neutral vocabulary: a retried process handler, an `mcp` handler with a templated `arguments` object, and both caps. That `config.rs` assertion now names the pre-plan table rather than scanning every example, since holding a documentation fixture to a rule written for deployments would be holding it to the wrong rule.

`docs/SPEC.md` needed nothing: `7401` added `effect_attempted` to the record-kinds table when it added the kind, and `spec_appendix`'s both-directions assertion has been passing since.

**Correction.** `7801`'s harness carried a `get(...).is_none()` the all-targets clippy pass flags; fixed here.
