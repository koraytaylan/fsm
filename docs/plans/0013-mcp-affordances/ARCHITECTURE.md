# Architecture — Plan 0013

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Fixtures first: commit the transcript goldens your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version.
7. When a golden fails, fix the code to match the fixture — unless the fixture contradicts this document or the MCP specification revision `2025-06-18`.
8. **Existing MCP goldens are regenerated, not hand-edited.** `crates/fsm-cli/tests/mcp_skeleton.rs` and `mcp_full.rs` carry `REGEN_SKELETON=1` and `REGEN_MCP_FULL=1` regeneration paths, and `docs/RELEASE.md` names them. Where a golden this plan moves has one, run it, then **read the resulting diff line by line** — regeneration is a typing shortcut, never a review shortcut. Hand-derive only *new* fixtures for *new* behaviour, where there is nothing to regenerate from and a captured first run would only prove the implementation agrees with itself.
9. **Verify every protocol shape in this plan against the specification while implementing.** This plan touches four areas — annotations, completion, elicitation, and capability negotiation — where a plausible-looking guess produces a message no client understands. The architecture states the shapes; confirm them.

## 0000 — Orientation: the three facts that shape this plan

- **`MUTATING_TOOLS` is the read/write split, and it is authoritative.** `crates/fsm-cli/src/mcp/tools/mod.rs` documents it as counted from `store/lifecycle.rs` and `store/instance/*.rs` rather than from memory, and `dispatch` consults it instead of six match arms. Every `readOnlyHint` in §0062 derives from it. **Do not hand-write a second table**; a hint that disagrees with the gate is worse than no hint.
- **Every mutating tool already requires a `request_id`.** `schema_in.rs` marks it required on `instance_create`, `instance_send`, `deadline_poll`, `effect_ack`, and `instance_cancel`, and the store keys idempotency on `(request_id, request_fp)`. That is a genuine `idempotentHint: true`, and it is a stronger claim than most servers can make.
- **The serve loop only ever reads client requests.** `parse_line` returns `Incoming::Request` or `Incoming::Notification`; there is no arm for a *response*, because the server has never sent a request. Elicitation makes the server a requester, and §0064 adds that arm before anything can use it. This is the only structural change in the plan.

## 0062 — Annotations and titles

Task `6201` extends `ToolSpec` in `crates/fsm-cli/src/mcp/tools/mod.rs` with `title: &'static str` and `annotations: fn() -> Value`, and `tools_list_result` emits both.

Each hint is **derived**, not declared:

| Hint | Derivation |
|---|---|
| `readOnlyHint` | `!MUTATING_TOOLS.contains(&name)` — one expression, no second table |
| `destructiveHint` | `true` for `instance_cancel` only; `false` for every other tool. Meaningful only when `readOnlyHint` is false, and the specification says so, but emit it consistently rather than conditionally |
| `idempotentHint` | `true` for every tool in `MUTATING_TOOLS`, because every one of them is keyed by `request_id` and the store refuses a reused key with different content rather than replaying it. `machine_create` qualifies because `if_exists: "return_existing"` is its idempotent form and content addressing makes a repeat definition the same machine |
| `openWorldHint` | `false` for every tool. This server reads and writes one data directory and reaches nothing else. Effects reach the outside world, but the **executor** runs them; no tool call in this surface does |

`title` is a short human display name — "Create machine", "Send event", "Cancel instance" — living beside the existing description constants in `crates/fsm-cli/src/mcp/descriptions.rs` so the two cannot drift apart.

Task `6202` adds `title` to every resource in `resources/list`, every entry in `resources/templates/list`, and the prompt in `prompts/list`. The rule is the same one the tool titles follow: `name` is the identifier, `title` is what a person reads.

Both tasks move the `tools/list`, `resources/list`, `resources/templates/list`, and `prompts/list` goldens. They are the only tasks allowed to, and each owns exactly the goldens for the lists it changes.

