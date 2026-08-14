# Architecture — Plan 0003

> The concrete deltas, by symbol.

## 0012 — Spec Model

Task `1201` is the plan's first task, so it wires the modules: `crates/fsm-core/src/lib.rs` gains `pub mod tree; pub mod spec; pub mod machine; pub mod step; pub mod trace; pub mod analyze; pub mod simulate; pub mod hashes;` with empty stub files, so later tasks fill files without touching `lib.rs` again.

`crates/fsm-core/src/spec.rs` (task `1201`) parses the `fsm.machine/1` JSON format into a typed model:

- Top-level keys: `format` (must be `"fsm.machine/1"`), `name`, optional `description`, optional `enums` (name → variant list), `context` (array), `events` (array), optional `effects` (array), `states` (recursive tree), `initial`, optional `on_unhandled` (`"reject"` default | `"ignore"`), `transitions` (**flat top-level array** — document order within a source is order among entries sharing `from`; canonicalization and `transition_idx` are untouched by nesting), optional `invariants`.
- Model types: `MachineSpec`, `CtxVar { name, ty: TySpec, init: String }`, `TySpec` (`int`, `decimal` with `scale`, `str`, `bool`, `enum` with `of`, `timestamp`, `duration`), `EventDecl { name, fields: Vec<FieldDecl> }`, `EffectDecl { name, fields }`, `StateNode { name, terminal: bool, history: Option<HistoryKind>, initial: Option<String>, entry: Option<Block>, exit: Option<Block>, states: Vec<StateNode> }`, `HistoryKind { Shallow, Deep }`, `Block { sets: Vec<SetSpec>, emits: Vec<EmitSpec> }` (JSON keys `do` and `emit`), `SetSpec { target, value: String }`, `EmitSpec { effect, args: BTreeMap<String, String> }`, `TransitionSpec { from, on, guard: Option<String>` (JSON key `if`)`, sets, emits, to: Option<String> }`, `InvariantSpec { name, expr, mode: Enforce | Monitor }`.
- Shape errors carry JSON-Pointer paths (e.g. `/states/1/states/0/entry/do/2`): unknown keys → `def/unknown_key`; wrong JSON types → `def/shape`; raw JSON number tokens anywhere a value is expected → `req/number_token` (numerics are strings, everywhere, always).
- The reference fixture `crates/fsm-core/tests/fixtures/machines/case_review.json` is committed verbatim from the approved plan: machine `case_review` with states `intake`, compound `in_review` (entry counts `visits` and emits `notify`, exit zeroes `notes`; children `resume_review` deep history, `docs_review`, `risk_review` with an entry block zeroing `score`), `suspended`, terminal `approved`/`rejected`; events `docs_ok`, `scored`, `note_added`, `withdraw`, `suspend`, `resume`; ancestor-sourced, internal, and history-targeting transitions; invariant `score_range` enforced. It exercises every construct in this plan and is the seed fixture for goldens here and sessions in plans 0005–0006.

Structural validation (task `1202`), `pub fn validate(spec: &MachineSpec) -> Result<(), Vec<Finding>>` in `spec.rs` — every rule with its own `def/*` code:

| Code | Rule |
|---|---|
| `def/dup_name` | State and history-pseudostate names share one global namespace; every name unique machine-wide |
| `def/one_initial`, `def/initial_not_child` | Every compound declares exactly one `initial`, naming a *direct real child* (never a history pseudostate → `def/initial_is_history`) |
| `def/unknown_state`, `def/unknown_event`, `def/unknown_effect`, `def/unknown_enum` | Every `from`/`to`/`initial`/`on`/`emit`/enum reference resolves |
| `def/terminal_not_leaf` | Terminal states must be leaves |
| `def/terminal_has_transitions` | No transition may have a terminal state as `from` |
| `def/initial_terminal` | The creation entry chain's leaf must not be terminal (a machine born completed is a spec bug) |
| `def/multiple_history` | At most one history pseudostate per compound |
| `def/from_history` | A history pseudostate can never be a transition source |
| `def/history_target_from_inside` | A history pseudostate may only be targeted by transitions whose source is outside the owning compound (inside-jumps hit stale bindings; direct state targets cover every legitimate inside case) |
| `def/reserved_ident` | `$`-prefixed identifiers rejected everywhere (names, fields, events, effects) |
| `def/not_supported` | The keys `regions` and `deadlines` are recognized and rejected with "not yet supported", so their later addition is non-breaking |
| `def/limit_*` | Size limits: ≤ 256 state nodes (history pseudostates included), nesting depth ≤ 12, ≤ 32 history pseudostates, ≤ 128 events, ≤ 32 enums (≤ 64 variants each), ≤ 2048 transitions and ≤ 32 per (state, event), ≤ 64 context variables, ≤ 32 fields per event/effect, ≤ 32 sets + 8 emits per block, ≤ 64 invariants, definition ≤ 256 KiB |

