# Architecture — Plan 0020

> One store owner, independent execution progress, bounded transport work.

## Implementer orientation

1. Read `CONTRIBUTING.md`, the task, and the relevant sections of this file.
2. Treat the task's **Tests:** as its acceptance inventory, not as evidence
   that those tests have already run.
3. Stay within `touches`; update normative and operator contracts in the
   same commit as the corresponding observable behavior.
4. Preserve the pure core, single-writer lock, stable request IDs, record
   formats, hash domains, and exactly-once journaling. New concurrency
   orders independent requests by actual admission; it does not promise
   identical interleaving for different arrival schedules.
5. Run the stable host gate in `CONTRIBUTING.md` for code changes, including
   all-target clippy, zero-dependency and embed acceptance checks; record
   native platform results separately from cross-compilation.
6. Follow the high-risk proof path if an implementation changes locking,
   durability, or idempotency, even though this design requires no new
   persistent format.

## Starting points

- `mcp/serve.rs` owns the stdio request loop, `ExecutorLoop`, and the
  request-following `drive_executor` call.
- `fsm-execute/src/service.rs` separates observation, preparation, and
  settlement, but currently invokes `Runner` directly; `Runner::kill`
  waits for a child, and result collection performs I/O.
- `http/mod.rs` currently discards the embedded executor after opening the
  store; `http/writer.rs` serializes endpoint calls through a mutex.
- `mcp/methods.rs`, `notify.rs`, and `elicit.rs` mix request dispatch,
  transport output, and client-response waits; an owner must never remain
  parked in one of those waits.
- `mcp/executor.rs` and SPEC's **MCP executor discovery** section report
  `fsm.executor/1` and `progress: "client_requests"` for embedded mode.

## 0089 — The execution host

### The owner and its commands (`8901`)

Add a private `mcp/host/` module with an owned `Store`, clock, executor
state, and typed command receiver. Transport threads submit owned
requests; none receives `&mut Store`. One owner applies complete store
operations and captures immutable response values before another
operation can observe them. A state transition, its response's sequence,
and its published durable-change marker describe the same committed
prefix. Do not put a second writer or a long-held shared-store mutex in a
handler worker.

The host accepts commands through a bounded mailbox and assigns admission
order there. Keep per-session FIFO among admitted commands; ordering across
sessions is admission order. Requests rejected before admission cause no
store operation. Each request has a stable session generation as well as
its JSON-RPC ID, so a response for an expired session cannot be delivered to
a replacement session using the same request ID.

Use named count and byte limits, initially 32 pending application commands
and 32 MiB of owned request bodies per host, with 8 commands and 16 MiB per
session; the existing 16 MiB stdio frame cap remains a separate parser
bound. Charge retained parsed payloads as well as wire copies, or eliminate
the copies before admission. Reserve separately bounded control capacity
for stop, transport close, and cancellation, so application saturation
cannot prevent shutdown. A rejected stdio request receives the existing
JSON-RPC server-busy code `-32004`; an HTTP request rejected before RPC
admission receives 503. Notifications have no RPC response and follow the
specified transport-close policy if their control admission fails.

### Handler work outside the owner (`8902`)

Keep `Watcher`, `Scheduler`, and `Pipeline` as the decision and journal
layers. Submit starts and stops to a bounded supervisor; receive immutable
completion events identified by effect and attempt generation. The owner
never calls a blocking child wait, MCP conversation, capture drain, or
transport write. The supervisor does not own a writer, allocate journal
request IDs, or infer domain events.

Admission reserves an execution slot before dispatch; inability to dispatch
does not journal a successful start or lose the effect. Completion and stop
acknowledgement consume that slot exactly once. Ignore duplicate or stale
completion generations without applying an outcome to a newer attempt.
Backpressure retains one bounded completion per admitted attempt until the
owner settles it; silently dropping a completion is forbidden. Preserve
the existing attempt, retry, ack, and advance order, including the existing
recovery-window filtering.

Plan 0022 owns the supervisor's process lifecycle guarantee. This plan owns
the nonblocking integration boundary and its scheduling/settlement proof.
Respect its durable run claim before launch: an uncertain cleanup retains
the claim and slot, reports the blocked lifecycle state, and is never
translated into a successful stop or an acknowledged failure. Host shutdown
leaves interrupted effects pending according to that lifecycle contract.
If a new `fsm-execute` public item is necessary, update its provisional
inventory in the same task; do not expose the CLI host as a supported
library API accidentally.

### Timed and fair progress (`8903`)

Drive the host with `recv_timeout` or an equivalent standard-library wait.
It wakes on admitted commands, supervisor completions, stop controls, and
its next executor poll. Sample the injected logical clock once for each
executor decision pass; derive retry eligibility and deadline polling from
that value using the existing scheduler. A monotonic wait clock controls
sleep only and never supplies a journal timestamp. A fixed logical clock
must not advance merely because real time passes.

Use the configured executor poll interval as the maximum idle recheck
interval; expose the same validated interval through embedded serve if
not already accepted there. Due work triggers another pass without waiting
for client input. Bound each command and completion batch and alternate
ready classes: after at most eight admitted application commands, service
ready executor work; after at most eight completions, service an admitted
application command. Existing scheduler fairness and in-flight caps remain
authoritative within execution. Test the bound in owner turns, not a
fragile wall-clock percentile. An idle host sleeps, and an unchanged
observation does not append `deadline_not_due` records continuously.

