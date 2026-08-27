# Embedding fsm as a library

The CLI and the MCP server are two front ends over the same engine. This page is
for the third consumer: a Rust program that drives the engine in process.

## The three crates

| Crate | What it is | Depend on it when |
|---|---|---|
| `fsm-core` | The engine. Pure: no I/O, no clock reads, no `HashMap`, no floats. Parses and compiles specs, steps instances, polls caller-timed deadlines, analyses machines, hashes state. | You keep your own persistence and supply timestamps. |
| `fsm-store` | The durable shell. Append-only hash-chained journal, fsync per record, snapshots, and wall-clock reads at mutation boundaries. | You want the journal as your store. |
| `fsm-execute` | The effect executor: watches a store's outbox, runs operator-configured handlers as subprocesses, acks outcomes, polls deadlines. | You are building your own executor host and want the loop rather than the binary. |
| `fsm-cli` | The `fsm` binary: CLI plus MCP server. | You are a host, not an embedder. |

`fsm-core` and `fsm-store` are supported embedding targets and are covered by
the release acceptance criteria. `fsm-execute` is a library too, but its surface
is younger than theirs — see [API-POLICY.md](API-POLICY.md) before depending on
it. `fsm-cli` is a binary crate; do not depend on it as a library — `fsm-store`
exists so you do not have to.

See [API-POLICY.md](API-POLICY.md) for what "supported" commits us to, and for
how to pin a version.

## Stage 1: the core loop

No store, no clock, no filesystem. `crates/fsm-embed-acceptance` is this loop as
compiling, tested code; it depends on `fsm-core` alone and its tests run in CI,
so it cannot drift from the real API.

```rust
use fsm_core::json::{parse, JsonLimits, Value};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;
use fsm_core::step::{create, poll_deadline, step, DeadlineOutcome, Outcome};
use fsm_core::expr::eval::Budget;
use fsm_core::limits::MAX_EVAL_TICKS;

// 1. Parse and compile. `machine_id` is a hash of the canonical definition.
let def = parse(spec_bytes, &JsonLimits::DEFAULT)?;
let compiled = compile_accepted(&def)?;          // Err = Vec<Finding>, each with a path and a hint
let tree = Tree::for_machine(&compiled.spec);

// 2. Create an instance. `create` enters every initial region and schedules
//    its deadlines relative to the caller-supplied timestamp.
let applied = create(&compiled, &tree, &overrides, created_at_ms)?;

// 3. Step. Pure: `st` is not mutated, nothing is committed. The timestamp is
//    used only when newly entered states schedule deadlines.
let mut budget = Budget::new(MAX_EVAL_TICKS);
match step(&compiled, &tree, &st, "docs_ok", &payload, event_at_ms, &mut budget) {
    Outcome::Applied(a) => { /* persist a.configuration_after / a.deadlines_after, then run a.effects */ }
    Outcome::Ignored    => { /* the machine declares this event ignorable here */ }
    Outcome::Rejected(r) => { /* r.code is a namespaced code, e.g. run/unhandled */ }
}

// 4. Poll explicitly. At most one due deadline is selected by
//    (due_ms, definition order); an early poll does not change `st`.
let mut budget = Budget::new(MAX_EVAL_TICKS);
match poll_deadline(&compiled, &tree, &st, polled_at_ms, &mut budget) {
    DeadlineOutcome::Applied(a) => { /* persist a.transition, then run its effects */ }
    DeadlineOutcome::NotDue { next } => { /* schedule the host's next wake-up from `next` */ }
    DeadlineOutcome::Rejected(r) => { /* same structured run errors as an event step */ }
}
```

Compilation accepts at most `MAX_EVAL_TICKS` worst-case evaluation ticks across
the machine: every compiled AST node, plus one tick per distinct event with an
omitted guard. A create, step, deadline poll, or enabled-event scan can visit
each compiled expression slot at most once. A step can evaluate at most one
omitted guard because it immediately wins selection; an enabled-event scan can
evaluate one for each affected event. A fresh standard budget therefore cannot
produce `internal/budget` for an accepted definition.
Supplying a smaller or already-consumed budget remains an embedder policy
choice.

