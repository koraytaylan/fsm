# Embedding fsm as a library

The CLI and the MCP server are two front ends over the same engine. This page is
for the third consumer: a Rust program that drives the engine in process.

## The three crates

| Crate | What it is | Depend on it when |
|---|---|---|
| `fsm-core` | The engine. Pure: no I/O, no clock reads, no `HashMap`, no floats. Parses and compiles specs, steps instances, polls caller-timed deadlines, analyses machines, hashes state. | You keep your own persistence and supply timestamps. |
| `fsm-store` | The durable shell. Append-only hash-chained journal, fsync per record, snapshots, and wall-clock reads at mutation boundaries. | You want the journal as your store. |
| `fsm-cli` | The `fsm` binary: CLI plus MCP server. | You are a host, not an embedder. |

`fsm-core` and `fsm-store` are supported embedding targets and are covered by
the release acceptance criteria. `fsm-cli` is a binary crate; do not depend on
it as a library — `fsm-store` exists so you do not have to.

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
