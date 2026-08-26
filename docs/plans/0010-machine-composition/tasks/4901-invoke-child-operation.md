---
id: invoke-child-operation
title: "Invoke Child Operation"
workstream: "0049"
kind: task
depends_on:
  - done-invoke-events
gated: false
touches:
  - crates/fsm-store/src/store/instance/invoke.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-store/src/store/instance/mod.rs
  - crates/fsm-store/src/store/lifecycle.rs
  - crates/fsm-store/src/store/view.rs
  - crates/fsm-store/src/store/mod.rs
  - crates/fsm-core/src/record.rs
  - crates/fsm-core/src/replay/apply.rs
  - crates/fsm-core/src/spec/validate/invoke.rs
  - crates/fsm-core/src/spec/compat.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-store/tests/invoke_child.rs
  - crates/fsm-cli/tests/naive_caller/composition_flows.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Invoke Child Operation

One record creates a child, and fold derives the child's whole existence from it — which is what lets composition keep the engine's one-record-one-fsync-one-atomic-outcome property instead of inventing group commits.

**Steps:**

1. Create `crates/fsm-store/src/store/instance/invoke.rs` and declare it in `instance/mod.rs`. Implement `invoke_child`, `invoke_child_on(clock, parent_id, slot, request_id)` in the three-arity style the other mutators use, gated by `ensure_writable()` like every mutator.
2. Add the `instance_invoked` record kind to `crates/fsm-core/src/record.rs` with body `{parent_instance_id, slot, child_instance_id, child_machine_id, overrides, request_id, state_hash, child_state_hash, state_format}`. `overrides` is the evaluated projection carried in the parent's state by `4802` — **do not re-evaluate it here**; the values must be the ones the entry pipeline computed.
3. Create the child through the same `create` path any instance uses, from the child machine's declared inits with `overrides` applied, at the record's `ts`. A failed creation fails the whole operation and journals **nothing**, mirroring SPEC's rule that `run/create_failed` is unjournaled; surface it as `run/invoke_create_failed` wrapping the inner error, leave the slot `Pending`, and let the caller correct and retry under the same key.
4. Move the slot `Pending → Running` and commit both `state_hash` (parent) and `child_state_hash`, so a fold can check the pair rather than trusting one side.
5. Enforce legality: the operation is valid only against a `Pending` slot. Against `Running` or `Returned` it returns `req/invoke_slot_state` and journals a `request_rejected` that claims the key, so a retry replays the same benign refusal — the shape `ack_effect` already uses for a settled effect.
6. Make fold derive the child: replay of `instance_invoked` reconstructs the child instance from the record body and `ts` alone, running `create` — which after plan 0009 is a macrostep, so a child whose initial state has an eventless exit reacts on creation exactly as a root instance does. There is **no** separate `instance_created` record for a child, and a reader must never need one. The child's reaction is **not** journaled as a `microsteps` array on this record: it is fully re-derived by the same `create` call, and `child_state_hash` is what proves the derivation matched. Recording it would duplicate a derivable fact in a permanent record, which SPEC's payload discipline exists to avoid.
7. **Fix every place that resolves a record to an instance by a field name.** This plan's records carry `parent_instance_id`/`child_instance_id` and `sender_instance_id`/`target_instance_id` — **none of them has a field called `instance_id`** — so every `body.get("instance_id")` probe silently drops them. A parent's `instance_history` would show the event that entered the invoking state and then nothing about the child ever existing, which defeats this plan's central promise that every edge between instances is a record rather than an inference.

   Add `pub fn instances_touched(record: &Record) -> Vec<&str>` to `crates/fsm-core/src/record.rs`, beside `RecordKind`, **exhaustively matched over every kind**, and route **all five** sites through it — there are more than the two obvious ones, so grep `get("instance_id")` before declaring this done:
   - `store/view.rs::history_page`, which filters the record list;
   - `store/view.rs::explain_seq`, which guards that a seq belongs to the named instance;
   - `store/mod.rs::HistSink::on_record`, which builds the per-instance `history: BTreeMap<String, Vec<u64>>` index as records are folded;
   - `store/lifecycle.rs`, which rebuilds that same index on **both** open paths (around lines 49 and 97).

   Fixing the view without fixing the index, or either open path without the other, leaves the two disagreeing about which records an instance has. Exhaustive matching at the point where kinds are *defined* is what forces every later plan to answer the question: plan 0011's `instance_migrated` and plan 0016's `effect_attempted` extend it, and plan 0012's change feed consumes it rather than inventing a second rule.
8. **Teach duplicate replay about this record kind.** `crates/fsm-store/src/store/idempotency.rs::replay_duplicate` reconstructs a retry's response from the journal with a chain of **kind-specific** branches — and it is `if`/`matches!`, not an exhaustive `match`, so a new kind falls through every arm **silently** rather than failing to compile. Add the `instance_invoked` arm that rebuilds this operation's response. Note the trap before you test it: `replay_duplicate` first consults an in-memory `last_responses` cache, so a same-process retry appears to work with no arm at all; the reconstruction path only runs after a restart, which is exactly the case the executor's resumption and every second client depend on.
9. In `crates/fsm-store/src/store/lifecycle.rs::define_machine_on`, add the five catalogue-dependent admission checks `4801` deferred: `def/invoke_unknown_machine` (the store does not hold the referenced machine), `def/invoke_unknown_ctx`, `def/invoke_type`, `def/invoke_cycle`, and `def/invoke_depth` — the last two by a depth-first walk over the invocation graph, which is finite and immutable precisely because `machine` is a content hash.

