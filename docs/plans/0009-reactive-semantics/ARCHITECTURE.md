# Architecture — Plan 0009

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/spec fragments your task names before writing implementation code. They are the executable definition of done — when they pass, you are done; do not "improve" beyond them.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer. The command named in the Done-when is what runs them.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design, not a reason to edit it.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`. A red gate is never someone else's flake — this workspace has zero dependencies and deterministic tests.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here; where a trick is genuinely needed, this document names it — and if it doesn't, don't use one.
7. When a golden or byte-comparison test fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document or `docs/SPEC.md`; then say so in your commit message.
8. **This plan edits `docs/SPEC.md`.** That is unusual and deliberate: SPEC is the source of truth and goldens derive from its prose, so the prose changes *first*, in the task that owns the semantic ruling, and the fixtures derive from the new prose. Never leave an implemented behaviour that SPEC does not describe.

## 0000 — Orientation: the five facts this plan must not break

Each was read out of the code named beside it. Check the code, not your memory of it.

- **`step` selects exactly one transition, globally.** `step/mod.rs::step` walks `t.active_leaves(&st.configuration)`, and for a parallel machine concatenates each region's leaf-to-root chain in region document order, tracking a single `winner: Option<(Option<String>, u16, u16, usize)>`. There is one winner across all regions — not one per region. This plan does not touch that rule for the *triggering* event; it adds what happens **after** the winner has been applied.
- **The state that persists is `fsm.state/2` and it has exactly six parts.** `InstanceState { status, configuration, ctx, history, deadlines, pending }` (`machine.rs:82`). SPEC hashes `{format, machine_id, instance_id, seq, status, configuration, ctx, history, deadlines, pending}`. **The internal event queue is not in that list and must never be added to it.** A macrostep runs to quiescence before it returns, so the queue is empty at every sealed state by construction. Keeping it out of `InstanceState` is what lets this plan ship without a state-format bump, and it is the single most important structural rule in the plan.
- **Admission guarantees the runtime budget is unreachable.** `def/limit_eval` charges the sum of every compiled AST's node count plus one tick per distinct event with an omitted guard, against `MAX_EVAL_TICKS = 4096` (`limits.rs`), and SPEC states that "a definition accepted by the current compiler MUST NOT exhaust a fresh standard 4096-tick budget". That property is load-bearing — `internal/budget` is an engine bug, not a user error — and §0042 preserves it by *multiplying the operation budget*, never by relaxing admission.
- **Records are never rewritten, and fold re-applies through the pure engine.** SPEC §Journal: "fold re-applies through `step`/`create`/`poll_deadline` using the record timestamp and checks journaled `state_hash` / `exited` / `entered` / `source_state`." Any new record field is therefore additive-and-optional or it makes every existing store unreadable. §0046 adds exactly one optional key.
- **`$` is already reserved and already enforced.** `def/reserved_ident` refuses `$`-prefixed state names (`spec/validate.rs:95,137`), region names, and declared identifiers. This plan spends that reservation on precisely what it was saved for: the `$always` transition key and the `$done.state.*` / `$done.region.*` generated events. No new reserved-prefix rule is needed, and a user definition cannot collide with one.

The consequence: a macrostep is a **pure loop around the existing pure primitives**, holding one mutable working state and one queue in a stack frame, returning the same `Outcome` type it always did. Nothing in the store, the CLI, or the MCP layer learns a new control flow — they learn one new optional field in a trace.

## 0042 — Macrostep foundations

### The macrostep, defined once

A **macrostep** is: one triggering step (the *trigger microstep*), followed by zero or more *reaction microsteps*, run until quiescence. It is produced by all three pure entry points — `step` (`step/mod.rs`), `create` (`step/create.rs`), and `poll_deadline` (`step/deadline.rs`) — and it is atomic: either every microstep applies and the caller sees one `Applied`, or nothing applies and the caller sees one `Rejected`.

**All three, not just `step`.** Creation enters an initial configuration and runs entry blocks, so a machine whose initial state has an eventless exit or a `final` child must react before its first sealed state; a due deadline runs the ordinary transition pipeline and can cascade for the same reasons an event can. Task `4201` therefore owns `step/mod.rs`, `step/create.rs`, and `step/deadline.rs` together — wiring only `step` would leave two of the three entry points unable to produce the `microsteps` that `4601` journals and `4603` proves inert.

New file `crates/fsm-core/src/step/micro.rs` (task `4201`) owns the loop. It is the only new control flow in the engine.

```rust
// trace.rs — one struct serves the trace, the record projection (§0046), and replay (§0046)
pub enum MicrostepTrigger { Eventless, Internal(String) }
pub struct MicrostepTrace {
    pub index: u32,                 // 1-based; the trigger is index 0 and is the trace's own fields
    pub trigger: MicrostepTrigger,
    pub source_state: String,
    pub transition_idx: u32,
    pub region: Option<String>,
    pub exited: Vec<String>,
    pub entered: Vec<String>,
    pub candidates: Vec<LevelTrace>,
    pub pipeline: Vec<BlockTrace>,
}
pub struct UnhandledInternalTrace { pub event: String, pub after_microstep: u32 }
// DecisionTrace gains `microsteps: Vec<MicrostepTrace>` and `internal_unhandled`, both emitted only when non-empty

