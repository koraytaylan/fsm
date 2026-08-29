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
`invocation_return`, `signal_deliver`, `instance_elicit`, and
`instance_annotate`.

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

## Auditing a store

Five tools answer the question "is this store what it says it is, and why did
it do that". They read; none of them writes, and none of them repairs.

| Tool | What it proves | What it costs |
|---|---|---|
| `explain_step` | why one journaled step did what it did: every candidate transition, each guard's verdict, what every action computed, and how the invariants came out | one record, reconstructed |
| `journal_verify` | that the bytes and the hash chain are what they should be | a walk over the journal, at 256 records a batch |
| `journal_replay` | that the outcomes the journal recorded are the outcomes the engine produces **today** | a full fold through the engine |
| `store_doctor` | the state of the store: health, format, counts, snapshot staleness, and who holds the writer | one classification pass |
| `instance_annotate` | nothing — it *writes* a note into the trail, and changes no logical state | one record |

**`journal_verify` and `journal_replay` are not the same tool twice.**
Verification checks the bytes and the chain: that nothing was edited.
Replay re-executes the journal through the engine and checks that what was
recorded is what the engine produces now. A store can verify perfectly and
still fail replay — that is the engine's semantics having drifted, and it is
the one failure verification cannot see. Replay also reports a `state_root`,
which is what makes two runs, two machines, or a store and its backup
comparable at all.

### Reading a health

`journal_verify` and `store_doctor` report one of the seven names
`docs/SPEC.md` §Recovery defines, and never a new word for an existing
condition:

| Health | Posture |
|---|---|
| `Ok` | open |
| `TornTail` | refuse; remedy `fsm repair --truncate-torn-tail` |
| `ChainBroken` | refuse; interior; no repair |
| `StateHashMismatch` | refuse; no repair |
| `NonCanonical` | refuse; no repair |
| `LockIo` | refuse; a lock acquisition or contention fault |
| `StoreIo` | refuse; repair the filesystem or input fault |
| `BaseMissing` | refuse; the journal starts above sequence zero and nothing explains why. Restore the journal, or restore the `BASE` the seal that removed its segments wrote |
| `BaseMismatch` | refuse; interior; **no repair** — the records the base replaced are in the archive, not in this directory |

Where the table prescribes a command, the tool returns it **verbatim** in
`remedy` — the exact string, never a paraphrase, because you may run it.
Where the posture is "no repair", `remedy` is absent rather than empty, and
`blast_radius` says how much of the journal is unverifiable.

### Why `repair` is not a tool

`fsm repair --truncate-torn-tail` destroys data. Its safety argument is that
a person reads the quarantined tail bytes first and decides they are
expendable — and that argument does not survive being automated. So the
audit surface diagnoses precisely and hands over the command, and somebody
with the authority to lose those bytes runs it.

This is a decision, not an oversight. Adding a repair tool later would be
undoing it, and whoever does should know that is what they are doing.

### When the store will not open

A server pointed at a store it cannot open **starts anyway**, because
diagnosis is exactly the case where it must not vanish. That state is called
degraded, and it is reported rather than selected: there is no flag for it.

`initialize` succeeds, capabilities are unchanged, and `tools/list` is the
same list — a shrinking list would have a client cache a surface that
reappears once the store is repaired. Three tools answer, from a
classification rather than an open: `store_doctor`, `journal_verify`, and
`journal_replay`. A `machine_create` with `dry_run: true` also works, because
checking a definition needs no store and refusing it would block authoring
at the moment it is most useful.

Every other tool is refused with `store/degraded`, carrying the health, the
blast radius, and the remedy — the same three facts `store_doctor` would
give you. An error that only said "unavailable" would make a model retry
instead of diagnose. The documentation resources keep working throughout,
and the client is told once, at `error` level, why everything else is
failing.

## Sealing a journal prefix

Every guarantee above is about what the store *keeps*. None of them is about
what it costs to keep it. A journal that only grows makes disk track lifetime
rather than workload, makes a cold open cost the whole history, and makes
`journal verify` — the strongest claim this project makes — more expensive
every week, which is exactly the incentive that stops people running it.