Canonical identity (task `1203`), `crates/fsm-core/src/hashes.rs`:

- `pub fn domain_hash(tag: &str, v: &Value) -> [u8; 32]` — `sha256(tag_bytes ‖ 0x0A ‖ canon_bytes(v))`; domain separation prevents cross-kind collisions.
- `pub fn machine_id(canonical_def: &Value) -> String` — `"{name}@sha256:{64 hex}"` with tag `fsm:machine:1` over the entire canonical definition *including* `description` strings (two definitions differing only in prose are different versions — the defensible audit position).
- `pub fn resolve_machine_ref<'a>(ids: impl Iterator<Item = &'a str>, query: &str) -> Result<String, ResolveError>` — accepts a full id, a `name@` + unique hex prefix ≥ 12, or a bare name when exactly one version exists; ambiguity errors list the candidate versions (`req/machine_ambiguous` at the API layer).

## 0013 — Static Checks

Expression binding (task `1301`), in `spec.rs` plus `crates/fsm-core/src/machine.rs`:

- `pub fn compile(spec: MachineSpec) -> Result<CompiledMachine, Vec<Finding>>` — parses and typechecks every expression with the correct scope: guards and transition sets/emits bind `ctx` + the transition's event fields (`ScopeKind::Guard` / `TransitionAction`); entry/exit blocks bind `ctx` only (`ScopeKind::Block`); invariants bind `ctx` only (`ScopeKind::Invariant`); emit args typecheck against the declared effect field types.
- `def/assign_type`: a `set` target's declared type must equal the RHS type **exactly, scale included** — every scale change is a visible `round`/`dec` call in the definition (this is where "no implicit rounding" is enforced at the machine level). `def/dup_set`: duplicate set targets within one block are a definition error (across blocks is legal; the pipeline order resolves it deterministically).
- `pub struct CompiledMachine { pub machine_id: String, pub spec: MachineSpec, pub canonical: Vec<u8>, pub transitions_by: BTreeMap<(String, String), Vec<usize>>, pub compiled_exprs: … }` — `transitions_by` maps (source state, event) to document-ordered indices into `spec.transitions`; compiled expressions are stored with their inferred types and verbatim sources for spans.

Static analysis (task `1302`), `crates/fsm-core/src/analyze.rs` — `pub fn analyze(m: &CompiledMachine, t: &Tree) -> Findings` with `pub struct Finding { pub severity: Error | Warning, pub code: &'static str, pub message: String, pub path: String, pub span: Option<Span>, pub hint: String }`:

- **Enterable-set reachability** (exact, not approximate): seed with the creation entry chain; each transition from an enterable source contributes its full entry set (target path, initial descents). History targets are modeled as the owner's initial chain, justified by the stated lemma — *history never extends the reachable set*, since deep/shallow bindings can only name configurations that were previously active, and a shallow child's initial descent requires that child reachable some other way first. Guard-optimistic (guards assumed satisfiable). Unenterable states → warning `def/unreachable_state`.
- **Completeness matrix**: rows = leaves, columns = events; each cell is `handled@<source_state>` (the innermost chain level declaring a transition for that event) or `unhandled(<policy>)`. The `@level` annotation is the hierarchy payoff for diagnosis.
- **Shadowing, pinned claims** (nothing more is promised): within one (from, on) group, a guardless or literal-`true` transition preceding later entries → error `def/shadowed`; two entries with structurally identical span-stripped guard ASTs → error `def/duplicate_guard`. Ancestor-vs-descendant handling of the same event is **legal by design** (child-first override is the feature); warning `def/ancestor_shadowed` fires only when the ancestor's transition is provably globally dead: for every leaf under the ancestor, some strictly-inner state on that leaf's chain declares a guardless/`true` transition on the same event, or masks it with an identical-normalized-guard transition — both conditions decidable.
- **`def/create_always_fails`**: const-fold the creation entry chain with declared initial values (all initializers are literals); if every path deterministically errors (e.g. an entry-block overflow independent of overrides), report it. Conservative: only provable failures are reported.

## 0014 — Engine

Tree machinery (task `1401`), `crates/fsm-core/src/tree.rs` — isolated so LCA/history logic is testable alone:

- `pub struct Tree { names: Vec<String>, parent: Vec<Option<u16>>, depth: Vec<u8>, children: Vec<Vec<u16>>, initial_child: Vec<Option<u16>>, kind: Vec<NodeKind>, index: BTreeMap<String, u16> }` with `pub enum NodeKind { Leaf, Compound, History(HistoryKind) }`; built once per `CompiledMachine` by `pub fn build(states: &[StateNode]) -> Tree`.
- `pub fn chain(&self, leaf: u16) -> impl Iterator<Item = u16>` (leaf → top-level, innermost first); `pub fn proper_lca(&self, a: u16, b: u16) -> Option<u16>` (`None` = the implicit root, which is unnamed, has no blocks, and is never exited or entered); `pub fn exit_set(&self, leaf: u16, dom: Option<u16>) -> Vec<u16>` (inner → outer, from the leaf up to the child of `dom`); `pub fn entry_path(&self, dom: Option<u16>, target: u16) -> Vec<u16>` (outer → inner); `pub fn initial_descent(&self, from: u16) -> Vec<u16>` (descend `initial_child` to a leaf); `pub fn history_descent(&self, hist: u16, binding: Option<&str>) -> Vec<u16>` (deep bound leaf: path from owner to that leaf; shallow bound child: the child then its initial descent; unbound: the owner's initial descent).

`crates/fsm-core/src/machine.rs` (task `1401`): `pub enum Status { Running, Completed, Cancelled }`; `pub struct InstanceState { pub status: Status, pub leaf: String, pub ctx: BTreeMap<String, Val>, pub history: BTreeMap<String, String>, pub pending: Vec<String> }` — `history` maps compound name → bound state name and **is hashed state**; `pending` holds shell-issued effect ids (the pure crate treats them as data).

Transition selection (task `1402`), `crates/fsm-core/src/step.rs`:

- Candidate collection: for each state on `chain(leaf)` innermost-first, the document-ordered transitions in `transitions_by[(state, event)]`; an empty candidate list across the whole chain is `run/unhandled` (the only case where `on_unhandled: "ignore"` applies). Guards evaluate **against the pre-transition (ctx, evt) only**, in candidate order, under the shared `Budget`; a guard evaluation error anywhere aborts the whole event with `run/guard_error` (source state, transition index, span, operand values) — never "treat as false". The first true guard wins; later candidates are traced `not_considered`. All guards false → `run/not_enabled` with per-chain-level guard traces (the caller sees whether to fix the payload or add a child override).

Apply pipeline and creation (task `1403`), `step.rs` — the full pure decision procedure:

- `pub enum Outcome { Applied(Applied), Rejected(Rejection), Ignored }`; `pub struct Applied { pub leaf_after: String, pub ctx_after: BTreeMap<String, Val>, pub history_after: BTreeMap<String, String>, pub effects: Vec<EffectOut>, pub monitor_flags: Vec<String>, pub status_after: Status, pub internal: bool, pub source_state: String, pub transition_idx: u32, pub exited: Vec<String>, pub entered: Vec<String>, pub trace: DecisionTrace }`; `pub struct EffectOut { pub name: String, pub args: BTreeMap<String, Val>, pub k: u32 }` — effect ids (`{instance}/{seq}/{k}`) are composed by the shell; the pure core supplies only `k`.
- `pub fn validate_event(m: &CompiledMachine, name: &str, payload: &Value) -> Result<BTreeMap<String, Val>, Rejection>` — declared event (`req/event_unknown`), exact field set (`req/field_missing` / `req/field_unknown`), values parsed at declared types with **no raw JSON number tokens** (`req/number_token`, `req/field_type`, `req/field_scale` — shorter decimal fractions widen exactly, longer are errors, never rounded).
- `pub fn step(m: &CompiledMachine, t: &Tree, st: &InstanceState, event: &str, payload: &BTreeMap<String, Val>, budget: &mut Budget) -> Outcome`, pure:
  1. Status gate: `Completed` → `run/instance_completed`; `Cancelled` → `run/instance_cancelled`.
  2. Select per task `1402`.
  3. Resolve the target: `to` absent → internal (no exit/entry, leaf unchanged; observably different from an external self-transition, which computes `dom = parent(from)` and exits/re-enters); a history target resolves through `history_descent` with the instance's current binding.
  4. Action pipeline, ctx threading block to block: exit blocks inner → outer, then the transition block (**the only block that sees `evt`**), then entry blocks outer → inner. Each block is snapshot-internal (all RHS evaluated against the ctx left by the previous block, then applied atomically). Emits collect across all blocks in pipeline order under one global `k` counter. Any evaluation error in any block → `Rejected(run/action_error)` naming the block (`exit(state)` | `transition` | `entry(state)`) and span, with computed-but-discarded values of completed blocks preserved in the trace.
  5. History capture: for each compound in the exit set owning a history pseudostate, capture from the **pre-transition** configuration (deep = the pre leaf; shallow = the owner's direct child on the pre chain) into `history_after`, atomically with the transition.
  6. Invariants: **all** evaluated on the final ctx, never short-circuited; any `enforce` failure or evaluation error → `Rejected(run/invariant)` listing every failing invariant with traces; `monitor` failures collect into `monitor_flags` and never block.
  7. `status_after = Completed` iff the new leaf is terminal. Rejection at any point discards everything — ctx, configuration, history bindings, effects (`step` is pure; the caller commits only `Applied`).
- `pub fn create(m: &CompiledMachine, t: &Tree, overrides: &BTreeMap<String, Val>) -> Result<Applied, Rejection>` — ctx₀ = declared inits + validated overrides; enter the root's initial descent outer → inner running entry blocks per the pipeline; collect effects; evaluate all invariants; `history` starts empty (creation exits nothing, so no captures). Creation failure is `run/create_failed` wrapping the inner error and full trace; there is no prior instance state, so the outcome is a pure function of (definition, overrides) — the shell never journals it, and no id or seq is consumed.

Traces and state hash (task `1404`):

- `crates/fsm-core/src/trace.rs`: `pub struct DecisionTrace { pub candidates: Vec<LevelTrace>, pub pipeline: Vec<BlockTrace>, pub invariants: Vec<InvariantTrace> }` — `LevelTrace { source_state, transitions: Vec<CandidateTrace { transition_idx, guard: Evaluated(TraceNode) | NotConsidered } > }` in chain order; `BlockTrace { block: Exit(String) | Transition | Entry(String), sets: Vec<SetTrace { target, before, after, expr: TraceNode }>, emits: Vec<EmitTrace> }`; rejections carry the completed blocks' traces marked discarded. `pub fn to_value(&self) -> Value` renders the whole trace as JSON for `explain` and the API layer.
- `crates/fsm-core/src/hashes.rs` gains `pub fn state_hash(machine_id: &str, instance_id: &str, seq: u64, st: &InstanceState) -> String` — tag `fsm:state:1` over the canonical Value `{format: "fsm.state/1", machine_id, instance_id, seq, status, state: <leaf>, ctx: {name: canonical string}, history: {compound: state}, pending: [sorted ids]}` (BTreeMap ordering makes ctx/history sorting free).

## 0015 — Assembly

Simulation (task `1501`), `crates/fsm-core/src/simulate.rs`:

- `pub enum OnReject { Stop, Continue }`; `pub fn simulate(m: &CompiledMachine, t: &Tree, overrides: &BTreeMap<String, Val>, events: &[(String, Value)], on_reject: OnReject) -> SimReport` — drives `create` then `step` per event with a fresh `Budget` each, collecting `SimStep { index, event, outcome, leaf_after, ctx_after, effects }` and a final summary (`final_leaf`, `terminal`, `stopped_at`). Pure; no persistence, no ids.

Enabled events (task `1502`), in `analyze.rs`:

- `pub enum EventStatus { Enabled, Disabled, DependsOnPayload, Preempted, PreemptedMaybe }`; `pub fn enabled_events(m: &CompiledMachine, t: &Tree, st: &InstanceState, budget: &mut Budget) -> Vec<EventReport>` — for each declared event, walk the chain in conflict order applying `partial_eval_bool` (ctx concrete, `evt.*` unknown): a definitely-true candidate → `Enabled` (everything after it `Preempted`); an `Unknown` candidate → `DependsOnPayload` (later candidates `PreemptedMaybe`); all false → `Disabled`. The per-event summary is the first non-preempted status down the chain; `EventReport` carries per-candidate detail (source state, transition index, truth) plus, for `DependsOnPayload`, the payload field names the guard reads.

Oracle differential (task `1503`), tests only:

- `crates/fsm-core/tests/oracle.rs`: a deliberately naive second interpreter — direct recursive tree walk over `MachineSpec` with no precomputed tables, recomputing exit/entry paths by walking parents, implemented for clarity over speed — exposing the same `step`/`create` signature.
- `crates/fsm-core/tests/enumerate_small.rs`: exhaustive enumeration of machines with depth ≤ 3, ≤ 5 states, ≤ 1 history pseudostate, ≤ 2 events, guards drawn from the pool `{none, true, false, ctx.b, not ctx.b}` over a single Bool context variable, ≤ 1 set per block from a two-assignment pool — crossed with all event sequences of length ≤ 4. For every run: `step` ≡ oracle on outcome kind, leaf, ctx, history, and effect order; rejected outcomes leave state bit-identical; `analyze` reachability agrees with the brute-force enterable set; the budget never trips.
- `crates/fsm-core/tests/step_golden.rs` + `crates/fsm-core/tests/fixtures/scenarios/*.json`: ordering goldens **authored from SPEC prose first**, pinning exact `exited`/`entered`/pipeline/effect sequences for the scenarios — external self-transition, ancestor target, descendant target, transition to an ancestor of the source, deep history, shallow history, unbound history, internal transition, and the creation chain (the `case_review` fixture covers several; small dedicated fixtures cover the rest).
- `crates/fsm-core/tests/history_props.rs`: seeded (xorshift, seed printed on failure) random walks over the `case_review` machine and generated trees — suspend anywhere, resume via deep history: the pre-suspend leaf is restored and entry blocks observably re-run (the `visits` counter increments); internal transitions never change leaf or history; rejected events never change history bindings.

## 0016 — Docs

Task `1601` appends the normative `## Semantics` section to `docs/SPEC.md` (created by plan 0002): the decision procedure as numbered pseudocode (status gate → candidate scan → guard evaluation → target/dom resolution → pipeline → history capture → invariants → status), the block ordering and snapshot rule with the staging idiom (`transition sets ctx.x = evt.y; entry block consumes ctx.x`), entry/exit scope rules, the complete history rule set (declaration, capture point, outside-only targeting, unbound default, restore re-runs entry blocks, bindings retained after completion), creation semantics and the unjournaled-creation-failure rule, atomicity guarantees, and the `run/*` catalogue (`run/unhandled` vs `run/not_enabled` distinction included). The goldens of task `1503` cite this section; where prose and golden disagree, the prose wins unless demonstrably wrong.
