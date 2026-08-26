# Architecture — Plan 0012

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here.
2. Fixtures first: commit the transcript goldens your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.
6. Write the obvious version. Blocking threads and a mutex, never an async runtime — this workspace has zero dependencies and intends to keep them.
7. When a golden fails, fix the code to match the fixture — unless the fixture contradicts this document or the MCP specification revision named below.
8. **This plan moves the `initialize` golden.** That is expected and is `5702`'s job. Every other task must leave it alone, and a task that finds itself editing a transcript golden it does not own has made a mistake.
9. **Existing MCP goldens are regenerated, not hand-edited.** `crates/fsm-cli/tests/mcp_skeleton.rs` and `mcp_full.rs` carry `REGEN_SKELETON=1` and `REGEN_MCP_FULL=1` regeneration paths, and `docs/RELEASE.md` names them. Where a golden this plan moves has one, run it, then **read the resulting diff line by line** — regeneration is a typing shortcut, never a review shortcut. Hand-derive only *new* fixtures for *new* behaviour, where there is nothing to regenerate from and a captured first run would only prove the implementation agrees with itself.

## 0000 — Orientation: the five facts that shape this plan

- **The loop blocks on the client.** `serve_session_with` calls `read_capped_line(&mut input, LINE_CAP)?` and does nothing until a line arrives. Any notification produced while it is blocked must be written by **another thread**, which is why §0057 exists before anything else in this plan.
- **`stdout` is the protocol stream and one stray byte is a protocol error.** `send_line` writes canonical bytes plus `\n` and flushes, with a `debug_assert!` that the payload contains no newline. Two producers writing concurrently would interleave bytes inside a line. The multiplexer's only real job is to make that impossible.
- **`open_read_only` takes no lock, creates nothing, and returns one consistent hash-verified prefix.** `journal_io/open.rs` sets `_lock: None`. This is what makes a polling change feed safe next to any writer, including the executor and including this same process in writer mode.
- **Protocol revision is `2025-06-18`, with `2025-03-26` and `2024-11-05` also accepted.** `KNOWN_VERSIONS` in `serve.rs`. Everything this plan adds — `resources/subscribe`, `notifications/resources/updated`, `notifications/resources/list_changed`, `logging/setLevel`, `notifications/message`, `notifications/progress`, `notifications/cancelled`, and `resource_link` content — exists in all three, so no capability in this plan is version-gated. Confirm that against the specification while implementing rather than trusting this sentence.
- **JSON-RPC batching is refused and stays refused.** `WireError::Batch` returns "batch requests are not supported", which is correct for `2025-06-18` — batching was removed. Do not add it.

## 0057 — Speaking at all

### The multiplexer

New file `crates/fsm-cli/src/mcp/notify.rs` (task `5701`):

```rust
pub struct Notifier { out: std::sync::Arc<std::sync::Mutex<Box<dyn std::io::Write + Send>>> }

impl Notifier {
    pub fn new(out: Box<dyn std::io::Write + Send>) -> Self;
    pub fn clone_handle(&self) -> Notifier;
    pub fn send(&self, message: &Value) -> std::io::Result<()>;
    pub fn notify(&self, method: &str, params: Value) -> std::io::Result<()>;
}
```

- `send` takes the mutex, writes `canon_bytes(message)`, writes `\n`, flushes, and releases. **The lock is held across the whole line and the flush** — that is the entire correctness argument, and a partial write outside the lock would corrupt the stream.
- A poisoned mutex is recovered with `into_inner()` rather than panicking: a panicking notifier would take down a server whose protocol state is otherwise fine, and the existing panic hook already aborts on real bugs.
- A write error is **not** fatal to the background thread. `stdout` closing means the client is gone; the thread records it and exits its loop, and the main loop discovers EOF on its own.
- The request path in `serve.rs` writes through the same `Notifier`, replacing the direct `send_line(&mut output, ..)` calls. There is exactly one writer type in the process after this task, and `send_line` becomes its private implementation.

**Why a mutex and not a channel.** A channel plus a dedicated writer thread would also work and is worse here: it adds a second failure mode (a full or disconnected channel) and it decouples "the response was produced" from "the response was written", which matters because the golden transcripts compare bytes in order. A mutex held for the duration of one small write is simple, correct, and has no queue to reason about.

### Two files, two owners

Before any feature lands, two shared files get a single owner each, or five later tasks queue behind them for the length of the plan:

- **`mcp/mod.rs` and the module shells belong to `5701`.** It creates `notify.rs` in full plus `subscribe.rs`, `watch.rs`, `logging.rs`, `progress.rs`, and `cancel.rs` as skeletons with `unimplemented!()` bodies, and declares all six. This is plan 0008's `3601` pattern, and it exists because a module cannot be declared without its file.
- **`serve.rs`'s method routing belongs to `5702`.** Every arm this plan adds — `resources/subscribe`, `resources/unsubscribe`, `logging/setLevel`, and the `notifications/cancelled` arm — is wired there, pointing at shells. `5901`, `6001`, and `6003` fill bodies and never touch `serve.rs`.