There is no implicit primary leaf. `InstanceState::configuration` is either
`ActiveConfiguration::Sequential { leaf }` or
`ActiveConfiguration::Parallel { leaves }`, where `leaves` is a deterministic
`BTreeMap<region, leaf>`. One event or one due deadline changes at most one
region. A parallel instance completes only when every active regional leaf is
terminal.

Deadlines are definition-owned timed transitions. Entering their source state
stores an absolute due timestamp in `InstanceState::deadlines`; leaving the
source cancels it. `after` expressions see context but not event payload. The
core never reads time and never wakes itself: the caller passes `now_ms` to
`create`, `step`, and `poll_deadline`, and decides when to call again.

Also available without a store: `analyze::analyze_all` (unreachable states,
shadowed transitions, guards that can never hold), `analyze::completeness_matrix`
(which `(leaf, event)` pairs are handled), `analyze::enabled_events`,
`simulate::simulate`, `diagram::{mermaid, dot}`, and `hashes::state_hash`.
`simulate` returns `Result<SimReport, Rejection>`: an `Ok` report always
descends from a real sequential or parallel creation, while a failed creation
is the typed `Err` and never a sentinel report.

### Persisting instance state yourself

`InstanceState` is a plain struct, but its context values are typed (`Val`), so
encoding them is where a hand-rolled implementation drifts. Use the pair in
`fsm_core::replay`:

| Direction | Function | Form |
|---|---|---|
| write | `ctx_val_string(&Val) -> String` | always a string |
| read | `parse_ctx_val(&TySpec, &str) -> Option<Val>` | exact inverse, per declared type |

These two are inverses for every declared type, including enums (written
qualified, as `Tier.premium`) and decimals (scale preserved). There is a second,
**non-invertible** pair for API output — `ctx_val_json` / `parse_ctx_json`, which
renders booleans as JSON booleans. Do not cross the pairs: `ctx_val_json` output
read back with `parse_ctx_val` loses booleans. `crates/fsm-core/tests/ctx_roundtrip.rs`
pins both laws.

To detect drift between your store and the engine, keep
`hashes::state_hash(machine_id, instance_id, seq, &state)` alongside your row and
compare it after every load. The acceptance test does exactly this.

Because `InstanceState` is public, a decoder can construct combinations that
no engine transition could produce. After decoding, call
`tree.validate_instance_state(&compiled, &state)`. It verifies the complete
sequential or parallel leaf set, running/completed terminal coherence, every
deep or shallow history binding (owner, ancestry, and binding shape), and the
exact deadline names required by the active nonterminal chains. `step` and
`poll_deadline` perform the same check at their boundary and reject an invalid
state with `run/configuration_invalid`; they never repair or partially apply
it. `StateValidationError::detail()` is diagnostic text, not a stable value to
branch on — runtime callers should branch on the rejection code.

The decoder must still enforce its own row schema and context types. The
`fsm-embed-acceptance::from_row` example accepts exactly the fields emitted by
its `to_row`, rejects unknown top-level and configuration fields, reads every
declared context value with `parse_ctx_val`, and then invokes the shared state
validator.

Persist the whole tagged configuration and the whole deadline schedule; do not
flatten a parallel configuration to a convenient “current leaf.” A direct JSON
shape is:

```json
{
  "configuration": {
    "kind": "parallel",
    "leaves": {"audit": "checking", "work": "waiting"}
  },
  "deadlines": {"work_timeout": 1750000000123}
}
```

Sequential rows use `{"kind":"sequential","leaf":"intake"}`. Deadline
values are signed 64-bit millisecond timestamps. The embedding acceptance crate
serializes, reloads, hashes, polls, and continues a parallel timed instance so
these fields cannot silently fall out of the public loop.

## Stage 2: using the journal as your store

`fsm_store::store::Store` gives you the durable, auditable version: a total
order, hash-chained records, idempotent requests, and replay.

```rust
use fsm_store::store::Store;

let mut store = Store::open(&data_dir)?;         // folds the journal (or a snapshot)
store.define_machine(def, /*dry_run*/ false, /*if_exists_error*/ false)?;
store.create_instance("case_review", "i1", "req-1", None)?;
store.send_event("i1", "docs_ok", payload, "req-2", None)?;
store.poll_instance_deadline("i1", "req-3", None)?;

let inspected = Store::open_read_only(&data_dir)?; // one verified journal prefix
```