**Tests:**

- `crates/fsm-store/tests/invoke_child.rs`: invoking a `Pending` slot writes one `instance_invoked`, creates the child at the derived id, and moves the slot to `Running`.
- The child's context equals the parent's declared projection over the child's inits; unnamed child variables keep their `init`.
- Idempotency: re-issuing the same `request_id` returns `duplicate: true` and writes nothing; re-issuing with different content is refused, not replayed.
- **Cold-path replay:** drop the `Store`, reopen it, and re-issue the same `request_id` — the reconstruction must produce the same `duplicate: true` response from the journal alone. The warm path is served by an in-memory response cache, so a test that only retries in the same process proves nothing about the case that actually matters.
- Invoking a `Running` or `Returned` slot returns `req/invoke_slot_state` and journals a `request_rejected` claiming the key; the retry replays the refusal.
- A child machine whose creation fails journals nothing at all, leaves the slot `Pending`, and reports `run/invoke_create_failed`; a corrected retry under the same key then succeeds.
- Fold: a journal containing only `instance_invoked` for the child reconstructs the child with the same `state_hash` the record committed.
- **History shows both sides:** after invoking, the **parent's** `instance_history` contains the `instance_invoked` record, and so does the **child's** — assert both, since a field-name filter would have shown it in neither.
- `explain_seq` resolves an `instance_invoked` seq for the parent and for the child rather than reporting a mismatch.
- The folded per-instance index and `history_page` agree: for both the parent and the child, the index's seq list equals the seqs `history_page` returns. A view fixed without its index is the failure this asserts against.
- Both `Store::open` paths build the same index for a store containing composition records — assert by opening a store fresh and by opening one that loaded a snapshot.
- `instances_touched` is exhaustive over `RecordKind`: adding a variant without mapping it fails to compile — verify by adding one locally during development, then revert.
- A record kind carrying a plain `instance_id` still resolves exactly as before, and the existing `instance_history` goldens do not move.
- Admission: defining a machine that invokes an unknown hash reports `def/invoke_unknown_machine`; a `with` naming an absent child variable reports `def/invoke_unknown_ctx`; a scale mismatch reports `def/invoke_type`; a two-machine cycle reports `def/invoke_cycle`; a five-deep chain reports `def/invoke_depth`.
- Read-only: `invoke_child` on a read-only store refuses with `io/write`.

- **Done when:** `cargo test -p fsm-store --test invoke_child` passes every case above including the unjournaled-failure, fold-derivation, and cold-path-replay rules, the five catalogue checks fire at `define_machine`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `invoke_child_on` in the three-arity mutator style, the `instance_invoked` kind with its body shape and read-time validation, fold's derivation of the child (including that the record's `child_instance_id` must equal the derived one and that the parent's slot must have been `Pending`), the `req/invoke_slot_state` refusal with its journaled `request_rejected`, the unjournaled `run/invoke_create_failed`, `fp_invoke` keyed on `(parent, slot)` — every other field being derived from them and the state — and the cold-path replay arm. `instances_touched` is exhaustive over `RecordKind` and all five resolution sites route through it.

**Corrections.** (1) Step 9 puts the catalogue rules in `store/lifecycle.rs`; they live in `fsm-core`'s `spec/validate/invoke.rs` as `validate_catalogue(compiled, catalogue)` and the store calls them. The rules judge a definition, so they belong beside the definition, and as a pure function they are testable without a store; the store's contribution is the catalogue, which is exactly what it has and the core does not. Typing a done-invoke payload needs the same catalogue, so `define_machine_on` also compiles through `compile_accepted_with_catalogue`. (2) The cycle walk keys on the **digest**, not the machine name: two machines may share a name and differ in content, and treating that as a cycle would refuse an ordinary revision invoking its predecessor. (3) `def/invoke_cycle` is unreachable by construction — a cycle needs each machine's digest inside the other's document, a hash preimage cycle — so it joins the two every-code allowlists with that reason, and the rule stays as defence in depth for a later plan that resolves a slot some other way. Its unreachability is the payoff of the content-addressed ruling, and worth stating where the ruling is. (4) The operations have no MCP tool until `5102`, so their one-step corrections and outcomes drive the store directly, one layer below a tool call; `composition_flows.rs` carries them and moves to `dispatch` when the tools land. (5) `4801`'s one-step rows named a fabricated digest, which this task's catalogue check turns into `def/invoke_unknown_machine`; the rows now name a pinned child document the suite defines first, with its digest asserted against `machine_id` so the two cannot drift apart. (6) The applier pushed `replay/apply.rs` to 1004 lines, so it is split into `replay/apply/{mod,event,deadline_records,instance,invoke}.rs` — one module per record subject behind the same dispatcher — following the rule `4801` step 5 states for `reactive.rs`: split at the seams when an addition crosses the cap, and say so in the commit message.
