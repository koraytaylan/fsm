# Embedding fsm as a library

The CLI and the MCP server are two front ends over the same engine. This page is
for the third consumer: a Rust program that drives the engine in process.

## The three crates

| Crate | What it is | Depend on it when |
|---|---|---|
| `fsm-core` | The engine. Pure: no I/O, no clock, no `HashMap`, no floats. Parses and compiles specs, steps instances, analyses machines, hashes state. | You keep your own persistence. |
| `fsm-store` | The durable shell. Append-only hash-chained journal, fsync per record, snapshots, the one wall-clock read. | You want the journal as your store. |
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
use fsm_core::step::{step, create, Outcome};
use fsm_core::expr::eval::Budget;

// 1. Parse and compile. `machine_id` is a hash of the canonical definition.
let def = parse(spec_bytes, &JsonLimits::DEFAULT)?;
let compiled = compile_accepted(&def)?;          // Err = Vec<Finding>, each with a path and a hint
let tree = Tree::build(&compiled.spec.states);

// 2. Create an instance. `create` runs the entry chain.
let applied = create(&compiled, &tree, &overrides)?;

// 3. Step. Pure: `st` is not mutated, nothing is committed.
let mut budget = Budget::new(4096);
match step(&compiled, &tree, &st, "docs_ok", &payload, &mut budget) {
    Outcome::Applied(a) => { /* persist a.ctx_after / a.leaf_after, then run a.effects */ }
    Outcome::Ignored    => { /* the machine declares this event ignorable here */ }
    Outcome::Rejected(r) => { /* r.code is a namespaced code, e.g. run/unhandled */ }
}
```

Also available without a store: `analyze::analyze_all` (unreachable states,
shadowed transitions, guards that can never hold), `analyze::completeness_matrix`
(which `(leaf, event)` pairs are handled), `analyze::enabled_events`,
`simulate::simulate`, `diagram::{mermaid, dot}`, and `hashes::state_hash`.

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

## Stage 2: using the journal as your store

`fsm_store::store::Store` gives you the durable, auditable version: a total
order, hash-chained records, idempotent requests, and replay.

```rust
use fsm_store::store::Store;

let mut store = Store::open(&data_dir)?;         // folds the journal (or a snapshot)
store.define_machine(def, /*dry_run*/ false, /*if_exists_error*/ false)?;
store.create_instance("case_review", "i1", "req-1", None)?;
store.send_event("i1", "docs_ok", payload, "req-2", None)?;
```

### Concurrency contract

Every `Store` method is **synchronous and blocking**, and a store is a
**single-writer** resource:

- one process at a time — `Open` takes a process-wide advisory lock on
  `<data_dir>/journal/LOCK`; a second opener gets `store/lock`;
- one writer at a time — `&mut self` on every mutating call;
- every append `fsync`s before returning;
- `Store::open` folds the whole journal, or a snapshot plus the tail.

There is no async API and no interior locking. On Tokio, **own the `Store` from
one dedicated blocking thread** and send commands to it over a channel — a writer
actor. Do not put it behind a `Mutex` shared across tasks: you would serialise
anyway, but on the async executor's threads.

### Measured cost

From `crates/fsm-store/tests/append_latency.rs`, release build, 2000 iterations,
AMD Ryzen 7 PRO 8700GE, ext4 on NVMe SSD:

| Operation | p50 | p95 | p99 | throughput |
|---|---|---|---|---|
| `create_instance` | 39 µs | 52 µs | 65 µs | ~24 000/s |
| `send_event` | 51 µs | 70 µs | 88 µs | ~19 000/s |

Open cost, 4001 records / 2000 instances: **195 ms** full fold (~49 µs/record).

Two caveats worth sizing around:

- These are one disk's fsync numbers. Re-run the harness on yours:
  `cargo test -p fsm-store --test append_latency -- --ignored --nocapture`.
- A snapshot did **not** beat the full fold at this shape (~225 ms vs ~195 ms):
  restoring 2000 instances means recompiling their machines and re-verifying
  every state hash. Snapshots pay off when records greatly outnumber instances.
  Measure before assuming they help you.

A single writer at ~19 000 sends/s is the throughput ceiling, and it is a
deliberate one. If your driver count implies more, shard by data directory —
there is no HA, replication, or multi-writer story, by design.

## Contracts an embedder should know

### `request_id` is an idempotency key over content

Every mutating call takes a `request_id`. Resending it with the **same** content
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

### What is deliberately absent

- **No parallel regions.** One instance per concurrent unit of work is the
  intended factoring.
- **No deadlines or timers.** The core is clock-free. Wall-clock caps belong in
  your runtime (e.g. `tokio::time::timeout`) and arrive as an ordinary event.
- **No hidden events.** Nothing but an explicit send advances an instance.

## Errors

Every error carries a namespaced `code`, a `message`, and a `hint` that states
the fix. Route on the namespace:

| Prefix | Meaning | Who fixes it |
|---|---|---|
| `def/`, `expr/` | the definition does not compile | the spec author |
| `req/` | the request is malformed or misaddressed | the caller |
| `run/` | the machine rejected the event | the caller, or the machine |
| `store/`, `io/` | the store or the disk | the operator |

`req/seq_mismatch` and `req/payload_too_large` do not consume the `request_id`;
retry under the same key once corrected. `req/request_id_conflict` is never
retryable — use a new key. The full list is in [SPEC.md](SPEC.md#appendix-a--error-codes).
