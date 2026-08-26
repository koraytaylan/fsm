# Architecture — Plan 0016

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Fixtures first: commit the handler-table examples and goldens your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version.
7. When a golden fails, fix the code to match the fixture.
8. **Two of plan 0008's rules are load-bearing here and neither may be weakened:** the journal is the executor's only memory, and the handler table is the security boundary. Every design decision below follows from one of them; if a shortcut seems attractive, check which rule it breaks.

## 0000 — Orientation: the four facts that shape this plan

- **The scheduler is pure and holds no clock.** `Scheduler::on_observation(&mut self, obs, now_ms) -> Vec<Directive>` takes time as a parameter. Retry deadlines and backoff must therefore be computed from `now_ms` and journaled timestamps, never from an elapsed timer, or the scheduler stops being a function of its inputs and the restart-equivalence test stops meaning anything.
- **`inflight` is process-local and correctness must not depend on it.** Plan 0008's architecture: "`inflight` tracks only what is running in *this* process right now; every other decision is taken from the `Observation`, so a fresh process with an empty `inflight` reaches the same conclusions." An attempt counter kept there would be lost by exactly the restart it exists to survive.
- **`claimed_request_ids` is how "did I already do this?" is answered.** Copied from `store.state.dedup` with the `exec-` prefix. Every new directive in this plan derives a key and checks it there — no new mechanism.
- **The runner already spawns processes with file-redirected capture under a timeout, and kills them.** An MCP handler is a process that gets talked to over its stdio rather than one whose exit status is the answer. Reuse the spawn, the timeout, the kill, and the bounded capture; add a conversation.

## 0074 — Attempt accounting and retry

### The record

A retry that is not journaled is a retry two processes can disagree about, so **every attempt is a record** (task `7401`).

New kind `effect_attempted`, body `{instance_id, effect_id, attempt, outcome, result, request_id, state_hash, state_format}`, written by a new store operation beside `ack_effect_outcome_on` in `crates/fsm-store/src/store/instance/ack.rs`:

- `attempt` is 1-based and strictly increasing per `effect_id`. A record whose `attempt` is not exactly one more than the last for that effect is refused — a gap would make the count unreliable.
- `outcome` is `"failed"` always. A **successful** attempt does not produce this record; it produces the ordinary `effect_acked`. `effect_attempted` records exactly the attempts that failed and will be retried, which is why the count derives cleanly.
- The record **does not clear the pending effect** and changes no logical state beyond claiming its key. The effect stays in `effects_pending` and the instance stays where it was; that is what makes a retry a retry rather than a re-emit.
- `result` carries the same bounded, digest-backed capture an ack carries, so the audit trail holds what each attempt actually produced rather than only what the last one did.
- Derived key `exec-try-{effect_id}-{attempt}` (task `7403`, in `rid.rs`), so a restart re-issuing the same attempt replays rather than double-writing.

The **terminal** outcome is unchanged: a success acks `ok`, and an exhausted retry acks `failed`. `effect_acked` keeps its exact current meaning and shape, so every existing consumer and golden is untouched.

### The policy

`HandlerSpec` gains one optional key (task `7402`, `crates/fsm-execute/src/config.rs`):

```json
"retry": {
  "attempts": 3,
  "backoff_ms": 1000,
  "max_backoff_ms": 60000,
  "on": ["timeout", "spawn", "nonzero_exit"]
}
```

- `attempts` is the **total** number of attempts including the first, 1 to 16. `1` means no retry and is the default when the key is absent, so every existing table keeps its exact behaviour.
- `on` is a closed set of failure classes: `"nonzero_exit"`, `"timeout"`, `"spawn"`, and — after §0077 — `"mcp_error"`. Absent means all of them. A class outside the set is `exec/config`.
- **`cancelled` is never retryable** and cannot be listed. A handler killed because its instance was cancelled must not be restarted; that is the one kill that means "stop".
- The whole block validates at startup like the rest of the table, and `attempts` above 16 is `exec/config` — a table that would retry sixty times is a table with a typo.

### The scheduler

Task `7403` extends the `Observation` with per-effect attempt state read from the journal — the highest `attempt` for each pending `effect_id` and the `ts` of that record — and adds one decision rule ahead of the existing start rule:

> A pending effect with a handler, no `inflight` entry, an unclaimed `ack_rid`, an attempt count below `attempts`, and a backoff deadline at or before `now_ms` → `Start` with `attempt = last + 1`.

Everything else falls out. An effect whose attempts are exhausted takes the ack path with the exhaustion cause (§0075). An effect still inside its backoff window produces no directive at all, which is what makes backoff free rather than a sleep.

## 0075 — Backoff and dead letters

**The schedule (task `7501`).** Exponential with a ceiling, computed from journaled facts only:

```
due_ms = last_attempt_ts + min(backoff_ms * 2^(attempt - 1), max_backoff_ms)
```

`last_attempt_ts` is the `ts` of the most recent `effect_attempted` record for that effect — a journaled value, so a restarted executor computes the identical deadline. There is deliberately **no jitter**: jitter would make the scheduler non-deterministic, which would break the restart-equivalence property plan 0008's suite pins, and a single-node executor has no thundering herd to spread.

**Exhaustion (task `7502`).** When `attempt == attempts` and the last attempt failed, the effect acks `failed` through the ordinary path with `result.error = "exec/retries_exhausted"` and `result.attempts` naming the count. Two consequences worth stating:

- The machine's `on_failed` advance still fires. Exhaustion is a failure like any other from the machine's point of view, and a definition that models a failure path keeps working without change.
- A handler with **no** `on_failed` stalls, exactly as plan 0008 documented for an undeclared failure. That is still the right behaviour, and it is why the dead-letter report exists.