Sealing is the answer, and it is not compaction: nothing is rewritten,
summarized, or made denser. A prefix of the journal is **relocated
unchanged** into an archive directory you name, and a `journal_sealed` record
in the live chain says exactly what moved and what it hashed to. A record that
is not the record that was written is not evidence, so the bytes in the
archive are the bytes that were on disk.

```console
$ fsm journal archive --to /backup/fsm-2026-Q1 --dry-run
$ fsm journal archive --to /backup/fsm-2026-Q1
```

### When to seal

When the retention window you actually need is shorter than the history you
have. After a seal, disk and cold-open cost track that window instead of the
store's lifetime, and everything the store claimed about the sealed prefix is
still checkable — with the archive present by walking it, and without it by
checking that the seal's committed hashes match the base the store runs on.

Sealing is always an explicit operator command with an explicit target. There
is no schedule and no automatic trigger, for the same reason a deadline fires
only when a caller polls: a store that reorganizes itself on a timer has a
background writer, and this engine does not have one.

### The cut is a segment boundary

The operation **creates** its cut when it can: it appends a `state_checkpoint`
and rotates, so the base derives from state a fold has already proved. When
it cannot cut at the head — see the pin below — it seals through the highest
segment boundary the cut is allowed to reach instead. Either way the cut is
segment-final, because a segment the cut fell inside could only be archived
by splitting it, and splitting means rewriting published bytes.

`--before-seq N` **asserts** which sequence the seal will seal through, as
`--dry-run` reported it. It does not pick a lower cut; it stops a preview and
a run from disagreeing about which prefix moved.

### What pins an archive

A **pending effect** holds the records its execution is derived from. The
executor keeps nothing in memory by design — the journal is its only memory —
so a pending effect's emitting record, its instance's creation record, and
every one of its attempt records are read back rather than remembered.
Archiving any of them would not corrupt the store; it would change what the
executor concludes, silently, which is worse.

So a store with work in flight seals **lower** than one at rest, and
`--dry-run` names the highest cut available. Only pending effects pin
anything: an instance that has been running for a year and is waiting at a
gate contributes nothing, whatever its age, because its whole history is
derivable from the base.

### Which idempotency keys survive, and why the rest may go

A dropped `request_id` cannot be told apart later from one the store never
saw — nothing records that it existed, so there is no honest way to report
"this one expired". A seal therefore **carries** every key claimed above the
cut, and every key whose claiming record names an instance that is **live** in
the base state, whatever its sequence. Carried keys track live workload, not
lifetime: a store with a thousand finished instances and three running ones
carries three instances' keys.

It drops the rest, and each dropped key is independently unreplayable. An
event, poll, ack, or annotation against a settled instance is refused by that
instance's terminal status. A `create` naming an instance that exists is
refused with `req/instance_exists` — creating never replaces. And a machine
definition is content-addressed, so re-adding it is idempotent by hash.

One consequence is worth knowing before you meet it. A carried key whose
claiming record is in the archive can no longer have its original response
reconstructed, because that response is rebuilt by reading the record. Retrying
such a key returns `store/sealed_replay_unavailable`: the request **was**
applied and is not applied again, and the store refuses rather than guess at a
thinner answer. Read the original outcome from the archive.

### `store/archive_refused` is a size limit, not a veto

Two things produce it, and the hint says which. Either the keys the cut must
carry do not fit a base state file — clear it by sealing at an earlier cut, or
by letting running instances settle — or the cut is at or above the pin, and
the hint names the highest admissible cut. Neither is a rule against sealing a
store that has work in flight.

### What a sealed store's `verify` says

Three verdicts, and the middle one is the point:

| Presented | Verdict | Exit |
|---|---|---|
| the store is not sealed | *(no seal reported)* | `0` |
| sealed, no archive given | `prefix_not_presented` | `7` |
| sealed, `--with-archive <dir>` | `prefix_walked` | `0` |

**A verification that did not read the sealed bytes never reports what one
that did reports.** Without `--with-archive` the prefix is not read at all —
not partially, not optimistically — and the middle verdict has its own exit
code because a shell script reads only that. With the archive presented, the
manifest, every segment digest, and the record at the cut are all checked, and
only then is the answer the one an unsealed store gives.