The `_on` mutation variants accept an `&mut dyn fsm_store::clock::Clock`.
Existing custom clocks may continue to implement only `now_ms`: the provided
`reserve_ms` method consumes that timestamp immediately and
`commit_reserved_ms` returns it without consuming another. A clock that wants
an abandoned stamped request to leave its own state unchanged must override
both hooks: reserve without advancing, then advance once and return the same
timestamp from commit. `GlobalClock` and `FixedClock` implement that deferred
behavior. The store uses it to measure the final post-stamp payload before
mutating the caller's value or advancing either built-in injected clock.

### Concurrency contract

Every `Store` method is **synchronous and blocking**, and a store is a
**single-writer** resource:

- one process at a time — `Open` takes a process-wide advisory lock on
  `<data_dir>/journal/LOCK`; a second opener gets `store/lock`;
- one writer at a time — `&mut self` on every mutating call;
- every append `fsync`s before returning;
- `Store::open` folds the whole journal, or a snapshot plus the tail.

`Store::open_read_only` is the inspection path. It creates no directory or
file, takes no advisory lock, does not migrate or stamp `VERSION`, and never
writes a snapshot (including on drop). It can coexist with the writer and
returns one internally consistent, hash-verified journal prefix; records
appended after that read are visible on the next open. Calling a mutator on a
read-only `Store` is refused as `io/write`. An unterminated line at the end of
the final segment is omitted from this inspection prefix because it may be a
writer's in-progress append; strict open and verification still classify it as
`TornTail`. The CLI's machine and instance views, analysis, diagrams,
simulation, explanation, journal replay/verify, and `doctor` follow the same
non-mutating inspection contract.

Each persistence unit read as a whole uses the parser's 16 MiB default byte
ceiling. Exactly 16 MiB is admitted to parsing or record verification; an
oversized `VERSION` or individual journal record is a fatal `io/read`, while
journal segments are streamed one bounded record at a time. A direct journal
append over the same per-record ceiling is refused as `io/write` before
rotation or persistence. Oversized snapshots are skipped as disposable caches
on read and refused before a snapshot writer changes the cache.

There is no async API and no interior locking. On Tokio, **own the `Store` from
one dedicated blocking thread** and send commands to it over a channel — a writer
actor. Do not put it behind a `Mutex` shared across tasks: you would serialise
anyway, but on the async executor's threads.

### Measured cost

From `crates/fsm-store/tests/append_latency.rs`, release build, three 2000-iteration
samples on an AMD Ryzen 7 PRO 8700GE, ext4 on a two-device NVMe RAID1. Each
cell is the median of the three run-level values:

| Operation | p50 | p95 | p99 | throughput |
|---|---|---|---|---|
| `create_instance` | 4 488 µs | 4 676 µs | 5 080 µs | ~227/s |
| `send_event` | 4 523 µs | 4 726 µs | 4 926 µs | ~226/s |

Open cost, 4001 records / 2000 instances: **236.4 ms** full fold
(~59.1 µs/record).

Two caveats worth sizing around:

- These are one storage stack's fsync numbers. Re-run the harness on the exact
  persistence filesystem you intend to use; the root must already exist:
  `FSM_BENCH_ROOT=/path/on/filesystem-under-test cargo +stable test --release -p fsm-store --test append_latency -- --ignored --nocapture`.
- A snapshot did **not** beat the full fold at this shape (~272.7 ms vs
  ~236.4 ms):
  restoring 2000 instances means recompiling their machines and re-verifying
  every state hash. Snapshots pay off when records greatly outnumber instances.
  Measure before assuming they help you.

A single writer at ~226 sends/s is the throughput ceiling on this measured
storage stack, and it is a
deliberate one. If your driver count implies more, shard by data directory —
there is no HA, replication, or multi-writer story, by design.

## Contracts an embedder should know

### `request_id` is an idempotency key over content

Every journaled instance request takes a `request_id`, including a deadline
poll that finds nothing due. Resending it with the **same** content
replays the original outcome with `duplicate: true`. Resending it with different
content is `req/request_id_conflict` — never a replay of the unrelated outcome.