**The `tools/list` response grows, and this is where that stops being incremental.** `crates/fsm-cli/tests/tools_budget.rs` asserts `canon_bytes(tools_list_result()).len() <= 20_000`. That ceiling was set for fourteen tools with no titles and no annotations. By the time this plan runs, plans 0010 and 0011 have added four tools; this plan adds a title and four hints to every one of them plus a fifteenth tool; plan 0014 adds five more. Four separate tasks each nudging the number up is how a budget stops meaning anything.

So `6201` makes **one** decision for the whole sequence: measure the annotated surface, set the ceiling once with stated headroom for plan 0014's five tools, and record the measured size and the reasoning in the commit message. `6403` and plan 0014's `6801` then **assert they fit** under that ceiling rather than raising it again — and if either does not fit, the response is to shorten descriptions, not to raise. `tools/list` is sent once per session and every byte of it is context the model pays for on every conversation; the budget exists to keep that honest, and a ceiling that only ever goes up is not a budget.

## 0063 — Completion

### The capability

Task `6301` adds `"completions": {}` to `initialize_result`'s capabilities and routes `completion/complete` in `crates/fsm-cli/src/mcp/serve.rs` to a new `crates/fsm-cli/src/mcp/complete.rs`.

The request shape, which must be confirmed against the specification: `{ref: {type: "ref/resource" | "ref/prompt", uri | name}, argument: {name, value}, context?: {arguments?: {..}}}`. The response is `{completion: {values: [..], total?, hasMore?}}` with **at most 100 values**.

Rules that hold for every completion in this plan:

- Values are filtered by the supplied `value` as a **prefix**, case-sensitively, because every identifier in this system is case-sensitive and a case-insensitive match would offer completions that then fail validation.
- Values are ordered the way the underlying listing is ordered — most-recent-first for instances and machines — so the first suggestion is the most likely one.
- `total` is the number of matches before truncation and `hasMore` is `true` when it exceeds 100. Returning 100 of 4000 silently would make completion feel broken in exactly the store where it matters most.
- An unknown `ref` is `INVALID_PARAMS`; a known ref with an unknown argument name returns an **empty** completion rather than an error, because "I have no suggestions" is a valid answer and an error would break a client that completes speculatively.

### What is completable

Task `6302` completes resource template variables: the `{id}` of `fsm://machine/{id}` from the machine catalogue, and the `{id}` of `fsm://instance/{id}` and `fsm://instance/{id}/history` from the instance listing.

Task `6303` is where completion earns its place. `crates/fsm-cli/src/mcp/prompts.rs` gains two prompts whose arguments are worth completing:

- `drive_instance` with arguments `instance_id` (required) and `event` (optional).
- `diagnose_instance` with argument `instance_id` (required).

Completing `event` uses the `2025-06-18` **resolved-argument context**: when `context.arguments.instance_id` is present, the values are that instance's `enabled_events` — the engine's own analysis, offered at the moment somebody is deciding what to send, filtered to genuinely enabled events rather than every declared name. Without the context argument, `event` returns an empty completion; guessing from the whole store would suggest events that cannot fire.

Internal events and generated `$`-prefixed names are excluded from `event` completions, since they are not externally sendable. This falls out of `enabled_events` already excluding them after plan 0009, and the test pins it so the two stay connected.

## 0064 — Elicitation

### Inbound responses

This is the structural change. `crates/fsm-cli/src/mcp/jsonrpc.rs` gains `Incoming::Response { id, result, error }`, and `parse_line` recognises it: a message with an `id` and either `result` or `error`, and no `method`.

Task `6401` implements the exchange in `crates/fsm-cli/src/mcp/elicit.rs`:

```rust
pub fn request_and_await(io: &mut SessionIo<'_>, params: Value, clock: &mut dyn Clock)
    -> Result<ElicitResult, ErrorObj>
```