The per-segment digests are **plain, undomained SHA-256 over each file's exact
bytes**, so `sha256sum seg-*.jsonl` reproduces them. Every other hash here is
domain-separated; this one is not, on purpose, because an archive auditable
only by the tool that wrote it is a weaker artifact than one auditable by
`coreutils`.

### The archive is yours

`fsm` writes an archive once and never reads it again unless you ask with
`--with-archive`. It does not manage retention, does not delete anything, and
will not seal into a directory that already holds a `MANIFEST` — one seal, one
archive, one manifest. Destroying archived bytes is a separate act you take
with your own tools, exactly as `repair --truncate-torn-tail` is.

### When a sealed store will not open

| Condition | What it means | Remedy |
|---|---|---|
| `store/base_missing` | the journal starts above sequence zero and no base explains why — records were removed without a seal saying so | restore the journal's segments from backup, or restore the `BASE` the seal that removed them wrote |
| `store/base_mismatch` | a base is present and does not match the seal that commits it, or its own declared roots | **no repair reconstructs a base.** The records it replaced are in the archive, not in this directory. Restore the `BASE` this store was sealed with |

Neither is repairable from the data directory alone, and neither is offered a
repair command, because a command that cannot work is worse than the truth.
The base state file is **required**, never a cache: a missing or stale snapshot
degrades to a fold, and a missing base refuses the open.

## Serving over HTTP

`fsm serve` speaks stdio by default: one client, spawned as a child process,
for the life of that process. `fsm serve --http <addr>` speaks the MCP
**Streamable HTTP** transport instead, and the difference that matters is not
the wire format — it is that one process serves every client and that process
is the single writer.

```
fsm serve --http 8080 --data-dir ./data
fsm serve --http 127.0.0.1:9000 --http-path /mcp --data-dir ./data
```

| Flag | What it does |
|---|---|
| `--http <addr>` | serve HTTP on `<addr>`. A bare port binds loopback |
| `--http-path <path>` | the endpoint path. Default `/mcp` |
| `--http-allow-remote` | permit a non-loopback bind. Read the security section first |
| `--http-origin <list>` | extra allowed origins, comma-separated |
| `--http-token-file <path>` | read the bearer token from a file |

Choose stdio when one client owns the store and is happy to spawn a child
process. Choose HTTP when more than one client needs the same store, when the
client is a browser, or when the store lives on a different machine from the
person using it.

### Sessions

`initialize` mints a session and returns it in `Mcp-Session-Id`. Every later
request must carry that header. A request without it is `400`; one naming a
session this server does not have is **`404`, and `404` means re-initialize**
rather than retry — sessions expire after 30 minutes idle, and `DELETE` on
the endpoint ends one immediately. A server holds at most 32 at once and
refuses the thirty-third with `503`.

`MCP-Protocol-Version` is checked on every request against the version
negotiated at `initialize`; a mismatch is `400`. An absent header is treated
as the negotiated version.

### The event stream

`GET` on the endpoint with `Accept: text/event-stream` opens the session's
stream, and everything the server says unprompted travels on it — plan 0012's
notifications, progress reports, and the elicitation requests plan 0013 added.
**One stream per session**: a second `GET` is `409`, because two would split
notification ordering with nothing to reassemble it. A client that wants two
streams opens two sessions.

Reconnect with `Last-Event-ID` to resume. The buffer holds **256 events or
1 MiB, whichever comes first**; an id whose events have been evicted is `409`
and means re-initialize, and an id this session never issued is `400`. A
disconnect frees the stream slot and leaves the session — and its
subscriptions — alone.

### Many clients, one writer

Every session's call goes through one mutex around one `Store`. The
single-writer constraint stops being the thing clients trip over and becomes
the serialization point they share: two clients, or a browser and a terminal,
or a person and a scheduled job, can all work against one store because they
are all talking to the process that holds the lock. Reads take that lock too,
because the reason there is no half-applied macrostep to observe is that it
is held across the whole call.