**The report.** `fsm execute --list-dead` and a `dead_letters` field on the executor's status output list every effect acked `failed` with the exhaustion cause, with its instance, effect name, attempt count, and last capture. It is **derived from the journal** at read time and stores nothing — a dead-letter queue with its own state would be a second source of truth about what happened, and the journal already knows.

## 0076 — Concurrency and fairness

Two caps in the handler table's top level (task `7601`), applied by the scheduler:

- `max_inflight` — global, default 8, range 1 to 64.
- `max_inflight_per_instance` — default 2, range 1 to 16.

Both are applied **deterministically**: candidates are ordered by `effect_id`, which is `{instance}/{seq}/{k}` and therefore a stable total order, and taken until a cap binds. The same observation always produces the same directives, which the existing determinism test extends to cover.

**Fairness (task `7602`).** Ordering by `effect_id` alone would let the lexicographically-first instance take every slot forever. The scheduler instead round-robins: order candidates by `(position within their instance's queue, instance_id, effect_id)`, so every instance gets its first pending effect considered before any instance gets its second. Deterministic, restart-stable, and computable from the observation alone.

`log()` what a cap deferred: a tick that starts 8 of 40 candidates says so, once per tick, at the identifier-only level plan 0008 requires. Silent truncation reads as "nothing to do" and is exactly the failure mode an operator cannot diagnose.

## 0077 — MCP handlers

A second handler kind (task `7701`), tagged so the two cannot be confused:

```json
{
  "effect": "summarize_case",
  "kind": "mcp",
  "argv": ["/usr/local/bin/some-mcp-server", "--stdio"],
  "tool": "summarize",
  "arguments": { "case_id": "{case_id}", "mode": "brief" },
  "timeout_ms": 60000,
  "retry": { "attempts": 3, "backoff_ms": 2000 },
  "on_ok": { "event": "summarized" },
  "on_failed": { "event": "summary_failed" }
}
```

- `kind` is `"process"` (default, and what every existing table means) or `"mcp"`. A table with no `kind` behaves exactly as it does today.
- **The security boundary does not widen.** `argv[0]` is still a literal rooted path with no placeholder; `tool` is a fixed name; `arguments` is a template whose placeholders substitute effect args by the same `{name}` rule and the same canonical rendering the process kind uses. Nothing about the handler is constructed from machine-emitted data.
- `arguments` values may be strings, numbers, booleans, or nested objects and arrays, since a tool's input schema is not restricted the way argv is. Placeholder substitution applies to **string** values only, and a placeholder naming an absent effect arg is a run-time failure of that effect exactly as it is for the process kind.

**The client (task `7702`, `crates/fsm-execute/src/mcp_client.rs`).** A minimal stdio MCP client using the workspace's own JSON parser and writer:

1. Spawn `argv` with piped stdin and stdout, under the handler's existing timeout and kill machinery.
2. Send `initialize` with protocol version `2025-06-18`, read the result, send `notifications/initialized`.
3. Send one `tools/call` with the substituted arguments; read until its response.
4. Send no second call. **One effect is one tool call**, and a handler that needs two is two effects — which keeps each one independently retryable and independently journaled.
5. Kill and reap on timeout, exactly as the process runner does, and account the same bounded capture over the server's stderr so a crashing server leaves evidence.

**One process per effect.** No pooling and no long-lived connections: the same reasoning that gives each subprocess handler its own process — an isolated timeout, an isolated kill, no state shared between effects that could make one effect's failure another's problem.

**Result mapping (task `7703`).** The `tools/call` result becomes the ack `result` deterministically:

| Server response | Ack |
|---|---|
| result with `isError` absent or false | `ok`, `result = {"structured": <structuredContent or content>}` |
| result with `isError: true` | `failed`, `result = {"error": "mcp/tool_error", "structured": …}` |
| JSON-RPC error | `failed`, `result = {"error": "mcp/rpc_error", "code": …, "message": …}` |
| timeout / spawn failure / protocol violation | `failed`, `result = {"error": "exec/timeout" \| "exec/spawn" \| "exec/mcp_protocol"}` |

The mapped result must be **deterministic and bounded**: no timestamps, no pids, no durations, and the same `MAX_PAYLOAD_BYTES`-respecting capping and digesting the process kind uses — the store fingerprints the ack over this value, and a re-issue with different content is a conflict rather than a replay.

New codes `exec/mcp_protocol` and `exec/mcp_tool` join `fsm-execute`'s `ALL_CODES`, and `"mcp_error"` becomes a valid `retry.on` class.

## 0078 — Proof and docs

**Chaos (task `7801`).** `crates/fsm-cli/tests/executor_policy_chaos.rs`, following `executor_chaos.rs`'s seeded-restart precedent, with restart points inside a retry sequence: after attempt 1 before its record, after the record before the backoff elapses, during backoff, after exhaustion before the ack, and mid-MCP-conversation. Invariants: attempts are gapless and strictly increasing per effect; **at most `attempts` attempt records** per effect; exactly one `effect_acked` per effect; the ack's cause is exhaustion if and only if the count reached the limit; caps are never exceeded across a tick even with a fresh scheduler; and no cancelled effect is ever retried.

**Docs (task `7802`).** `docs/EMBEDDING.md`'s handler-table section gains every new key with its range and default, the retry semantics including the no-jitter decision and why, the exhaustion behaviour and its interaction with a missing `on_failed`, the two caps and the fairness rule, and the MCP handler kind with the argv/tool/arguments security boundary restated. `README.md`'s executor paragraph gains one sentence: effects can now call another MCP server's tool, which makes the engine an orchestrator of the ecosystem it belongs to rather than only a member of it.