`5702` also introduces the **`ToolCtx` seam**. `dispatch(store, clock, name, args)` cannot see a request's `params`, so it can reach neither `_meta.progressToken` nor a cancellation flag; `5702` changes the signature to carry a `ToolCtx<'_>` holding the `Notifier`, the request id, and `_meta`, threaded from the `tools/call` arm and unused until `6002` and `6003` consume it.

### Capabilities and `initialize`

Task `5702` rewrites `initialize_result` in `crates/fsm-cli/src/mcp/serve.rs`:

```json
"capabilities": {
  "tools":     { "listChanged": false },
  "resources": { "subscribe": true, "listChanged": true },
  "prompts":   { "listChanged": false },
  "logging":   {}
}
```

`tools.listChanged` stays `false` deliberately: the tool set is static, a per-machine tool surface would make `tools/list` depend on store contents, and no client is obliged to re-read it. `prompts.listChanged` stays `false` for the same reason.

This moves `crates/fsm-cli/tests/mcp_lifecycle.rs`'s byte-compared `initialize` golden, and updating that golden is part of **this** task and no other. The instructions string is untouched here — `6103` owns the one sentence that describes the live surface, so the transcript moves once in this plan rather than twice.

### Shutdown

Task `5703`. The background thread must not outlive the session:

- `serve_session_with` owns a `ChangeFeed` handle carrying an `Arc<AtomicBool>` stop flag and the `JoinHandle`.
- On `Line::Eof`, on a fatal write error, and on any early return, set the flag, then `join()`. The feed's poll loop checks the flag every iteration and between sleeps in short slices, so a shutdown never waits a full poll interval.
- The thread is spawned **only** when the session actually subscribes to something. A server nobody subscribes to spawns nothing, does no I/O between requests, and behaves exactly as it does today — which keeps every existing non-subscribing golden byte-identical and is the cheapest way to guarantee this plan is inert for callers that do not use it.

## 0058 — Addressable instances

Task `5801` extends `crates/fsm-cli/src/mcp/resources.rs`:

| URI | Content |
|---|---|
| `fsm://instance/{id}` | the `instance_view` structured object as `application/json` |
| `fsm://instance/{id}/history` | the first history page as `application/json` |

- Both join `resources/templates/list` beside the existing `fsm://machine/{id}` template, with `name`, `title`, and `mimeType`.
- `resources/list` lists **machines** as it does today, plus the most recent instances up to the same cap of 50, most-recent-first by the seq of their `instance_created` record — reusing the sort the machine listing already does rather than inventing a second ordering.
- A `resources/read` of an unknown or malformed instance URI returns the existing `-32002` "Resource not found", which the current code already does for unknown URIs.
- The history URI serves a **page**, not the whole journal: the same default limit `instance_history` uses, with a note in the resource description pointing at the tool for paging. A resource that could return an unbounded journal is a resource that will one day return an unbounded journal.

Task `5802` adds `resource_link` content to tool results. `tool_ok` in `serve.rs` currently returns `content: [{type: "text", ...}]` plus `structuredContent`. It gains a third element for the tools that produce or touch exactly one instance — `instance_create`, `instance_send`, `deadline_poll`, `effect_ack`, `instance_cancel`, `instance_get` — of the form `{"type": "resource_link", "uri": "fsm://instance/<id>", "name": "<id>", "mimeType": "application/json"}`.

**This changes the shape of every one of those tools' results**, so the golden transcripts in `mcp_full.rs` move, and updating them belongs to this task. `structuredContent` is untouched, which is what keeps `review_regressions/cli_mcp_parity.rs` passing: the CLI's `--json` output is compared against `structuredContent`, not against `content`.

## 0059 — Subscriptions and the change feed

### The registry

New file `crates/fsm-cli/src/mcp/subscribe.rs` (task `5901`):