`journal_verify` and `journal_replay` are the exception, and deliberately:
they read through `open_read_only`, take no lock, and so cannot block a
writer no matter how long they run.

Three deployment shapes, following plan 0008's run modes:

- **One HTTP server as the writer.** Many clients, one process, one lock.
- **An executor plus a read-only HTTP server.** The executor owns the writer
  and the server watches; clients read and subscribe.
- **A contended server.** If another process holds the writer, `serve` retries
  briefly and then starts **read-only**, saying so in its startup line, in an
  error-level notification, and in `instructions`. That is *healthy and busy*,
  not broken — the remedy is to stop the other writer or to use the paired
  deployment, and it is deliberately distinct from an unhealthy store, whose
  remedy is `store_doctor` and a repair.

### Status codes

| Code | When |
|---|---|
| `200` | a request answered, in JSON or as a stream |
| `202` | a notification or a response accepted; no body |
| `400` | malformed request, missing session id, protocol-version mismatch, unknown `Last-Event-ID` |
| `401` | missing or wrong bearer token |
| `403` | missing or unlisted `Origin` |
| `404` | unknown path, unknown or expired session |
| `405` | a method this endpoint does not route |
| `406` | a stream is needed and the client does not accept one |
| `408` | the request did not arrive in time |
| `409` | a second stream for one session, or a `Last-Event-ID` that has been evicted |
| `411` | a chunked request; this server reads `Content-Length` bodies |
| `413` | a body, or a whole request, over the limit |
| `414` | a request line over 8 KiB |
| `431` | too many headers, or headers too large |
| `500` | an internal failure |
| `503` | too many connections, or too many sessions |

### Security

State it plainly, because a reader who infers a security model from a flag
list will infer one this binary does not have:

- **Loopback by default.** A bare `--http 8080` binds `127.0.0.1`. A
  non-loopback bind requires `--http-allow-remote` **and** a token, or the
  server refuses to start.
- **`Origin` is validated on every request, in every configuration**,
  including loopback. It is compared exactly — scheme, host, port — with no
  wildcards and no suffix matching. A missing `Origin` is refused. This is the
  DNS-rebinding defence and it is not optional.
- **A static bearer token**, compared in constant time over the full length.
  It is read from `--http-token-file` or `FSM_HTTP_TOKEN`, and **never** from
  a command-line argument, because arguments are visible in `ps` to every user
  on the host.
- **There is no TLS in this binary.** There will not be one. Exposing this
  server beyond loopback means putting it behind a reverse proxy that
  terminates TLS; anything else puts your traffic and your token in the clear.

**Session ids, and what they are not.** An id is
`sha256("fsm:session:1" || seed || counter || pid || nanos)` truncated to 32
hex characters. The seed is 32 bytes from `/dev/urandom` where that is
readable, read once at start; where it is not — Windows — it is two `u64`s
from `RandomState`, which the standard library seeds from the operating
system per process, hashed with the process id. **That is process-seeded
entropy, not a CSPRNG**, and the reason is concrete: Rust's standard library
has no random-number API, this workspace has zero dependencies, and
`unsafe_code = "forbid"` rules out calling `getrandom` or
`BCryptGenRandom` directly. Treat the session id as **defence in depth**. The
controls that carry the weight are the loopback default, `Origin` validation,
and the token.