- Generates a server-side request id from a monotonic per-session counter, prefixed so it can never collide with a client id: `"fsm-elicit-<n>"`.
- Writes `elicitation/create` through the `Notifier`, then reads lines from the same input until it sees a **response with the matching id**.
- **Client requests that arrive while waiting are handled normally**, by re-entering the request handler at nesting depth 1. A client is not obliged to stop working because the server asked a question, and a server that ignored its requests would deadlock a well-behaved client.
- **Nesting is capped at 1.** An elicitation attempted while one is outstanding returns a tool error immediately. A recursive ask is a design mistake, and a cap makes it a diagnosable one.
- **A timeout, read from the injected clock**, defaulting to 300 seconds, after which the exchange returns a tool error naming the timeout. `notifications/cancelled` for the elicitation is also honoured. A server that waits forever for a client that will never answer is a hung server.
- EOF while waiting ends the session normally — the client is gone, and there is nothing to answer.

### The tool

Task `6402` adds `instance_elicit(instance_id, event, request_id)` to the registry. It:

1. Reads the instance and confirms `event` is currently enabled, refusing with the ordinary `run/not_enabled` vocabulary otherwise — asking a person to fill in a form for an event that cannot fire is worse than refusing.
2. Refuses with a clear tool error when the client did **not** advertise `elicitation` in its `initialize` capabilities, naming `instance_send` as the direct path. Capability detection is read once at `initialize` and stored on the session.
3. Builds the elicitation schema from the event's declared fields (§ below) and performs the exchange.
4. On `action: "accept"`, coerces the returned content into a typed payload and calls the ordinary `instance_send` path with the caller's `request_id`. **The journal records the event and nothing else** — there is no elicitation record, because what happened to the workflow is that an event arrived.
5. On `action: "decline"` or `"cancel"`, sends **nothing**, journals nothing, consumes no `request_id`, and returns a structured result naming the action so the caller can react.

It joins `MUTATING_TOOLS`: it can write, so a read-only server must refuse it.

### Schema derivation

Task `6403`. MCP elicitation restricts a schema to a **flat object of primitive properties**, which is exactly the shape an event's `fields` already has. The mapping:

| Declared type | Elicitation property |
|---|---|
| `str` | `{"type": "string"}` |
| `int` | `{"type": "integer"}` |
| `bool` | `{"type": "boolean"}` |
| `{enum: "Name"}` | `{"type": "string", "enum": [variants…]}` |
| `{decimal: "N"}` | `{"type": "string", "description": "decimal with exactly N fraction digits"}` |
| `timestamp` | `{"type": "string", "format": "date-time"}` |
| `duration` | `{"type": "string", "description": "duration…"}` |

Decimals and timestamps are **strings**, matching SPEC's rule that numerics are strings everywhere and a raw JSON number is `req/number_token`. Sending `{"type": "number"}` for a decimal would invite a client to return a float, which is the one thing this engine refuses to accept anywhere.

Every field is `required` unless the machine gives it a default, since `validate_event` demands the exact declared field set. Each property's `description` is the field's own documentation where the definition has one, so the form a person sees is the form the machine's author wrote.

Coming back, the response content is coerced through the **same** `coerce_ctx_override`-style typed path an external payload uses (`store/json_helpers.rs`), so an elicited value and a sent value are validated identically. A response that fails coercion returns a tool error naming the field, and sends nothing.

## 0065 — Proof and docs

**Goldens (task `6501`).** Byte-exact fixtures for `tools/list` with annotations and titles, `resources/list` and `resources/templates/list` with titles, `prompts/list` with the two new prompts, four `completion/complete` exchanges (machine id, instance id, event with context, event without context), and a full elicitation exchange in both directions — accept and decline — including a client request handled *while* the elicitation was outstanding, since that is the interleaving the design specifically allows.

**Docs (task `6502`).** `docs/EMBEDDING.md` gains an *Affordances* section: what each annotation claims, with the `idempotentHint` claim spelled out because it is unusually strong and a host operator should know it is exact rather than aspirational; what is completable and what is deliberately not; and the elicitation path with its three honest limits — the client must advertise the capability, nesting is capped at one, and there is a timeout. `README.md` gains one guarantee row about annotation accuracy. The MCP `instructions` string is **not** touched: plan 0012 already spent this plan sequence's one sentence there, and the two prompts are self-describing.