### Session channels and client waits (`8904`)

Session adapters own output and interactive continuations. Split tool
preparation from a committing operation where elicitation or long-running
read-only diagnostics would otherwise hold the owner. An elicitation reply
resumes the original request and revalidates any expected instance sequence
before mutation; intervening executor progress is not overwritten. Existing
cancellation semantics remain scoped to that request, and cancellation of
a client request is not cancellation of a durable workflow it already
created.

The owner publishes immutable messages to bounded per-session egress
queues, initially 64 messages and 32 MiB, with an explicit encoded-message
cap of 16 MiB checked without first constructing an unbounded duplicate.
Keep HTTP's tighter existing SSE replay bounds. Responses are never
silently dropped: if an admitted request commits but its session cannot
receive the response, close that session and retain journal idempotency so
reissuing the same request ID resolves the outcome. Coalesce resource
invalidation notifications by URI only where the resource protocol already
permits it. Do not coalesce responses, elicitation IDs, or completion
results. Notification publication occurs after fsync; on the originating
session enqueue its mutation response before notifications caused by that
mutation, and preserve commit order for subsequent asynchronous changes.

No socket or stdout write runs on the owner. A saturated or broken HTTP
session is detached while the host continues. A broken or saturated stdio
session requests host shutdown because that session is the host's lifetime
boundary. A stuck output adapter must not keep the writer owned after the
supervised host shutdown bound; use the process-entry I/O boundary and the
lifecycle mechanism from plan 0022, rather than claiming arbitrary blocking
`Read`/`Write` objects can be interrupted portably.

## 0090 — Transport integration and the public contract

### Stdio (`9001`)

The process entry owns transport I/O and starts the host once. An open,
quiet stdin means the session remains alive: a workflow proceeds while no
new lines arrive. EOF and broken output stop admission and enter supervised
host shutdown; EOF does not mean "run every workflow to completion" and
does not cancel durable instances. Reopening the same data directory
recovers unfinished work under the existing at-least-once contract.

Keep deterministic borrowed-input test helpers explicit rather than
silently giving every helper a background thread with a `'static` lifetime
requirement. Tests claiming production autonomy must launch the real
binary, keep stdin open, and refrain from sending polling requests.

### HTTP (`9002`)

`run_http` retains `ServeMode::Embedded` and constructs one host shared by
all sessions. POSTs route through its mailbox; session creation does not
construct an executor. DELETE, expiry, a closed SSE stream, and the absence
of any clients remove session state only. Execution continues until the
HTTP host itself stops. New sessions see the current committed state and
the same loaded handler contract.

The same mode-selection policy applies to both transports. A successful
read-only open or contention fallback has no execution supervisor, refreshes
its journal prefix before requests, and never starts a handler; a degraded
host serves diagnostics without effects. Tests must exercise production
startup and the low-level public session helpers that previously needed
the defensive read-only guard.

### Discovery and compatibility (`9003`)

Do not extend the closed `fsm.executor/1` progress enum silently: publish
`fsm.executor/2` with `progress: "autonomous"` for an active embedded host,
retaining `manual`, `external`, and `unavailable` for their existing modes.
The resource URI and sanitized handler fields remain stable; older clients
receive an explicit new format discriminator instead of a misleading v1
value. Document this wire-version and pre-1.0 semver consequence in
`API-POLICY.md` and release notes before shipping; supported compatibility
tests pin rejection of unknown formats rather than treating them as v1.
The first production adapter enabling autonomy publishes this versioned
metadata in the same commit; the final contract task verifies and reconciles
all surfaces rather than leaving an intermediate release misdescribed.

Initialization instructions, prompts, tool descriptions, README, EMBEDDING,
and goldens must agree: polling observes progress and no longer causes it.
Subscriptions are optional observations; the client does not have to stay
chatty for recovery to run. Keep `external_executor: "unknown"` and all
command/template secrecy guarantees. Reference the shutdown limits from
plan 0022 and operational evidence from plan 0023 without claiming either
has been completed by writing this plan.

## Integration proof

Unit tests inject both logical and wait clocks and a controlled supervisor;
private host integration tests live under `mcp/host/tests/` behind
`cfg(test)`, so access to the actual host does not require a new public API;
`fsm-execute` integration tests prove the supervisor boundary separately.
Real-binary integration tests use portable re-executed Rust fixtures with
observable start/release barriers and bounded watchdogs. Observe handler
markers and subscribed changes after submitting the workflow, then read
the journal to prove acknowledgements, outcome events, and integrity; do
not accidentally drive the claimed autonomy with repeated RPC requests.
Verify success, transient retry, due deadline, compensation, interrupted
advance recovery, quiet clients, slow output, concurrent clients, read-only
fallback, and degraded startup through their production entry points.

Existing store/crash/replay/golden tests must remain green, and the native
Linux, macOS, and Windows matrix on stable and MSRV is required before
release. This plan authors the work; its status files carry no execution or
integration claim until the coordinator records that evidence.