**The OAuth deviation.** The MCP specification recommends that an HTTP
transport behave as an OAuth 2.1 resource server. This one does not, and the
reason is the same constraint: with zero dependencies and no TLS, a partial
OAuth implementation over cleartext would be worse than an honest static
token — it would look like a security model while providing less than one.
Closing the gap would require a TLS implementation or a mandated proxy, token
introspection against an authorization server, and protected-resource
discovery metadata. Until then this is a documented decision rather than an
omission.

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
| `handlers` | a non-empty array of handler objects |
| `max_inflight` | optional, top level. Handler processes this executor runs at once, `1` to `64`; default `8` |
| `max_inflight_per_instance` | optional, top level. Handler processes one instance may occupy at once, `1` to `16`; default `2` |
| `effect` | a non-empty effect name the machine declares; unique across the table |
| `kind` | optional, `"process"` (default) or `"mcp"` |
| `argv` | non-empty array of strings. `argv[0]` is the command and must be a **literal rooted path** — no `{placeholder}`, no bare name. **Identical for both kinds** |
| `tool` | required for `kind: "mcp"`, refused otherwise. The one tool name this handler calls |
| `arguments` | optional for `kind: "mcp"`, refused otherwise. An object; `{placeholder}` substitutes in **string values** at any depth |
| `timeout_ms` | a whole number of milliseconds, `1` to `86400000`; the run is killed past it |
| `retry` | optional. An object with `attempts` (total including the first, `1` to `16`; default `1`), `backoff_ms` (default `1000`), `max_backoff_ms` (default `60000`), and `on`, an array of failure classes |
| `on_ok` / `on_failed` | optional. An object with a non-empty `event`, an optional `payload` **object** (default `{}`), and an optional `stamps` array of field names (default `[]`) the store fills from the clock |

Every other key is refused, at the top level and inside a handler alike. A
misspelled `on_okay` that validated would ack effects and never advance, which
at run time is indistinguishable from a deliberately undeclared advance — and a
misspelled `max_in_flight` would spawn without a bound while looking like it
had one.

A table that says none of the new keys means exactly what it meant before they
existed: `kind` defaults to `process`, `retry` defaults to one attempt, and the
two caps are bounds a normal deployment never reaches.

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

### Retry, backoff, and dead letters

`retry` is per handler and absent means `attempts: 1` — one try, exactly what a
table written before this feature meant.

```json
"retry": { "attempts": 3, "backoff_ms": 2000, "max_backoff_ms": 60000, "on": ["timeout", "nonzero_exit"] }
```

**Attempts are journaled, not remembered.** Each failed attempt that will be
tried again writes an `effect_attempted` record; the count comes from those
records and from nothing a process holds in memory. That is the whole point: a
retry counter kept in memory is lost by exactly the restart it exists to
survive, so a restarted executor resumes mid-retry at the number its
predecessor would have reached. The final failure is **acked** rather than
journaled as an attempt, so a three-attempt handler leaves two records.

**The backoff is a formula, not a schedule:**

```
due_ms = last_attempt_ts + min(backoff_ms * 2 ^ (attempt - 1), max_backoff_ms)
```

Every term comes from a journaled fact or the table. `last_attempt_ts` is the
record's own timestamp, so an executor that comes up an hour later **resumes**
the wait rather than restarting it. The multiply and the shift saturate: an
overflowed deadline would land in the past and turn backoff into a busy loop,
which is the opposite of what it is for. An effect inside its window produces
**no directive at all** — the executor does not sleep and does not hold a
concurrency slot, it simply does not act yet — and the tick says so, so an
operator watching a quiet tick can tell "waiting to retry" from "nothing to do".

**There is no jitter, and that is a decision rather than an omission.** Jitter
would make the scheduler non-deterministic, and determinism is what makes
restart equivalence testable: the same observation and the same `now_ms` must
produce the same directives, or a chaos suite cannot assert anything. Jitter
exists to spread a thundering herd across many nodes, and this executor is
single-node — there is no herd to spread.

`on` is the closed set of failure classes a policy may name:

| Class | Raised by |
|---|---|
| `nonzero_exit` | the handler exited non-zero |
| `timeout` | the run passed `timeout_ms` and was killed |
| `spawn` | the command could not be started |
| `mcp_error` | a tool call failed — `kind: "mcp"` handlers only |

Omitting `on` means every class **that kind can produce**, so a process handler
never silently carries `mcp_error`, which would retry nothing.

**`"cancelled"` is not a class and cannot be made one.** A handler killed
because its instance was cancelled must never be restarted: somebody decided
that instance was over, and a retry would spend the budget undoing their
decision. Writing `"cancelled"` in `on` is refused at startup with that reason,
because it is the one class an operator will try to configure. A failure the
executor cannot honestly repeat is likewise never retried — an argv template
naming an argument the emit did not produce fails identically every time, and a
server that violates the protocol produces the same broken exchange next time.

