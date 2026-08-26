---
id: invoke-declaration-and-validation
title: "Invoke Declaration And Validation"
workstream: "0048"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/spec/mod.rs
  - crates/fsm-core/src/spec/parse/states.rs
  - crates/fsm-core/src/spec/validate/reactive.rs
  - crates/fsm-core/src/limits.rs
  - crates/fsm-core/src/error.rs
  - crates/fsm-core/src/spec/serialize.rs
  - crates/fsm-core/tests/invoke_declaration.rs
  - crates/fsm-cli/tests/naive_caller/one_step_data.rs
  - crates/fsm-cli/tests/naive_caller/harness.rs
  - crates/fsm-cli/tests/naive_caller/reactive_flows.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Invoke Declaration And Validation

An invocation is declared by content hash, not by name, and that single ruling is what makes a parent's identity pin its child's behaviour forever and the invocation graph statically checkable — so the declaration and the rules that guard it land together, before anything can enact one.

**Steps:**

1. Add `pub invokes: Vec<InvokeSpec>` to the state node in `crates/fsm-core/src/spec/mod.rs`, with `pub struct InvokeSpec { pub id: String, pub machine: String, pub with: Vec<(String, String)>, pub returns: Vec<(String, String)> }`. Both projections are ordered vectors so document order survives into canonical serialization and the trace.
2. In `crates/fsm-core/src/spec/parse/states.rs`, add `"invoke"` to the state key list. Parse each entry's required `id` and `machine` and optional `with`/`returns` objects; anything else is `def/shape`. Serialization omits `invoke` when empty, keeping the `machine_id` of every machine that does not compose.
3. Add `pub const MAX_INVOKES_PER_STATE: usize = 4;` and `pub const MAX_INVOKE_DEPTH: usize = 4;` to `crates/fsm-core/src/limits.rs`, each documented as deliberately **absent** from the genesis `limits` block for the reason `MAX_PAYLOAD_BYTES` gives.
4. Add this plan's **complete** closed set of new codes to `crates/fsm-core/src/error.rs`'s `ALL_CODES`, so no later task edits that file: `def/invoke_machine_ref`, `def/invoke_dup_slot`, `def/invoke_on_terminal`, `def/invoke_unknown_ctx`, `def/invoke_type`, `def/invoke_evt`, `def/limit_invokes`, `def/invoke_cycle`, `def/invoke_depth`, `def/invoke_unknown_machine`, `def/limit_signals`, `req/invoke_slot_state`, `req/signal_target`, `run/invoke_create_failed`.
5. **Check `reactive.rs` against the 1000-line ceiling before adding to it.** Plan 0009's `4202` created that file as a destination, and by now it holds the eventless rules, the `final`-state rules, and the done-event name resolution; this task adds invoke rules and `4803` adds more, with plan 0011's `supersedes` rules still to come. If `scripts/oversized-files.sh` is close to refusing it, split it into a `validate/reactive/` directory by feature — `eventless.rs`, `final_states.rs`, `invoke.rs` — preserving the call order `4202` fixed, and say so in the commit message. Discovering the cap in plan 0011 with three plans' rules already in the file is the failure this step prevents.
6. In `crates/fsm-core/src/spec/validate/reactive.rs`, implement the four rules decidable from **this definition alone**: `def/invoke_machine_ref` (not 64 lowercase hex), `def/invoke_dup_slot` (slot ids are unique machine-wide, across every state), `def/invoke_on_terminal` (an invoke on a `terminal` or `final` state, whose result nothing could consume), `def/invoke_evt` (a `with` expression naming `evt` — an invocation is triggered by state entry, not by an event), and `def/limit_invokes`.
7. Leave the four catalogue-dependent rules — `def/invoke_unknown_ctx`, `def/invoke_type`, `def/invoke_cycle`, `def/invoke_depth`, and `def/invoke_unknown_machine` — to `4901`, which runs them in `define_machine_on` where the child definitions are actually in hand. Note that split in the module doc so the next reader does not go looking for them here.
8. Add the appendix rows for all fourteen codes and the two limits to `docs/SPEC.md`? **No** — `5202` owns every SPEC edit in this plan, and `spec_appendix.rs` is what will catch the gap. Coordinate by leaving `5202` as the single documentation task, exactly as plan 0009 concentrated its prose in one place.

**Tests:**

- `crates/fsm-core/tests/invoke_declaration.rs`: a state with one valid invoke slot parses, compiles, and round-trips byte-stably.
- `machine` values that are 63 hex, 65 hex, uppercase hex, or a plain name all report `def/invoke_machine_ref`.
- Two slots sharing an `id` across two different states report `def/invoke_dup_slot` — the namespace is machine-wide, not per-state.
- An invoke on a `terminal` state and on a `final` state each report `def/invoke_on_terminal`.
- A `with` expression naming `evt.x` reports `def/invoke_evt`; one naming `ctx.x` is accepted.
- Five slots on one state report `def/limit_invokes`; four are accepted.
- Identity: a machine with no `invoke` serializes without the key and keeps its committed `machine_id`.
- `ALL_CODES` entries are unique, non-empty, and each carries one of the four namespace prefixes.
- `scripts/oversized-files.sh` passes, and `scripts/oversized-files.sh 500` reports `reactive.rs`'s size for the record so plan 0011 knows how much room is left.

- **Done when:** `cargo test -p fsm-core --test invoke_declaration` covers every rule and counter-case above, every `examples/` machine keeps its committed `machine_id`, the fourteen codes are registered, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `InvokeSpec { id, machine, with, returns }` on `StateNode.invokes`, parsed from `invoke` (projections are objects, kept in key order — the order canonical JSON already imposes, so "document order" of an object key set is its sorted order), serialized only when present (`serialize.rs`, where state serialization lives), and the five rules a definition decides alone in `validate/reactive.rs` with the module doc naming the store-level five. A `$`-prefixed slot id is `def/reserved_ident` like every other identifier. Step 4 was corrected: the every-code gates plan 0009 built require each registered code to be produced by a real tool outcome and taught by a one-step correction, so a code lands with the task that first produces it — this task registers its five (with their appendix and structural-rules rows, which `spec_appendix` now demands in both directions, and their naive-caller rows, repairs, and drives); `def/invoke_unknown_ctx`, `def/invoke_type`, `def/invoke_cycle`, `def/invoke_depth`, `def/invoke_unknown_machine`, `req/invoke_slot_state`, and `run/invoke_create_failed` land with 4901/4902, `def/limit_signals` with 5001, `req/signal_target` with 5002. Step 8's "no SPEC rows" was corrected the same way: the rows are mechanical, 5202 still owns the prose. `reactive.rs` is 350 lines after this task (`scripts/oversized-files.sh 200` reports it), so no split yet; plan 0011 has room.