// micro.rs
pub struct InternalEvent { pub name: String, pub payload: BTreeMap<String, Val>, pub origin: InternalOrigin }
pub enum InternalOrigin { Raise { block: BlockKind }, DoneState { compound: String }, DoneRegion { region: String } }
pub struct ReactionSelection { pub region: Option<String>, pub leaf: u16, pub source: u16, pub transition_idx: usize, pub candidates: Vec<LevelTrace> }
pub trait ReactionSelector {
    fn select_eventless(&mut self, m, t, working: &InstanceState, budget) -> Result<Option<ReactionSelection>, Rejection>;
    fn select_internal(&mut self, m, t, working, event: &InternalEvent, budget) -> Result<Option<ReactionSelection>, Rejection>;
}
pub struct EngineSelector;   // the engine's scans: 4303 fills the eventless one, 4403 the internal one
```

The plan's first draft put a `MicrostepRecord` in `micro.rs` beside a `MicrostepTrace` in `trace.rs`; implementation collapsed them into the one `MicrostepTrace` above because the record shape is a projection of the trace shape and two structs carrying the same six fields would have been pure duplication. `Macrostep` (queue, microsteps, effects, reaction count) is the driver's private working aggregate, and the queue is a `VecDeque<InternalEvent>` held in a stack frame only — **it is never added to `InstanceState`**. The seam a test uses is the `ReactionSelector` trait, reached through `step_with`, `create_with`, and `poll_deadline_with`; `step`, `create`, and `poll_deadline` pass `EngineSelector`.

`pub(super) fn run_to_quiescence(m, t, state, trigger: Transitioned, now_ms, budget, selector) -> Result<Applied, Rejection>` is the loop; `Transitioned` is a microstep's pipeline result before its deadline schedules settle (see *Settlement order* below). One iteration, in exactly this order — the order is a normative ruling and belongs in SPEC:

1. **Eventless first.** Attempt an eventless selection over the current working configuration (§0043). If a transition is selected, apply it through the existing pipeline and continue to the next iteration.
2. **Then the internal queue.** If no eventless transition was selected and `queue` is non-empty, pop the **front** and attempt selection for that event name over the current configuration. If a transition is selected, apply it and continue.
3. **An unhandled internal event is discarded, not rejected — and it counts against the ceiling.** If the popped event selects nothing, record it in the trace as `internal_unhandled` and continue the loop with the next queue entry. The discard still charges one reaction against `MAX_MICROSTEPS`, because every iteration scans guards and the ceiling is what bounds the scans a macrostep can spend (see *The ceiling and the budget*). This is a ruling, and the reason is structural: rejecting would have to unwind an already-applied trigger microstep, and `on_unhandled: reject` is a statement about *callers* sending events the machine does not model — an engine-generated `$done.*` nobody listens for is not a caller error. **`on_unhandled` governs the trigger microstep only.** SPEC says so in §0047's edit.
4. **Quiescence.** No eventless transition selected and an empty queue ends the macrostep.

Ordering rule 1-before-2 matches SCXML's `selectEventlessTransitions` preceding the internal queue and, more importantly, makes the loop's fixpoint independent of how many events a block raised: eventless transitions describe the *shape* of the configuration, and draining them first means an internal event is always delivered to a settled configuration.

**Guards, blocks, history, deadlines, and invariants inside a microstep.** A reaction microstep runs the *same* `apply_selected_transition` pipeline the trigger microstep runs — exit blocks inner→outer, transition block, entry blocks outer→inner, history capture from the pre-*microstep* configuration, deadline rescheduling from the macrostep's single `now_ms`. Three deliberate exceptions, each a ruling:

- **Invariants are evaluated once, at quiescence** — not per microstep. An intermediate configuration is by definition mid-reaction, and tripping an enforce invariant on a state the machine was about to leave would make a correct machine unrunnable. Because there is exactly one evaluation, `monitor_flags` is that evaluation's list — a monitor that would have failed on an intermediate configuration and passes on the final one is not reported, and one that fails on the final configuration is reported once. **This requires moving an existing call:** `eval_invariants` runs inside `apply_selected_transition` today (`crates/fsm-core/src/step/transition.rs:213`), and a reaction microstep runs that same pipeline, so the call must move up into the driver — task `4201` owns that refactor and `transition.rs` is in its `touches` for exactly this reason.
- **`evt` binds only in the microstep whose trigger supplied it.** SPEC already says "only an event transition block sees `evt`". An eventless transition's block sees no `evt`; an internal event's block sees the raised payload as `evt`. A block that names `evt.x` in an eventless transition is a *compile-time* error (`def/eventless_evt`), caught by the existing expression scope machinery, not a runtime one.
- **`now_ms` is read once for the whole macrostep** and every microstep schedules from that same value. A macrostep is one instant. A state entered and then exited within one macrostep has its deadline scheduled and then removed; the net effect on `deadlines_after` is nothing, which falls out of the existing per-microstep rules and needs no special case — but the tests pin it, because "falls out" is a claim.

**Settlement order.** SPEC §Semantics orders invariants (8) before deadline scheduling (9), and a deadline-schedule failure's trace carries the invariant traces that were evaluated first; replay verifies those traces byte for byte. Moving invariants to quiescence must not reorder them past the trigger's own schedules on a non-reactive machine, so the driver defers each microstep's schedule settlement (`settle_schedules`: §9 rescheduling, terminal-region clearing, §10 status) until it knows another reaction follows; the last microstep settles after the invariants. A one-microstep macrostep therefore runs pipeline → invariants → schedules exactly as before, and a reactive one runs pipeline₀ → schedules₀ → pipeline₁ → … → invariants → schedulesₙ. Each microstep's deadlines are evaluated against the context that microstep left, which is the ruling SPEC's macrostep section records.

**Atomicity.** `run_to_quiescence` works on a *clone* of the post-trigger state. Any `Rejection` from any microstep — a guard error, an action error, an invariant failure at quiescence, or the ceiling below — returns that `Rejection` from the whole `step` call, and the caller's state is untouched. The `Rejection.trace` carries the microsteps that ran before the failure, because a permanent record that says only "microstep 4 failed" without showing microsteps 0-3 is a worse audit trail than the one we have today.

### The ceiling and the budget

`limits.rs` (task `4201`) gains two constants and the doc comment that justifies them:

```rust
/// Reactions allowed after the trigger microstep of one macrostep: applied
/// eventless transitions, applied internal-event transitions, and discarded
/// internal events all count.
pub const MAX_MICROSTEPS: u32 = 64;