**Exhaustion is an ordinary failure.** When the last attempt fails, the effect
is acked `failed` through the same path every other failure takes, with
`result.error` set to `exec/retries_exhausted`, `result.attempts` naming the
count, and `result.class` preserving the cause that `error` replaced. There is
no terminal state and no new record kind: the ack is already the terminal fact.
So a machine that models a failure path **keeps working unchanged** — its
`on_failed` event fires exactly as it did before retry existed.

A handler with **no** `on_failed` still stalls its instance deliberately, which
is what an undeclared failure path has always meant. The instance sits where it
was with nothing in its outbox to say why, and that is precisely why the
dead-letter report exists:

```console
$ fsm execute --list-dead
$ fsm execute --list-dead --since 412
```

Every effect acked `failed` whose result carries the exhaustion cause, with its
instance, effect name, attempt count, failure class, and the last attempt's
capture. `--since` is exclusive, so passing the newest `seq` you have seen
returns what has died since. The report is **derived from the journal at read
time and stores nothing**: a dead-letter queue with its own state would be a
second source of truth about what happened to an effect, and it would drift
from the first the moment one of them was pruned, restored, or replayed. Both
it and the `dead_letters` field on `fsm execute --check` read through
`Store::open_read_only`, which takes no lock, so either answers while the
executor is running.

### Concurrency and fairness

An outbox holding five hundred pending effects would spawn five hundred
subprocesses, so two caps bound it: `max_inflight` (default 8) across the whole
executor, and `max_inflight_per_instance` (default 2) within one instance. Both
are counted over the handler processes **this process** is running now — a cap
on concurrency is a statement about this executor, so a restarted one correctly
fills up to the cap again, its predecessor's children being gone and their
effects still pending precisely because nothing acked them.

Only starts are capped. A kill, an advance event, and a deadline poll are
bookkeeping against the journal, cost no subprocess, and are never deferred by a
concurrency bound — a timed-out handler that could not be killed because the
host was busy would be the worst version of that bug.

Candidates are taken in a **round-robin**: ordered by position in their own
instance's queue, then `instance_id`, then `effect_id`, so every instance's
first pending effect is considered before any instance's second. Ordering by
`effect_id` alone would let the lexicographically-first instance take every slot
forever. The ordering is a pure function of one observation and needs no memory
between ticks, which is what keeps a restarted executor's decisions identical.
It does not *rotate*: with more permanently-busy instances than global slots,
the highest-sorting ones wait until one of the others empties. A rotating cursor
would close that window and cost restart equivalence with it, since two
executors reading the same journal would disagree about whose turn it was. What
the ordering does buy is that no instance can convert more queued work into more
of the host.

A tick that defers says so, once per tick with counts only:

```
error exec/inflight_deferred deferred=38 inflight=2
```

Silent truncation reads as "nothing to do", which is exactly the failure an
operator cannot diagnose.

### `kind: "mcp"`: an effect that calls another server's tool

A handler may be an MCP server the executor talks to over its stdio, rather than
a command whose exit status is the answer:

```json
{
  "effect": "summarize_case",
  "kind": "mcp",
  "argv": ["/usr/local/bin/case-tools", "--stdio"],
  "tool": "summarize",
  "arguments": { "case_id": "{case_id}", "mode": "brief" },
  "timeout_ms": 60000,
  "retry": { "attempts": 3, "backoff_ms": 2000, "on": ["mcp_error", "timeout"] },
  "on_ok": { "event": "summarized" },
  "on_failed": { "event": "summary_failed" }
}
```

**The security boundary does not widen by one inch.** `argv[0]` is still a
literal rooted path with no `{placeholder}` and no bare name — the same rule,
enforced by the same code, for both kinds. `tool` is one fixed name the operator
wrote. `arguments` is a template the operator wrote, whose placeholders name
effect arguments by the same `{name}` rule and the same canonical rendering
`argv` uses. **Nothing about a handler is constructed from machine-emitted
data.** Substitution applies to string values only, at any nesting depth:
numbers, booleans, and object **keys** are copied verbatim, because letting an
effect argument choose a property name would let emitted data reshape the call.
A placeholder that fills a whole string still produces a string.