Derive ids **per attempt**, not per `(task, event)`. If you derive them from the
task, your second event reuses the first event's key and gets a conflict; you
want a retry of *that* request to replay, and anything else to be a new key.
`expect_seq` is excluded from the fingerprint, so refreshing it across a retry is
still a retry. Full rules in [SPEC.md](SPEC.md#idempotency).

### Machine versions are pinned per instance

`machine_id` is a content hash, so definitions are immutable. Adding a changed
spec under the same name creates a **second** machine, and in-flight instances
**stay pinned to the definition they started with** and keep stepping. This is a
guarantee, not incidental behaviour.

The consequence for shipped releases: an upgrade mid-run leaves running
instances on the old definition indefinitely. There is no automatic migration and
none is planned. Detect it yourself — the pinned `machine_id` is on every
instance view — and drain or cancel on your own schedule.

### Effect acks never drive transitions

`effect_ack` clears a pending effect. It does not fire a transition, and
`outcome: "failed"` is no exception: acking a failure leaves the instance exactly
where it was. This keeps the engine's one-event-one-transition rule intact —
acks are bookkeeping, events are causal.

To act on a failure, send an explicit domain event (`gate_failed`, `retry`,
`abandon`) and let a guarded transition decide. Model the failure in the machine,
not in the ack.

### Payloads are journalled forever

Event payloads, ack `result`s, and annotation notes are written to the journal
verbatim and never rewritten, so their cost is permanent and is re-paid on every
fold, snapshot, and verify. Anything over 64 KiB canonical
(`fsm_core::limits::MAX_PAYLOAD_BYTES`) is refused with `req/payload_too_large`.

Journal a digest or an identifier and keep the blob in your own store. The check
runs before the request is applied and does not consume the `request_id`, so you
can correct the payload and resend under the same key.

### Regions and deadlines stay explicit

- A machine uses either the sequential `states` + `initial` form or the
  top-level `regions` form. Regional state names are globally unique, and a
  transition or deadline cannot cross a region boundary.
- An event scans live regions in definition order and applies exactly one
  winning transition. Other regional leaves remain unchanged.
- Deadlines do not turn the core into a scheduler. Your runtime wakes up and
  explicitly polls with a timestamp; one poll applies at most one due deadline.
  Poll again to drain another due schedule.
- There are still no hidden events and no hidden clock reads. Advancement is
  caused only by an explicit event step or deadline poll, both with caller-owned
  time in the pure core.

## Watching a store live

A model that wants to know when an instance advances has two options, and
until this existed only one of them worked: call `instance_get` in a loop, or
subscribe and be told. Subscribe.

Two resource URIs describe an instance:

- `fsm://instance/{id}` — the instance as `instance_get` reports it:
  configuration, context, enabled events, pending effects and deadlines, and
  its place in any invocation tree.
- `fsm://instance/{id}/history` — the first page of its records, at the same
  default limit `instance_history` uses. Page with the tool; a resource that
  could return an unbounded journal will one day return an unbounded journal.

`resources/subscribe` takes one `uri` and is scoped to the session that called
it: subscriptions are not shared between clients and do not outlive the
connection. One session may watch at most **64** URIs, which is a number
chosen to be far above what any real client watches and far below what would
turn one poll into a scan; the sixty-fifth is refused rather than silently
dropped, so a client that leaks subscriptions finds out.

A subscribed URI produces `notifications/resources/updated` when a journal
record touches its instance — any record: an applied event, a refusal, an
effect ack, a deadline poll, a migration. Separately, and whether or not you
subscribe to anything, `notifications/resources/list_changed` says that the
resource *listing* changed: a machine was defined, an instance was created, or
a child instance was invoked.

The feed behind both is a poll, not a watch on the filesystem. It runs every
**250 ms**, takes no lock, opens the store read-only, and returns after one
integer comparison when nothing has happened — so a subscriber costs a
quiescent server nothing measurable and perturbs a writer not at all. Budget
for a change to arrive within a poll interval of the write that caused it, and
do not build anything that depends on arriving sooner. A batch of records
touching one instance produces one notification, not one per record: the
notification says *something changed here*, and the resource read that follows
says what.

`logging/setLevel` selects a severity, from `emergency` through `debug`, and
the server sends `notifications/message` at or above it. The default is
`info`. In embedded executor mode this is also how executor ticks reach the
client: a tick that did work says so, and at `debug` a tick that did nothing
says that too.

A tool call that carries `_meta.progressToken` gets `notifications/progress`
as it runs. Two tools report: `simulate`, per rendered step, and
`instance_history`, per page chunk. Progress is rate-limited to one
notification per 100 ms with the final report always sent, so a token on a
short call costs one notification rather than a burst.

`notifications/cancelled` withdraws a request, and it does two things
honestly: a request whose id is already cancelled is never dispatched and
never answered, and a dispatched call stops at its next coarse loop boundary —
between simulated events, between history chunks — returning the tool error
`req/cancelled`. It does **not** interrupt work in progress inside a single
engine step: **a single tool call is not interruptible mid-step**. Engine
operations are bounded by the evaluation budget and are short by construction,
and threading a cancellation token through the pure core would cost the core
its purity and buy nothing. Cancel a `simulate` of a thousand events and it
stops within one event; cancel an `instance_send` and it completes.

## Affordances

Three protocol affordances this server offers are all cases where it already
holds the answer and used to keep it: what each tool is safe to do, what an
identifier can be spelled as, and what a person still has to decide.

### What the annotations claim

Every tool in `tools/list` carries a `title` and four hints. None of them is
declared beside the tool; each is derived from the code that already enforces
it, so a hint cannot drift from the behaviour it describes.

| Hint | Where it comes from |
|---|---|
| `readOnlyHint` | the negation of `MUTATING_TOOLS`, the same constant a read-only server refuses on |
| `destructiveHint` | true for `instance_cancel` only: cancelling ends an instance and no later event revives it |
| `idempotentHint` | true for exactly the mutating tools — see below |
| `openWorldHint` | false everywhere: no tool call reaches past one data directory. Effects reach the world; the executor runs them |

The read-only tools are therefore `machine_list`, `machine_get`,
`machine_analyze`, `machine_diagram`, `instance_get`, `instance_list`,
`instance_history`, `explain_step`, `journal_verify`, `journal_replay`, `store_doctor`, and `simulate`. The mutating ones are `machine_create`,
`instance_create`, `instance_send`, `deadline_poll`, `effect_ack`,
`instance_cancel`, `instance_migrate`, `invocation_start`,
`invocation_return`, `signal_deliver`, and `instance_elicit`.

**The idempotency claim is exact, and stronger than most servers can make.**
Every mutating tool requires a `request_id`. The store keys on the pair of
that id and a fingerprint of the request's content: retrying with identical
content replays the first outcome and returns `duplicate: true`, and reusing
the same key with **different** content is **refused rather than replayed**,
as `req/request_id_conflict`. That last clause is the one to understand
before auto-approving retries: a client that reuses a key for a new request
gets an error, never a silent second write and never a silently discarded
one. `machine_create` qualifies through content addressing — an identical
spec is the same machine — and `if_exists: "return_existing"` is its
idempotent form.

### What completes, and what does not

`completion/complete` answers two reference types, because the protocol
defines two. Resource template variables: the `{id}` in `fsm://machine/{id}`,
`fsm://instance/{id}`, and `fsm://instance/{id}/history`, offered
newest-first in the order `resources/list` shows them. And prompt arguments:
`instance_id` on `drive_instance` and `diagnose_instance`, and `event` on
`drive_instance`.

**Tool arguments do not complete.** The protocol has no reference type for
them, and inventing a private fourth one would be a message no client sends.

Matching is a case-sensitive prefix, because every identifier here is
case-sensitive and a suggestion that then fails validation is worse than
none. At most 100 values come back, with `total` counting the matches before
truncation and `hasMore` saying the rest exist.

The `event` completion is the one worth knowing about: when the request
carries `context.arguments.instance_id` — the resolved-argument context the
`2025-06-18` revision defines — the answer is that instance's own enabled
events, the same analysis `instance_get` reports. Without that context it
returns **empty by design**: guessing from the catalogue would offer events
that cannot fire against the instance in question, which is worse than
offering nothing.

### Asking a person: `instance_elicit`

A workflow at a human gate can ask. `instance_elicit(instance_id, event,
request_id)` derives a form from the event's declared fields, sends it as an
`elicitation/create` request, and — on an answer of `accept` — coerces the
content into a typed payload and sends the event down the ordinary
`instance_send` path with your `request_id`. There is no elicitation record
and no new record kind: what happened to the workflow is that an event
arrived.

Three limits, stated together because a caller needs all three:

- **The client must advertise `elicitation`** in its `initialize`
  capabilities. Without it the tool refuses and names `instance_send` as the
  direct path.
- **Nesting is capped at one.** A second ask while one is outstanding is
  refused as `req/elicit_nested` rather than queued.
- **There is a 300-second timeout.** After it the tool returns
  `req/elicit_timeout`.

A `decline`, a `cancel`, a timeout, a nesting refusal, and an answer that
fails coercion all do the same thing to the store: **nothing**. No record is
written and the `request_id` is not consumed, so it is still yours to use for
whatever you do instead. While an ask is outstanding the client keeps
working: its notifications are handled and its requests are answered, though
the ones that need the store are told to retry, since the tool that asked is
holding it.

**The server still never parses natural language.** That is what makes this
feature compatible with the oldest rule in this project rather than an
exception to it: the request carries a schema derived from typed
declarations, the response is structured data, and every value is validated
against the same declarations an external `instance_send` is validated
against — a raw JSON number for a decimal is `req/number_token` here exactly
as it is there. An elicitation that returned prose for the server to
interpret is out of scope permanently, not merely for now.

## Executing workflows

Everything above leaves the *running* of effects to you. `fsm execute` is the
process that does it: it watches a store's outbox, runs an operator-configured
table of handlers as subprocesses, acknowledges each outcome into the journal,
and polls due deadlines — so a workflow triggered in a chat this morning
proceeds gate to gate this afternoon with nobody watching.

### The outbox contract, restated for operators

A transition emits a named effect into `effects_pending` and stops there. The
executor runs it and acks it, and **the ack does not transition anything**. The
instance moves only when a domain event the machine itself declares is sent, so
every advance the executor makes is an event your definition already allows —
named in the handler table, never improvised. An effect whose name has no
handler is a deliberate stall: the executor logs `exec/unhandled_effect` once
and takes no other action, because the alternative is guessing what to run
against the world's computers.

### The handler table: `fsm.handlers/1`

One operator-owned JSON file, read once at startup, before any store is opened.
It is the security boundary of the whole design: it closes the set of commands
the executor can ever run.

```json
{
  "format": "fsm.handlers/1",
  "handlers": [
    {
      "effect": "request_confirmation",
      "argv": ["/usr/local/bin/notify-supplier", "--order", "{order_id}", "--quiet"],
      "timeout_ms": 120000,
      "on_ok": { "event": "confirmed", "payload": {}, "stamps": ["at"] },
      "on_failed": { "event": "confirmation_failed", "payload": { "reason": "handler" } }
    }
  ]
}
```

| Field | Rule |
|---|---|
| `format` | exactly `fsm.handlers/1` |
| `effect` | a non-empty effect name the machine declares; unique across the table |
| `argv` | non-empty array of strings. `argv[0]` is the command and must be a **literal rooted path** — no `{placeholder}`, no bare name |
| `timeout_ms` | a whole number of milliseconds, `1` to `86400000`; the run is killed past it |
| `on_ok` / `on_failed` | optional. An object with a non-empty `event`, an optional `payload` **object** (default `{}`), and an optional `stamps` array of field names (default `[]`) the store fills from the clock |

Every other key is refused. A misspelled `on_okay` that validated would ack
effects and never advance, which at run time is indistinguishable from a
deliberately undeclared advance.

`{placeholder}` in any element after `argv[0]` is replaced by the effect
argument of that name, rendered in the same canonical form the engine persists
context with — an int is exact, a decimal keeps its scale, a string is
verbatim. **No shell is involved anywhere.** One template element always
produces exactly one argv element, so a value containing spaces, `;`, or
`$(…)` is one opaque argument; nothing re-splits it and no glob expands. A
placeholder naming an argument the emit did not produce is a run-time failure
of that effect — acked `failed` so the machine's own failure path can fire —
not a table error. There is no escape for a literal brace: values may contain
`{` and `}`, templates may not.

Handler output is captured and bounded. At most 4 KiB of each stream reaches
the journal; when the stream was longer the ack also carries a SHA-256 of the
whole thing, so a permanent record keeps a tamper-evident reference to output
it does not store. Bytes that are not valid UTF-8 survive as replacement
characters, and a character the cap cut in half is dropped rather than
rendered, so an ack never fails to journal because of what a handler printed.

Validate a table before pointing it at a store:

```console
$ fsm execute --check --handlers ./handlers.json
```

### Idempotency: why a restarted executor is safe

The executor never invents a `request_id`. Every key is derived from content
the journal already holds:

| Write | Key |
|---|---|
| ack | `exec-ack-{effect_id}` |
| advance event | `exec-ev-{effect_id}-{event}` |
| deadline poll | `exec-poll-{len}-{instance_id}-{deadline}-{due_ms}` |

The store keys idempotency on the pair `(request_id, request fingerprint)`, and
both halves derive from journaled state, so a restarted executor recomputes the
identical key for the identical intent and the store answers `duplicate: true`
instead of applying it twice. A key re-used for *different* content is refused
with `req/request_id_conflict` rather than replayed — that refusal is the
design working, and it is what makes derivation safe rather than merely
convenient. The due time is part of the poll key because a rescheduled deadline
is a new observation, and replaying the old key would answer with the old
poll's outcome.

### Three run modes, and which one you want

| Mode | Who writes acks | `fsm serve` while it runs | Use it for |
|---|---|---|---|
| `paired` (default) | the executor | read-only, for monitoring | the headline case: the model watches progress while the executor drives the workflow unattended |
| `embedded` | the serve process itself | holds the writer, runs handlers inline | one ad-hoc session, at a keyboard |
| `exclusive` | the executor | not running | unattended batch or CI where nothing else touches the store |

`paired` is the default. Start the MCP host read-only against the same data
directory and it can call `machine_list`, `machine_get`, `machine_analyze`,
`machine_diagram`, `instance_get`, `instance_list`, `instance_history`, and
`simulate` while the executor writes.

What it cannot do there is write. These eleven refuse with a message naming
the mode: `machine_create`, `instance_create`, `instance_send`,
`deadline_poll`, `effect_ack`, `instance_cancel`, `instance_migrate`,
`invocation_start`, `invocation_return`, `signal_deliver`,
`instance_elicit` — the last because an ask that could not send the answer
would waste a person's time. A `machine_create` with `dry_run`, and
an `instance_migrate` with `dry_run`, both still
answer, because checking a definition and asking what a migration would do are
reading, not writing.

That is the one real ergonomic price of a single-writer store, and it decides
the order you do things in: **author and trigger through a writer, then let the
executor run while the model watches.** Define the machine and send the trigger
event before starting the executor, or from a terminal (`fsm machine add`,
`fsm instance new`, `fsm instance send`) while it runs — those contend for one
tick at worst — or use `embedded` mode.

`embedded` (`fsm serve --execute --handlers ./handlers.json`) runs the same
loop on the serve thread. Two limits, stated rather than papered over: a
long-running handler blocks the protocol, and because the server blocks waiting
for the next client line, **a tick happens only when the client speaks**.
Embedded mode advances a workflow during a conversation, never overnight.

Each process announces its mode once on stderr, and a non-default mode says so
in the MCP `instructions` as well.

### What this does not promise

The guarantee, stated in the shape the design actually holds:
**at-least-once execution, exactly-once journaling.**

- **Single-node.** The executor is single-node and inherits the store's
  single-writer ceiling.
  There is no HA, no multi-writer coordination, and no distribution of handlers
  across machines.
- **At-least-once at the process boundary.** What the journal knows, a
  successor honours; what it does not, a successor repeats. An ack that was
  journaled but whose advance was lost is re-derived and replayed, never
  double-applied. A handler that was running when the executor was killed is
  re-run by the next one, because nothing in the journal says it ever started.
- **No rollback.** A handler that already reached the outside world is not
  undone by `fsm`. Model the undo as an explicit **compensating** effect the
  machine's failure path emits, and let the engine decide when it fires.
- A clean shutdown kills and reaps every handler it started. A signalled one —
  `kill -9`, or Ctrl-C — cannot: those children are orphaned and keep running,
  and the next executor starts fresh ones rather than adopting them.

### Executor error codes

These live under `exec/` and are the executor's own; they are not engine codes
and do not appear in SPEC.md's appendix.

| Code | Raised when |
|---|---|
| `exec/config` | the handler table is malformed, or an argv placeholder names an argument the emit did not produce |
| `exec/effect_unresolved` | a pending effect id whose name and args cannot be re-derived from the journal |
| `exec/unhandled_effect` | a pending effect with no handler — logged once, and nothing else happens |
| `exec/spawn` | the command could not be started |
| `exec/timeout` | a run was killed for passing its `timeout_ms` |
| `exec/cancelled` | a run was killed because its instance was cancelled |
| `exec/store` | a store operation failed; the original code is preserved in `details` |
| `exec/mode` | `--exclusive` found another writer holding the data directory |
| `exec/invoke` | creating a child or returning its result failed; the store's own code is preserved in `details` |
| `exec/signal` | delivering a signal failed; the store's own code is preserved in `details` |

### Migrating a cohort

A definition bug found on day thirty is fixed for the instances still
running, and the order of operations is the whole of the discipline:

1. **Preview.** `fsm migrate --from <old> --to <new> --dry-run` reads the
   store — it opens read-only, so a monitoring session can ask without
   holding the writer — and prints the cohort grouped by outcome.
2. **Read the refusals.** Each group names its code and the state
   responsible: "four are in `awaiting_countersign`, which your map does not
   cover". Decide whether to widen the mapping, which means a new definition
   and a new hash, or to accept the exclusion and leave those instances where
   they are.
3. **Migrate in batches.** `--limit N` moves N and stops, so a cohort can be
   watched rather than launched.
4. **Re-run after any interruption.** The command is **not atomic**: it is N
   idempotent operations, not a transaction, so a crash halfway leaves half
   the cohort migrated. Every `request_id` derives as
   `migrate-{instance_id}-{to_machine_id}` from content the journal already
   holds, so re-running re-derives the identical key and the store replays
   what it already did instead of migrating twice. Resumption is free; it is
   not a feature somebody has to remember to use.

The consequence to say out loud before step 3: **migration reschedules every
deadline from the migration instant.** A workflow whose timer was about to
fire gets a fresh one. That is the correct behaviour — an old due time would
be a promise the new definition never made — but it is not what an operator
expects unless somebody tells them.

### Composition without a human

The executor enacts composition the same way it enacts effects: from the
journal, with derived keys. Three directives join the tick, in this order
within one tick — invoke, then return, then signal — so a slot created and
settled across two ticks never races itself:

| Directive | When | Derived key |
|---|---|---|
| invoke a child | a slot is `pending` | `exec-inv-{parent}/{slot}` |
| return a result | a `running` slot's child has settled | `exec-ret-{parent}/{slot}` |
| deliver a signal | an instance holds an undelivered signal | `exec-sig-{sender}/{signal_id}` |

None of them spawns a subprocess. A handler exists to reach the world's
computers; these three reach only the journal, so they take the writer for
the tick and go straight to the store. A restarted executor recomputes each
key from journaled content and the store answers `duplicate: true` rather
than acting twice.

The run modes apply unchanged. In `paired` the executor writes and the model
reads; a composed workflow runs to completion with the model watching the
tree through `instance_get`. In `embedded` the server drives the same tick on
the handle it already holds, so a model can invoke, return, and deliver
itself through `invocation_start`, `invocation_return`, and `signal_deliver`.
In `exclusive` the executor holds the directory alone and every composition
tool refuses with the message naming the mode — which is the same trade as
every other write.

A returnable invocation is decided from the child's own status, never from
elapsed time: the watcher reports a slot as returnable only when the child is
`completed` or `cancelled`.

## Errors

Every error carries a namespaced `code`, a `message`, and a `hint` that states
the fix. Route on the namespace:

| Prefix | Meaning | Who fixes it |
|---|---|---|
| `def/`, `expr/` | the definition does not compile | the spec author |
| `req/` | the request is malformed or misaddressed | the caller |
| `run/` | the machine rejected creation, an event, or a deadline poll | the caller, or the machine |
| `store/`, `io/` | the store or the disk | the operator |

`req/seq_mismatch` and `req/payload_too_large` do not consume the `request_id`;
retry under the same key once corrected. `req/request_id_conflict` is never
retryable — use a new key. The full list is in [SPEC.md](SPEC.md#appendix-a--error-codes).