- `resources/subscribe` and `resources/unsubscribe` take `{uri}` and are per-session, held in a `BTreeSet<String>` behind an `Arc<Mutex<..>>` shared with the feed thread. Both return an empty result object on success.
- Subscribing to a URI the server does not serve is `-32002`, the same code `resources/read` uses. Subscribing twice is idempotent and succeeds. Unsubscribing something not subscribed succeeds — the client's intent is satisfied either way, and an error would only invite retry loops.
- A cap of `MAX_SUBSCRIPTIONS = 64` per session, refused with `INVALID_PARAMS` and a hint naming the cap. An unbounded set is an unbounded per-poll cost.
- The first successful subscription starts the feed thread (§0057's rule).

### The feed

New file `crates/fsm-cli/src/mcp/watch.rs` (task `5902`):

- A thread running `loop { if stop { break } ; poll() ; sleep_in_slices(interval) }` with `interval` defaulting to **250 ms**, matching the executor's default so the two processes have one cadence to explain.
- `poll()` opens `Store::open_read_only(data_dir)`, reads `journal.last_seq`, and returns immediately if it is unchanged. This is the common case and must be cheap: one open, one integer comparison, no view rendering and no `enabled_events` scan — the exact discipline plan 0008 imposed on the executor's watcher, for the same reason.
- When `last_seq` advanced, walk **only the new records** and map each to the URIs it affects: any record carrying an `instance_id` affects `fsm://instance/{id}` and `fsm://instance/{id}/history`; a `machine_defined` affects `fsm://machine/{id}`. Emit one `notifications/resources/updated` per **subscribed** URI that appears, de-duplicated within the batch — ten records for one instance produce one notification, not ten.
- The feed holds a watermark of the last seq it reported so a reconnecting or slow poll never re-notifies, and it never reads records it has already walked.
- In `ServeMode::Writer` and `Embedded`, this process holds the writer lock; the feed's read-only open coexists with it, and a notification for a change **this session just made** is expected and correct — a client that subscribed asked to be told, regardless of who caused it.

### List changed

Task `5903`: `notifications/resources/list_changed` is emitted when the feed sees a `machine_defined` or an `instance_created` in the new records, at most once per poll batch regardless of how many appeared. It is not tied to any subscription — `listChanged` is a capability, not a per-resource subscription — and a session that never lists resources simply ignores it.

## 0060 — Logging, progress, and cancellation

**Logging (task `6001`, `crates/fsm-cli/src/mcp/logging.rs`).** `logging/setLevel` takes `{level}` from the eight RFC-5424 names the specification uses and stores it per session, defaulting to `info`. `notifications/message` carries `{level, logger, data}`. Three producers are wired: the mode/startup line, store warnings that today reach stderr, and — the one that matters — **the embedded executor's tick lines**, which `drive_executor` currently writes to stderr where no client can see them. Keep writing them to stderr too; an operator reading a terminal must not lose them because a client is attached. Below-threshold messages are dropped before serialization, not after.

**Progress (task `6002`, `crates/fsm-cli/src/mcp/progress.rs`).** When a request's `params._meta.progressToken` is present, the dispatcher builds a `ProgressReporter` carrying the token and the `Notifier`; without one it builds a reporter that discards. `notifications/progress` carries `{progressToken, progress, total?, message?}`. Wire it into the two calls that can genuinely take time today — `simulate` with a long event list (one report per event) and `instance_history` with a large page (one report per chunk) — and leave the reporter available for plan 0014's `journal_verify`, which is the real consumer. Reports are rate-limited to at most one per 100 ms of wall time **and** always include the final one, so a fast call emits one report rather than a thousand.

**Cancellation (task `6003`, `crates/fsm-cli/src/mcp/cancel.rs`).** A `BTreeSet<RequestId>` of cancelled ids, populated from `notifications/cancelled`, replacing the stderr line that discards it today. Two effects, and the plan claims exactly these two:

1. **Before dispatch**, a request whose id is already in the set is not executed. This is reachable and useful: a client can cancel request 7 while the server is still working on request 6.
2. **At coarse loop boundaries**, a `CancelFlag` is checked — between events in `simulate`, between chunks in `instance_history`, and between records in plan 0014's verify. A cancelled call returns a tool error with a `req/cancelled` code rather than a JSON-RPC error, because the call was dispatched and the result is a tool outcome.

**A single `step` is not interruptible and this plan does not pretend otherwise.** Engine operations are bounded by the evaluation budget and are short by construction; threading a cancellation token through the pure core would cost the core its purity and buy nothing. `6103` documents that limit in the same paragraph that advertises the capability, and the honest sentence is the point.

Per the specification, the server sends **no response** to a request it did not execute because of a cancellation, and never a response to the `notifications/cancelled` itself. Pin that in the golden.

## 0061 — Proof and docs

**Golden transcripts (task `6101`).** `crates/fsm-cli/tests/mcp_live_golden.rs` drives a full live session against a temp store with a `FixedClock` and byte-compares the whole stream: initialize with the new capabilities → subscribe to an instance → a write that advances that instance → the `notifications/resources/updated` line → `resources/read` of the instance URI → `logging/setLevel` → a call carrying a `progressToken` and its progress lines → unsubscribe → EOF. The feed is driven by an **injected poll trigger** rather than by sleeping, so the golden is deterministic and the suite does not spend wall time; the real interval is exercised separately by one timing-tolerant test that asserts a notification arrives at all, never when.

**Ordering and interleaving (task `6102`).** The property that matters: **no notification's bytes ever appear inside another message's line.** Drive many concurrent notifications against a response-producing loop and assert every line in the output stream parses as a complete JSON-RPC message and that the multiset of messages is exactly what was produced. Plus: no notification for an unsubscribed URI; exactly one notification for a batch of ten records touching one instance; no re-notification of a seq already reported; the thread exits within one poll interval of EOF; and a closed stdout ends the thread without a panic.

**Docs (task `6103`).** `docs/EMBEDDING.md` gains a *Watching a store live* section: what the server pushes, the poll interval and how to think about latency, the subscription cap, the per-session nature of subscriptions, and the honest cancellation limit. `README.md`'s read-only pairing paragraph is corrected — it currently implies live watching that does not exist, and after this plan it can say what is true. The MCP `instructions` string gains one sentence telling a model it may subscribe rather than poll, which is the whole point of the plan reaching the audience that will use it.