**One effect is one tool call**, and the exchange is fixed: `initialize` at
protocol version `2025-06-18`, `notifications/initialized`, one `tools/call`,
and the response to it. A handler that needs two calls is **two effects**, which
keeps each independently retryable, independently journaled, and independently
visible in the outbox. **One process per effect**, with no pooling and no
long-lived connections — the same isolation every subprocess handler gets.
Notifications and log messages the server sends while the call is outstanding
are ignored: a server that logs is not a server that failed. The server's
standard error is captured with the same bound and digest a process handler's
is, so a crashing server leaves evidence.

The result becomes the ack deterministically:

| Server response | Ack |
|---|---|
| result with `isError` absent or false | `ok`, `result.structured` = `structuredContent` if present, else `content` |
| result with `isError: true` | `failed`, `result.error` = `mcp/tool_error`, with the content kept |
| JSON-RPC error | `failed`, `result.error` = `mcp/rpc_error`, with `code` and `message` |
| timeout, spawn failure, protocol violation | `failed`, `result.error` = `exec/timeout`, `exec/spawn`, or `exec/mcp_protocol` |

Deterministic and bounded, both for the same reason: the store fingerprints the
ack over this object, so a value carrying a timestamp, a pid, or an OS message
would turn a re-issued ack into a conflict instead of a replay, and a result
past 4 KiB is truncated on a character boundary with a SHA-256 of the whole
value beside it rather than pushing the ack past the journal's payload limit. A
result that fits is journaled as it came — an object stays an object.

Validate a table before pointing it at a store:

```console
$ fsm execute --check --handlers ./handlers.json
```

The pre-flight also reports the store's `dead_letters`, so "your table is
valid" is not the only thing it tells you: an effect that exhausted its retry
budget under the previous run is still sitting there, acked failed, possibly
with an instance stalled behind it. Ask the same question on its own with
`fsm execute --list-dead`, or `--list-dead --since <seq>` for what has died
since you last looked. Both read through `Store::open_read_only`, which takes
no lock, so either answers while the executor is running.

### Idempotency: why a restarted executor is safe

The executor never invents a `request_id`. Every key is derived from content
the journal already holds:

| Write | Key |
|---|---|
| ack | `exec-ack-{effect_id}` |
| failed attempt | `exec-try-{effect_id}-{attempt}` |
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

What it cannot do there is write. These twelve refuse with a message naming
the mode: `machine_create`, `instance_create`, `instance_send`,
`deadline_poll`, `effect_ack`, `instance_cancel`, `instance_migrate`,
`invocation_start`, `invocation_return`, `signal_deliver`,
`instance_elicit`, `instance_annotate` — the ask because an answer it could not
send would waste a person's time, the note because it writes a record like any
other. A `machine_create` with `dry_run`, and
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
| `exec/retries_exhausted` | a handler failed its last attempt; the effect is acked `failed` so the machine's own failure path still fires |
| `exec/mcp_protocol` | an MCP handler's server did not speak the protocol: no `initialize` result, or a malformed message |
| `exec/mcp_tool` | an MCP handler's tool call returned an error result |
| `exec/inflight_deferred` | a run was deferred because the in-flight cap was reached; it is attempted on a later tick, and nothing is journaled |

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

### Reading the tree back

The two directions are not symmetric, and the difference is worth knowing
before you build a view on them.

`instance_get`'s **`children` lists live invocations**: a slot appears while
it is `pending` — carrying the child id it *will* have, because that id is a
function of the parent and the slot and can be computed before the child
exists — and while it is `running`. Once `invocation_return` settles the
slot, the entry is gone, and a parent whose children have all returned
reports `children: []`.

`instance_get`'s **`parent` is permanent**: a child names the instance and
slot that invoked it for as long as it exists, settled or not.

So a question about what is happening now is answered by `children`, and a
question about what happened is answered by the journal —
`instance_history` holds the `instance_invoked` and `invocation_returned`
records, and every edge the tree ever had is in them. `instance_list
--roots-only` hides children at any status, which is what makes it a list of
workflows rather than a list of instances.

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
