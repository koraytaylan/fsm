---
id: completion-capability
title: "Completion Capability"
workstream: "0063"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/mcp/complete.rs
  - crates/fsm-cli/src/mcp/elicit.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/tests/mcp_completion.rs
  - crates/fsm-cli/src/mcp/notify.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/tests/fixtures/transcripts/
  - crates/fsm-cli/tests/fixtures/mcp_live/
status: done
merged_as: ""
---
# Completion Capability

The shared half of completion — the capability, the routing, the truncation contract, and the two error rulings — lands once here so the two suppliers that follow only have to produce values.

**Steps:**

1. Add `"completions": {}` to `initialize_result`'s capabilities in `crates/fsm-cli/src/mcp/serve.rs` and route `completion/complete` to a new `crates/fsm-cli/src/mcp/complete.rs`.
2. **Scaffold both modules this plan adds, here.** Create `complete.rs` in full and `elicit.rs` as a skeleton with `unimplemented!()` bodies, and declare both in `crates/fsm-cli/src/mcp/mod.rs`. `6401` fills `elicit.rs` and must not have to edit `mod.rs`; a module cannot be declared without its file, so the shell and the declaration land together. Route nothing for elicitation — the server *sends* those requests rather than receiving them.
3. **Capture the client's capabilities at `initialize`** and store them with the other per-session state plan 0012 established. `6403` needs to know whether the client advertised `elicitation`, and it does not touch `serve.rs`; capturing it here is what lets it stay in its own files.
4. **Define the `SessionIo<'_>` seam and thread it from `serve.rs`.** `6401`'s `request_and_await` has to write a request through the `Notifier` *and* read lines from the same session input until the matching response arrives — neither of which it can reach on its own. Define the borrow that carries both, hand it to the tool-call path, and leave it unused at this stage. This is the same move `5702` made for `ToolCtx`, and for the same reason: the task that owns `serve.rs` provides the seam, and the task that needs it does not have to reshape the loop.
5. Parse the request shape — `{ref: {type, uri | name}, argument: {name, value}, context?: {arguments?}}` — and **confirm it against the MCP `2025-06-18` specification while implementing**, rather than trusting this document. A plausible-looking guess here produces a message no client understands.
6. Return `{completion: {values, total, hasMore}}` with **at most 100 values**. `total` is the number of matches before truncation and `hasMore` is `true` when it exceeds 100 — returning 100 of 4000 silently would make completion feel broken in exactly the store where it matters most.
7. Filter by the supplied `value` as a **case-sensitive prefix**. Every identifier in this system is case-sensitive, and a case-insensitive match would offer completions that then fail validation.
8. Order values the way the underlying listing orders them — most-recent-first for machines and instances — so the first suggestion is the most likely one. Do not sort alphabetically and do not re-rank.
9. Implement the two error rulings: an unknown `ref.type` is `INVALID_PARAMS`; a **known** ref with an unknown argument name returns an **empty** completion rather than an error, because "I have no suggestions" is a valid answer and an error would break a client that completes speculatively.
10. Provide the supplier seam `pub(crate) fn values_for(ref_: &Ref, arg: &str, prefix: &str, ctx: Option<&Value>, store: Option<&Store>) -> Vec<String>` returning empty at this stage; `6302` and `6303` fill it. The truncation, ordering, and error handling all live here so neither supplier reimplements them.
11. Answer with an empty completion, not an error, when there is no store — a read-only or storeless session should degrade rather than fail.

**Tests:**

- `crates/fsm-cli/tests/mcp_completion.rs`: `initialize` advertises `completions`, byte-matching the updated golden line.
- A well-formed `completion/complete` returns the documented result shape with `values`, `total`, and `hasMore`.
- Truncation: a supplier stub yielding 250 matches returns 100 values, `total: 250`, `hasMore: true`; one yielding 40 returns 40, `total: 40`, `hasMore: false`.
- Prefix filtering is case-sensitive: a lowercase prefix does not match an uppercase candidate.
- Ordering is the supplier's order, not sorted — assert with a stub returning deliberately unsorted values.
- An unknown `ref.type` returns `INVALID_PARAMS`.
- A known ref with an unrecognised argument name returns an empty completion with `total: 0` and no error.
- A session with no store returns an empty completion rather than an error.
- A request with `context.arguments` present is accepted and the context reaches the supplier seam unchanged.
- Both `complete.rs` and `elicit.rs` are declared in `mcp/mod.rs` and the crate compiles with the elicitation shell in place.
- The client's `initialize` capabilities are captured on the session: a client advertising `elicitation` and one that does not are distinguishable from the session state after `initialize`.
- `SessionIo` exists, carries both the notifier and the session input, and is reachable from the tool-call path — asserted by a test that constructs one, since `6401` depends on it existing.

- **Done when:** `cargo test -p fsm-cli --test mcp_completion` passes every case above including both error rulings and the truncation contract, the request and response shapes are verified against the specification, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `completions: {}` is advertised, `completion/complete` is routed, and `complete.rs` holds everything that is not "which values": the request shape verified against the `2025-06-18` schema (`ref` is `ref/prompt` with a `name` or `ref/resource` with a `uri`; `argument` is `{name, value}`; `context.arguments` carries previously-resolved arguments), a case-sensitive prefix filter, the supplier's own order kept, truncation at 100 with `total` counted **before** it and `hasMore` saying so, and both error rulings — an unknown `ref.type` is `INVALID_PARAMS`, an unknown argument name is an empty completion. A session with no store answers empty rather than failing.

`SessionIo` carries a session's two halves — the one writer, and the input the loop is reading — so 6401 can write a request and read its response without reshaping the loop. Threading it needed one change to the loop itself: the line is bound before it is matched, because a `match` scrutinee holds its temporaries for the whole match and the arm has to borrow the same reader.

`elicit.rs` lands as a shell with the one function that is genuinely 6301's: whether the client advertised `elicitation`, captured at `initialize` because that is the only message carrying it.

**Corrections.**

- *The truncation seam is a public function, not a private supplier stub.* The tests step 6 asks for need to feed 250 candidates through the rules; `completion_from(candidates, prefix)` is the one place filtering, ordering and truncation live, and `values_for` feeds it. A test-only injection point would have been a second path through the same rules.
- *`SessionIo` lives in `notify.rs`, beside the writer it holds.* A third module for a two-field borrow would separate it from the `Notifier` it half is.
- *The captured client capability is observable through the seam that needs it.* `Live` is private to `serve.rs` by design, so the flag is asserted as the pure predicate `elicit::client_supports` and carried on `ToolCtx`, which is where 6403 reads it. A test that could read `Live` directly would mean `Live` was public, which is a worse trade than this one.
- *Three test files stopped naming every `ToolCtx` field.* They now spread `..Default::default()`, so the next field added to the context does not break a suite that does not care about it.
- *Seven goldens gained `"completions":{}`.* Verified field by field: one added key in the `initialize` result, nothing else.