/// Expression-evaluation ticks a whole macrostep may spend: one standard
/// budget for the trigger, one per reaction, and one for the closing scan
/// that proves quiescence and evaluates the invariants.
pub const MACROSTEP_EVAL_TICKS: u32 = MAX_EVAL_TICKS * (MAX_MICROSTEPS + 2);
```

The first draft wrote `MAX_MICROSTEPS + 1` and counted only applied microsteps. Both were unsound against SPEC's guarantee that an accepted definition never exhausts a fresh budget: the loop's closing iteration — the eventless scan that finds nothing, plus the invariants — is a full iteration's worth of ticks on top of the trigger and the reactions, and a discarded internal event costs a selection scan without producing a microstep, so unbounded discards would have been unbounded scans. Counting discards makes every iteration a reaction, `MAX_MICROSTEPS + 2` iterations is then an exact upper bound, and admission's per-iteration bound (every compiled slot at most once, plus one implicit `true` per omitted-guard event, all of which are distinct slots within one iteration) multiplies out.

**The ceiling is shared across regions, and that is not obvious.** Selection picks **one global winner** per microstep — for a parallel machine, across every region's chain in region document order — so two regions each wanting an eventless transition consume two microsteps, not one. A parallel machine's reaction budget is therefore divided among up to `MAX_REGIONS` = 8 regions: eight regions each running a three-step eventless cascade is 24 microsteps, not 3. Sixty-four is comfortable for that shape and tight for a pathological one, and an author who hits it deserves to find out at admission rather than at run time — which is why `4304` also reports a **static depth warning** (`def/eventless_depth`) when the longest acyclic eventless path times the region count approaches the ceiling. The analysis cannot know which branches a guard will take, so it is a warning and never a refusal.

Exceeding `MAX_MICROSTEPS` rejects the macrostep with the new code `run/microstep_limit`. **Every new code lands in `fsm_core::error::ALL_CODES`, SPEC Appendix A, and the naive-caller suites in the task that first makes the engine produce it** — `crates/fsm-cli/tests/naive_caller/tool_outcomes.rs::all_codes_hygiene` and `one_step_every_non_infra_code.rs` both fail for any catalogued code that no real tool outcome produces, so the first draft's plan to catalogue the whole closed set in `4201` would have left those gates red for the length of the plan. `run/microstep_limit` itself lands with `4303`, the first task whose definitions can loop. Its hint names the highest-index microstep and the transition that kept firing, because "it looped" is useless and "state `route` re-entered itself 64 times via transition 12" is a bug report.

**Why the budget is multiplied rather than the admission relaxed.** `def/limit_eval` continues to charge exactly what it charges today: the worst-case cost of **one** microstep. A macrostep is at most `MAX_MICROSTEPS + 1` microsteps, so `MACROSTEP_EVAL_TICKS` is a sound ceiling, and SPEC's guarantee — an accepted definition never exhausts a fresh standard budget — survives unchanged with "standard" now meaning the macrostep budget for a macrostep entry point. Do **not** raise `MAX_EVAL_TICKS`, and do **not** charge admission `nodes × 65`: the first weakens per-microstep bounds, and the second refuses every real machine.

Callers that supply their own budget are the store (`store/instance/*.rs`) and the executor. They construct `Budget::new(MAX_EVAL_TICKS)` today; §0046's task updates the three call sites that drive macrosteps to `Budget::new(MACROSTEP_EVAL_TICKS)`. An enabled-event **scan** is not a macrostep and keeps the standard budget — it evaluates guards for selection, never applies a pipeline.

### The validation module split

`crates/fsm-core/src/spec/validate.rs` is 746 lines of one `validate()` function plus `check_block_limits`, and `scripts/oversized-files.sh` refuses anything over 1000. Three feature workstreams each need to add validation, and serialising them behind one file for the whole plan would be an authoring artefact, not a design.

Task `4202` therefore converts it to a directory **with no behaviour change whatsoever**:

- `crates/fsm-core/src/spec/validate/mod.rs` — the public `pub fn validate(spec: &MachineSpec) -> Result<(), Vec<Finding>>`, the shared collection/ordering of `Vec<Finding>`, and `mod structure; mod blocks; mod reactive;`.
- `crates/fsm-core/src/spec/validate/structure.rs` — everything currently in `validate()` about names, trees, initials, history, terminals, regions, and limits, moved verbatim.
- `crates/fsm-core/src/spec/validate/blocks.rs` — `check_block_limits` and the assignment/dup-set rules, moved verbatim.
- `crates/fsm-core/src/spec/validate/reactive.rs` — created empty but for `pub(super) fn validate_reactive(spec: &MachineSpec, errs: &mut Vec<Finding>) {}` and a module doc naming the three workstreams that will fill it.

**Finding order is observable.** `spec_validate.rs`, `analyze_golden.rs`, and the MCP `machine_create` error payloads all depend on the order findings come back in. The split must preserve it exactly: call the moved helpers from `mod.rs` in the same sequence the original function ran them. This task's Done-when is that every existing test passes **unchanged** — if a golden moves, the split is wrong.

## 0043 — Eventless transitions

### Shape

`TransitionSpec.on` becomes `Option<String>` (`spec/mod.rs`, task `4301`). An omitted `on` key is an eventless transition; `"on": null` is `def/shape`, because an explicit null is a typo, not an intention.

`CompiledMachine.transitions_by: BTreeMap<(String, String), Vec<usize>>` is keyed `(from, on)`. An eventless transition is keyed `(from, "$always")` — the sentinel is a `pub const ALWAYS_KEY: &str = "$always"` in `spec/mod.rs`, and `def/reserved_ident` already guarantees no declared event can collide with it. Nothing else in the compile step changes: document order within the cell is preserved by the existing `Vec<usize>`.

`spec/parse/transitions.rs` (task `4301`) drops `on` from the required set and keeps it in `check_keys`'s allowed set. Serialization (`spec/serialize.rs`) omits the key when `None` — this is load-bearing, because `machine_id` hashes the canonical definition and an emitted `"on": null` would change the identity of a machine that does not use the feature. The round-trip test in `crates/fsm-core/tests/spec_parse.rs` is what pins it.

### Selection

Task `4303` adds `pub fn select_eventless(m, t, config, ctx, budget) -> Result<Option<SelectedTransition>, Rejection>` to `step/mod.rs`, reusing the existing candidate scan by parameterising the event key. The scan is *identical* to the event scan — `chain(leaf)` innermost-first for sequential, region document order then chain for parallel, skipping regions on terminal leaves, first true guard wins, `run/guard_error` never treated as false — with the cell key `(state, "$always")` instead of `(state, event)`.

Three rulings:

- **A guard sees no `evt`.** The expression scope for an eventless transition's `if`, `do`, and `emit` excludes `evt` entirely. Referencing it is `def/eventless_evt` at compile time, reported with the same span precision as any other unknown binding.
- **All false is not a rejection.** For an event, "candidates exist but every guard is false" is `run/not_enabled`, because a caller sent something and deserves an answer. For an eventless scan it means *quiescence with respect to eventless transitions* — the loop moves to the internal queue. This asymmetry is the single most likely thing to be implemented wrong; the tests pin it directly.
- **Eventless transitions are external unless `to` is absent.** `to`-absent is still internal (no exit/entry), and an internal eventless transition that changes nothing is precisely how a machine spins. It is legal — the ceiling catches the spin, and the cycle analysis below refuses the statically certain case — but `def/eventless_internal_noop` is a **warning** when an eventless transition has neither `to` nor `do` nor `emit` nor `raise`, because that transition cannot do anything except burn a microstep.

### Validation and cycle analysis

`spec/validate/reactive.rs` (task `4302`) adds:

| Code | Rule |
|---|---|
| `def/eventless_evt` | an eventless transition's guard or block references `evt` |
| `def/eventless_from_terminal` | an eventless transition's `from` is a terminal or final state (the existing `def/terminal_has_transitions` covers `on`-transitions; this is its eventless twin) |
| `def/eventless_shadowed` | a guardless or `true`-guarded eventless transition precedes another eventless transition from the same `from` — the eventless mirror of `def/shadowed` |
| `def/eventless_cycle` | error: a cycle in the eventless transition graph on which **every** transition is guardless or `true`-guarded |
| `def/eventless_internal_noop` | warning: an eventless transition with no `to`, `do`, `emit`, or `raise` |
| `def/eventless_cycle_guarded` | warning: a cycle in the eventless transition graph with at least one guarded transition |

The cycle analysis (task `4304`, in `analyze.rs` beside the existing `reachability_findings` / `ancestor_shadowed` machinery) builds a directed graph whose nodes are states and whose edges are eventless transitions resolved through the same target rules `step` uses (`history_descent`, external self-transition, `properLCA`), then runs an iterative Tarjan SCC — iterative, not recursive, because `MAX_STATES` is 256 and depth 12 but a hostile definition should not be able to blow the stack, and `diagram_hostile.rs` sets the precedent for caring about that.

The graph is over leaves, because a configuration is a leaf, and from each non-terminal leaf it holds exactly the candidates the scan could select, in scan order — every guarded eventless transition up the chain until the first guardless (or literal-`true`) one, which is certain and ends the walk, since nothing after a certain winner can fire. An internal (`to`-absent) eventless transition is a self-edge: the configuration it leaves unchanged re-selects it.

For each SCC with more than one node, or a single node with a self-edge: if every node in the SCC has a certain edge **and** every edge the scan could select from any node stays inside the SCC, emit `def/eventless_cycle` naming the states in document order — whatever the guards say, this machine **provably** cannot leave the cycle, so it is refused at admission and never reaches a journal. (The first draft said "every edge in the SCC guardless", which both refuses a cycle a guarded leaf transition can steer out of and misses a certain cycle sitting under a shadowed exit.) Otherwise emit the `def/eventless_cycle_guarded` warning, naming the same cycle and saying plainly that the engine cannot decide it and the microstep ceiling is what will stop it. Analysis findings never refuse a definition on their own — `machine_create` reports them as warnings — so `spec/compile.rs` runs the same function after expression binding and treats its `Error`-severity findings as refusals; that is how a certain cycle becomes an admission error rather than advice.

Guard *truth* is not evaluated here — `expr/partial.rs`'s three-valued partial evaluator could prove more, and deliberately is not used: admission must be a pure function of the definition, and a partial evaluation over an unknown context would make admission depend on which context a caller might supply. Syntactic guardlessness (`if` absent, or the literal `true`) is the whole test.

## 0044 — Internal events

### Declaring one

`events` entries gain an optional `internal: true` (task `4401`, `spec/parse/decls.rs`). An internal event is an ordinary declared event — a name and typed `fields` — with two differences:

- **It cannot be sent from outside.** `step`'s `validate_event` refuses it with the new `req/event_internal`, whose hint says the event is raised by the machine and names the states whose blocks raise it. This is a `req/*` code because it is a caller mistake, and it sits beside `req/event_unknown` in SPEC's `run/*` catalogue table.
- **It is excluded from `enabled_events`.** `analyze::enabled_events` skips internal events entirely rather than reporting them `disabled`; a caller reading that list is deciding what to send, and an event they can never send is noise. Task `4703` adds a separate `internal_events: Vec<String>` to the analysis output so the information is not lost.

An internal event may still be the `on` of any number of transitions, and its payload types are checked exactly like any other event's.

### Raising one

`Block` gains `raises: Vec<RaiseSpec>` beside `sets` and `emits` (task `4402`):

```rust
pub struct RaiseSpec { pub event: String, pub with: Vec<(String, String)> }
```

`with` maps declared field names to `expr/1` source, and it is validated exactly like an `emit`'s args: every declared field present, no extras, RHS type equal to the declared field type including decimal scale (`def/assign_type`). `parse_block` in `spec/parse/states.rs` adds `"raise"` to its `check_keys` allow-list, and `spec/parse/transitions.rs` forwards the key into the synthetic block object it already builds for `do`/`emit`.

New limit `MAX_RAISES_PER_BLOCK = 8` in `limits.rs`, mirroring `MAX_EMITS_PER_BLOCK`, enforced by `check_block_limits` as `def/limit_raises`. It joins the genesis `limits` block? **No** — and this is the important part: `MAX_RAISES_PER_BLOCK` is *not* added to the genesis limits object, for exactly the reason SPEC gives for `MAX_PAYLOAD_BYTES`: that block is hash-verified on fold, and adding a key would make every store written by an earlier build unreadable rather than migratable. §0046 restates this.

The raise evaluates its `with` expressions **inside the block's snapshot semantics**: all RHS evaluate against the ctx the previous block left, exactly like `do` and `emit`, and a raise in a block that is later discarded (the pipeline's "computed-but-discarded values of completed blocks stay in the trace" rule) does **not** enqueue. Only a committed block's raises reach the queue.

### Ordering

Task `4403` fixes the queue's contract in `step/micro.rs`, and it is FIFO with one deterministic enqueue order:

- Within one block, raises enqueue in **document order**.
- Blocks run in the pipeline's existing order — exit blocks inner→outer, then the transition/deadline block, then entry blocks outer→inner — so a macrostep's enqueue order is fully determined by the definition, never by a map iteration.
- The queue is drained from the **front**, one event per loop iteration, and a microstep triggered by an internal event may itself raise more, which append to the **back**. Breadth-first, not depth-first: an event raised while handling `a` is delivered after every event already waiting, which is the only order that makes "raised together, delivered together" true.

`MAX_MICROSTEPS` bounds the total, so an unbounded raise chain is a `run/microstep_limit` rejection and not a hang. There is deliberately no separate queue-length limit: one more constant to explain, and the microstep ceiling already dominates it.

## 0045 — Done events

### `final` is not `terminal`

`terminal: true` today means "this leaf ends the machine" (sequential) or "this leaf ends the region" (parallel), per SPEC §Semantics 10. That is exactly the meaning we need for regions and exactly the wrong meaning for a compound state: a nested workflow that finishes should not complete the whole instance.

Task `4501` adds `final: true` to the state shape (`spec/parse/states.rs`'s key list becomes `["name", "terminal", "final", "history", "initial", "entry", "exit", "states"]`), with these rules in `spec/validate/reactive.rs`:

| Code | Rule |
|---|---|
| `def/final_not_leaf` | a `final` state has children |
| `def/final_at_root` | a `final` state's parent is the machine root or a region root — use `terminal` there, which already means what you want |
| `def/final_and_terminal` | `final` and `terminal` are both true on one state |
| `def/final_has_transitions` | a transition (event, eventless, or deadline) has a `final` state as its `from` |
| `def/final_is_initial` | a compound's `initial` names its `final` child, which would complete the compound before it began |

`final` states are otherwise ordinary leaves: they have entry blocks, they can be targeted, and history binds through them.

### Generation

When a microstep's entry set contains a `final` state, the engine enqueues `$done.state.<parent-compound>` at the **end** of the entry pipeline for that microstep, after every entry block has run and after any raises those blocks produced. Rationale: `$done.state.X` means "X's inner workflow finished, including its final state's entry actions", so any action the final state performs must be visible to the handler.

When a **region's** active leaf becomes `terminal`, the engine enqueues `$done.region.<region-name>` under the same rule. This is the join primitive: a transition in region B written `{"from": "wait_for_a", "on": "$done.region.review_a", "to": "proceed"}` fires the moment region A finishes, inside the same macrostep, sealed in one record.

Both are enqueued into the same FIFO from 0044 and are indistinguishable from a user `raise` once queued, except that `InternalOrigin` records which they were, for the trace.

Five rulings, all of which belong in SPEC:

- **A generated event nobody handles is never raised.** (Added by `4603`, whose inertness suite found a plain parallel machine's trace gaining `internal_unhandled` the moment a region finished.) The driver raises `$done.state.X` or `$done.region.X` only when some transition names it in `on`; otherwise the event could only be discarded, and the discard would show in the trace and count toward the microstep ceiling of a definition that never asked for it. Discovering an unhandled generated name is `analyze`'s job, not the trace's.

- **`$done.machine` does not exist.** A sequential instance whose leaf is terminal, or a parallel instance whose every region is terminal, is `Completed`, and the status carries that fact. There is nothing left to handle it — every region is inert — so generating it would be a queue entry that is guaranteed to be discarded. Say so in SPEC rather than leaving the reader to wonder.
- **A completed region is inert, and that is unchanged.** SPEC §Semantics 9 already removes schedules sourced from a terminal region's chain and makes completed regions inert to events and deadline polls. `$done.region.X` is delivered to the *other* regions; a transition sourced inside region X cannot handle its own region's done event, and the existing region-skipping in the candidate scan enforces that without new code. `def/cross_region` still forbids a transition targeting another region, so a join moves region B's own leaf — it does not reach into A.
- **`$done.*` events are not declared and not sendable.** They never appear in `spec.events`. `validate_event` refuses any `$`-prefixed event name from the external path with `req/event_internal`. `def/unknown_event` is extended (task `4502`) to accept `on: "$done.state.<X>"` when `X` is a compound owning at least one `final` descendant child, and `on: "$done.region.<X>"` when `X` is a declared region name; anything else `$done.`-shaped is `def/unknown_event` with a hint listing the valid generated names for this machine. That hint is the feature's discoverability, so write it well.
- **They carry no fields.** The payload is empty and `evt` binds to an empty object in a handling transition's block. A join that needs data reads `ctx`, which the finishing region already wrote.

Task `4503` handles the region case and the interaction that only exists there: **a single macrostep can complete two regions.** A transition changes only its own region, so no single microstep finishes two regions, and the same-microstep rule — region document order, the total order everything else in this engine uses — is pinned on `done_region_events` alone. Across microsteps the events enqueue as the regions finish: when region B finishes in the reaction to region A's event, A's event was already consumed, so document order has nothing to reorder. (Corrected by `4503`; the earlier text read as if document order applied across microsteps.)

## 0046 — Persistence & compatibility

### The record

`event_applied`, `deadline_applied`, and `instance_created` bodies gain **one optional key**, `microsteps` (task `4601`, `record.rs` and `store/instance/send.rs` / `poll.rs` / `create.rs`):

```json
"microsteps": [
  {"index": 1, "trigger": "eventless", "source_state": "route", "transition_idx": 7,
   "exited": ["route"], "entered": ["approve"]},
  {"index": 2, "trigger": "internal", "event": "$done.state.approve", "source_state": "review",
   "transition_idx": 9, "exited": ["review", "approve"], "entered": ["done"]}
]
```

- `index` starts at **1**: index 0 is the trigger microstep, and it is already described by the record's existing `exited`, `entered`, `source_state`, and (for deadlines) `deadline_idx` fields. Those fields keep their exact current meaning — they describe the trigger, not the union. Anything else would silently redefine every field a fold checks.
- `trigger` is `"eventless"` or `"internal"`; an `"internal"` entry carries `event`. There is no `"event"` trigger value, because index 0 is not in this array.
- **The key is absent, not empty, when there were no reaction microsteps.** This is the compatibility anchor of the entire plan: a definition with no eventless transitions, no `raise`, and no `final` states produces a body with no `microsteps` key, hence identical canonical bytes, hence an identical record hash, hence an identical chain. Emitting `"microsteps": []` would change every hash in every store on earth. The test for this is not optional; it is task `4603`.
- `state_hash` is the hash after the **whole** macrostep and its meaning does not change. `fsm.state/2` does not move.

### Fold and replay

`replay/apply.rs` and `replay/verify.rs` (task `4602`) re-apply through the same macrostep entry points, so the microsteps re-derive rather than being trusted from the record. Verification then checks the journaled `microsteps` array against the re-derived one **when the key is present**, and checks that no reaction microsteps were derived **when it is absent**. That second half is what turns the array from decoration into a tamper-evident claim.

A journal written before this plan can contain a machine that, recompiled under the new engine, is still non-reactive — the features are opt-in syntax, so an old definition cannot acquire an eventless transition. No historical-compiler exception is therefore needed, and none may be added: SPEC's existing legacy-compiler rule stays scoped to the history-shape bug it was written for. Task `4602`'s Done-when includes folding the committed legacy fixtures unchanged.

The three macrostep call sites move to `Budget::new(MACROSTEP_EVAL_TICKS)`: `store/instance/send.rs`, `store/instance/poll.rs`, `store/instance/create.rs`. `replay/apply.rs` does the same, or replay of a legitimate deep macrostep would fail where the original write succeeded — a divergence that would surface as `StateHashMismatch` on a healthy store, which is the worst possible failure mode.

### The inertness suite

Task `4603` is a proof obligation, not a feature: `crates/fsm-core/tests/reactive_inertness.rs` plus a store-level leg.

- Every machine in `examples/` and every fixture machine in the existing golden suites is non-reactive. Assert that for each: `machine_id` is byte-identical to the value recorded in the committed goldens; a create/step/poll sequence produces record bodies with no `microsteps` key; and the resulting `state_hash` values match the existing `step_golden.rs` / `record_golden.rs` expectations.
- Re-fold a committed legacy journal fixture and assert `Ok` with an unchanged final `state_root`.
- Assert `MACROSTEP_EVAL_TICKS` never actually binds for a non-reactive machine by asserting the budget consumed for a one-microstep macrostep equals the budget the same operation consumed before the plan, using the existing budget accounting.

If any of these fail, the plan has broken its own central promise and the failure is not negotiable.

## 0047 — Surface & proof

**Traces and explain (task `4701`).** `DecisionTrace` (`trace.rs`) gains `pub microsteps: Vec<MicrostepTrace>`, where each entry carries its own `candidates`, `pipeline`, and the trigger that selected it — the existing three fields describe microstep 0 and keep describing it. `DecisionTrace::to_value` emits `microsteps` **only when non-empty**, for the same canonical-bytes reason as the record. `store/view.rs::explain_seq` renders each microstep as its own indented section under the trigger's, and `render.rs`'s human output gets a `→ microstep 2 (internal $done.state.approve): review → done` line form. The existing `trace_render.rs` goldens must not move for non-reactive machines.

**Analysis and diagram (task `4702`).** `machine_analyze` output gains `eventless_transitions` (count and the cycle warnings from 0043) and `done_events` (the generated names this machine can produce). `diagram.rs` renders an eventless transition with an empty label and a dashed arrow in Mermaid (`-->` becomes `-.->`) and `style=dashed` in DOT; a `final` state gets the double-circle shape Mermaid spells `(((name)))` and DOT spells `shape=doublecircle`. `diagram_hostile.rs` gains cases for a definition whose state names collide with the generated `$done.` names once escaped — the `$` cannot appear in a user name, but the escaping path must still be exercised.

**Simulate and enabled events (task `4703`).** `simulate` runs macrosteps like everything else, and its per-event report gains the microstep list so a caller can see the cascade a single event caused — this is the feature's main authoring affordance and the reason `simulate` exists. `enabled_events` keeps its exact current meaning (which events, sent now, would select a transition in the trigger microstep) and gains the `internal_events` sibling list from 0044. It does **not** attempt to predict the cascade: doing so would mean running speculative macrosteps for every declared event under a scan budget, and the honest answer to "what will happen" is `simulate`.

**The oracle (task `4704`).** `crates/fsm-core/tests/oracle/` already differentially tests `step`, `create`, and `deadline` against a deliberately naive second interpreter over exhaustively enumerated small trees (`enumerate_small/`). Extend the naive interpreter with the macrostep loop written the dumbest possible way — a `Vec` used as a queue, a linear scan for eventless candidates, no memoisation — and extend the tree enumerator to emit eventless transitions, one `raise`, and one `final` state. The oracle must agree on the final state, the effect list, the microstep sequence, and the rejection code including `run/microstep_limit`. Generated machines that the cycle analysis refuses are asserted refused by both, which is how the analysis itself gets differentially tested.

**SPEC and README (task `4705`).** `docs/SPEC.md` gains a `### Macrosteps` subsection under `## Semantics` covering the loop order, the three exceptions, atomicity, the ceiling, and the queue's non-persistence; `## Machine definitions` gains the optional `on`, `internal`, `raise`, and `final` shapes; the `def/*` and `run/*` tables gain every code above; `### Record kinds` gains the optional `microsteps` key with its absence rule; and `## Appendix B — Limits` gains `MAX_MICROSTEPS` and `MAX_RAISES_PER_BLOCK`. `crates/fsm-cli/tests/spec_appendix.rs` already asserts every code in `ALL_CODES` appears in the appendix, so that test is the mechanical gate.

`README.md`'s guarantees table is edited in exactly one row, and the edit is a strengthening rather than a retreat — say what is true:

| Guarantee | What it means |
|---|---|
| one-event-one-macrostep | at most one transition fires for the event you sent; the machine may then react to itself to quiescence, bounded, in the same atomic record |

Add one row to the honest non-claims paragraph: reaction is bounded at 64 microsteps and a machine that needs more is refused at run time, not truncated.
