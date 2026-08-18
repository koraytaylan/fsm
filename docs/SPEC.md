# fsm — normative specification

This document is the source of truth for `fsm`. Implementers MUST treat the
keywords MUST / NEVER as binding. Golden fixtures derive from this prose, never
from observed implementation behavior — a golden that disagrees with SPEC.md is
a bug in the implementation or in the golden, never a reason to edit SPEC.md
silently.

## Format versions

| Version | Role |
|---|---|
| `fsm.machine/1` | Machine definition documents |
| `fsm.journal/1` | Journal record envelopes |
| `fsm.state/1` | Instance state snapshots |
| `expr/1` | Expression grammar |

## Machine definitions

Format `fsm.machine/1`. Top-level keys: `format`, `name`, optional `description`, optional `enums`, `context`, `events`, optional `effects`, `states`, `initial`, optional `on_unhandled` (`reject` default | `ignore`), `transitions` (flat array, document order), optional `invariants`. The keys `regions` and `deadlines` are recognized and rejected as `def/not_supported`. Numerics are strings everywhere (`req/number_token` on a raw JSON number). Machine identity hashes the entire canonical definition *including* `description`.

Context variables: `{name, ty, init}`. Types: `int`, `str`, `bool`, `timestamp`, `duration`, `{decimal: "N"}`, `{enum: "Name"}`. Events and effects declare `fields`. States are a recursive tree; a child with `history: "deep"|"shallow"` is a history pseudostate. Blocks use `do` (sets) and `emit`. Transitions use `from`, `on`, optional `if` (guard), optional `to` (absent = internal), optional `do`/`emit`.

### Structural rules (`def/*`)

| Code | Rule |
|---|---|
| `def/unknown_key` | unknown key at a given JSON-Pointer path |
| `def/shape` | wrong JSON type or missing required field |
| `def/dup_name` | state and history names share one global namespace |
| `def/one_initial` | every compound declares exactly one `initial` |
| `def/initial_not_child` | `initial` names a direct real child |
| `def/initial_is_history` | `initial` must not name a history pseudostate |
| `def/unknown_state` | `from`/`to`/`initial` resolve |
| `def/unknown_event` | `on` resolves |
| `def/unknown_effect` | emit names resolve |
| `def/unknown_enum` | enum references resolve |
| `def/terminal_not_leaf` | terminal states are leaves |
| `def/terminal_has_transitions` | no transition has a terminal `from` |
| `def/initial_terminal` | creation entry chain leaf is not terminal |
| `def/multiple_history` | at most one history per compound |
| `def/from_history` | history is never a transition source |
| `def/history_target_from_inside` | history is only targeted from outside its owner |
| `def/reserved_ident` | `$`-prefixed identifiers rejected |
| `def/not_supported` | `regions` / `deadlines` |
| `def/assign_type` | set target type equals RHS exactly, scale included |
| `def/dup_set` | duplicate set targets in one block |
| `def/shadowed` | guardless/`true` transition precedes later same `(from,on)` |
| `def/duplicate_guard` | structurally identical guards in one group |
| `def/unreachable_state` | warning: state never enterable |
| `def/ancestor_shadowed` | warning: ancestor handler globally dead |
| `def/create_always_fails` | creation fails on declared inits |
| `def/limit_states` | ≤ 256 state nodes |
| `def/limit_depth` | nesting depth ≤ 12 |
| `def/limit_history` | ≤ 32 history pseudostates |
| `def/limit_events` | ≤ 128 events |
| `def/limit_enums` | ≤ 32 enums |
| `def/limit_variants` | ≤ 64 variants each |
| `def/limit_transitions` | ≤ 2048 transitions |
| `def/limit_cell` | ≤ 32 transitions per (state, event) |
| `def/limit_ctx` | ≤ 64 context variables |
| `def/limit_fields` | ≤ 32 fields per event/effect |
| `def/limit_sets` | ≤ 32 sets per block |
| `def/limit_emits` | ≤ 8 emits per block |
| `def/limit_invariants` | ≤ 64 invariants |
| `def/limit_bytes` | definition ≤ 256 KiB |

These numeric limits match `crates/fsm-core/src/limits.rs`.

## Semantics

`step(machine, tree, state, event, payload, budget)` is a pure function.

1. **Status gate.** `Completed` → `run/instance_completed`. `Cancelled` → `run/instance_cancelled`.
2. **Validate event.** Declared name, exact field set, typed string values, no raw JSON numbers.
3. **Candidate scan.** Walk `chain(leaf)` innermost-first. At each state, take document-ordered transitions for this event. Empty candidates across the whole chain is `run/unhandled` (`ignore` yields `Ignored`).
4. **Guard evaluation.** Guards see the pre-transition `(ctx, evt)` only. A guard evaluation error is `run/guard_error` (never treat-as-false). The first true guard wins; later candidates are `not_considered`. All false → `run/not_enabled`.
5. **Target / dom.** Absent `to` is internal (no exit/entry). A history target resolves through `history_descent` (owner used for dom). External self-transition uses `dom = parent(from)` and exits/re-enters. Otherwise `dom = properLCA(source, target)`.
6. **Block pipeline.** Exit blocks inner→outer, then the transition block (the only block that sees `evt`), then entry blocks outer→inner. Each block is snapshot-internal: all RHS evaluate against the ctx left by the previous block, then apply atomically. Staging idiom: `transition` sets `ctx.x = evt.y`; an entry block consumes `ctx.x`. Emits collect under one global `k`. Any evaluation error is `run/action_error` naming `exit(state)` / `transition` / `entry(state)`; computed-but-discarded values of completed blocks stay in the trace.
7. **History capture.** For each exited compound that owns a history pseudostate, bind from the **pre-transition** configuration (deep = pre leaf; shallow = owner's direct child on the pre chain). Unbound history later descends the owner's initial chain. Restore re-runs entry blocks. Bindings are retained after completion/cancel. History may only be targeted from outside its owner.
8. **Invariants.** All evaluated on the final ctx. Enforce failure or eval error → `run/invariant`. Monitor failures collect into `monitor_flags` and never block.
9. **Status.** New leaf terminal → `Completed`. Rejection discards ctx, configuration, history, and effects.

**Creation.** `create` validates overrides like event fields, starts from declared inits, enters the root initial descent outer→inner, evaluates invariants, starts with empty history. Failure is `run/create_failed`. The shell NEVER journals a failed create and consumes no id or seq.

**Atomicity.** `step` is pure. The caller commits only `Applied`.

### `run/*` catalogue

| Code | Trigger | Hint policy |
|---|---|---|
| `run/unhandled` | no candidate on the chain | add a transition or send a handled event — this is a definition gap, not a payload miss |
| `run/not_enabled` | candidates exist but every guard is false | fix the payload or add a child override |
| `run/guard_error` | guard evaluation failed | source state, index, span |
| `run/action_error` | a block evaluation failed | name the block |
| `run/invariant` | enforce invariant failed | list every failing invariant |
| `run/instance_completed` | event against a completed instance | — |
| `run/instance_cancelled` | event against a cancelled instance | — |
| `run/create_failed` | creation failed; **unjournaled** | wrap the inner error |
| `run/overflow` | checked arithmetic in an action/guard | operand strings |
| `req/event_unknown` | undeclared event | — |
| `req/field_missing` | declared field absent | — |
| `req/field_unknown` | extra field | — |
| `req/field_type` | value does not match declared type | — |
| `req/field_scale` | decimal has too many fraction digits | — |
| `req/number_token` | raw JSON number | quote it |


## Journal

### Idempotency

`request_id` is an idempotency key over the *content* of a request, not a label
on a slot. Every record that claims a key also stores `request_fp`, a
`fsm:request-fp:1` digest of the operation and its arguments — for a send, the
instance, event, and payload as received. Resending a key:

- with the same fingerprint replays the original outcome, marked `duplicate`;
- with a different fingerprint is `req/request_id_conflict`, never a replay.

Without that check a driver deriving ids from (task, event) rather than
per-attempt would receive the *first* request's success for a second, different
request, and diverge from the instance silently. `req/request_id_conflict` is
not retryable: the remedy is a new key, and the old key still replays its own
outcome. `expect_seq` is excluded from the fingerprint — it is a concurrency
precondition, so refreshing it across a retry must not look like new content.

Keys claimed by records written before store format 7 carry no fingerprint and
remain replay-only; the format is migrated on open without rewriting records.

### Payload size

Event payloads, effect-ack `result`s, and annotation notes are journalled
verbatim and never rewritten, so their cost is permanent and is paid again on
every fold, snapshot, and verify. Anything larger than `MAX_PAYLOAD_BYTES`
(64 KiB of canonical bytes) is refused with `req/payload_too_large` — journal a
digest or an identifier and keep the blob in its own store. The check runs
before the request is applied and does not depend on instance state, so like
`req/seq_mismatch` it is unjournaled and does not consume `request_id`: correct
the payload and resend under the same key.

`MAX_PAYLOAD_BYTES` is deliberately absent from the genesis `limits` block,
which is hash-verified on fold; adding a key there would make every store
written by an earlier build unreadable rather than migratable.

A request-outcome record exists **iff** the outcome depended on instance state and is not retry-stable. The unique admitted state-dependent-but-retry-stable case is `expect_seq` mismatch (`req/seq_mismatch`): it is unjournaled and does not consume `request_id`. Dedup lookup MUST precede the `expect_seq` check — otherwise a lost-response retry with a stale seq would be rejected, the client would "fix" the seq under a new request_id, and the event would apply twice. `run/create_failed` is the one unjournaled `run/*` outcome (no prior instance exists). `state_checkpoint` is a maintenance record rather than a request outcome; it changes no logical state and consumes no `request_id`.

Envelope (one canonical LF-terminated line, domain `fsm:record:1` over the envelope minus `hash`):

`{"body":…,"hash":"<64 hex>","kind":"…","prev":"<64 hex>","seq":…,"ts":…}`

Genesis is `seq` 0, `prev` sixty-four `0`s, body `{format: "fsm.journal/1", created_ts, limits}`.

### Record kinds

| Kind | Body fields |
|---|---|
| `genesis` | `format`, `created_ts`, `limits` |
| `machine_defined` | `machine_id`, `def` |
| `instance_created` | `instance_id`, `machine_id`, `request_id`, `state_hash`, `leaf`, `overrides` |
| `event_applied` | `instance_id`, `event`, `payload`, `request_id`, `state_hash`, `exited`, `entered`, `source_state` |
| `event_rejected` | `instance_id`, `event`, `payload`, `request_id`, `state_hash`, `code`, `message`, `hint`, `details`, optional `span` |
| `event_ignored` | `instance_id`, `event`, `payload`, `request_id`, `state_hash` |
| `effect_acked` | `instance_id`, `effect_id`, `request_id`, `outcome` (`ok` or `failed`), `state_hash`, optional `result` |
| `request_rejected` | `request_id`, `instance_id`, `code`, `message`, `hint`, `details`, `operation`, `state_hash`; `effect_id` required when `operation` is `ack` |
| `instance_cancelled` | `instance_id`, `request_id`, `reason`, `state_hash` |
| `annotated` | `instance_id`, `request_id`, `note` |
| `state_checkpoint` | `state_root` |

Verification: the stored line MUST equal its canonical re-serialization; seq is consecutive; `prev` matches the prior hash; `hash` is recomputed; fold re-applies through `step`/`create` and checks journaled `state_hash` / `exited` / `entered` / `source_state`. Duplicate `request_id` values are a fold error. `effect_acked` and `instance_cancelled` commit the post-operation instance `state_hash`. A record carrying `state_root` commits the complete logical store state after that record at its `seq`; the root excludes the record hash to avoid a cycle, and replay MUST recompute it. On-disk store `VERSION` is `6`. Opening a `VERSION` `1` through `5` directory, or a journal with no `VERSION` marker, MUST attempt a best-effort migration: ignore snapshot caches entirely, fold the complete journal under current `fsm.journal/1` semantics, and on success stamp `VERSION` `6`. Interior journal records MUST NOT be rewritten. If classify is not `Ok` (including a migratable marker whose journal is missing) or fold fails, refuse with that health and leave `VERSION` unchanged — a migratable directory is never re-created over. A successful `repair --truncate-torn-tail` on a migratable store folds the complete retained journal and likewise stamps `VERSION` `6`. Any other `VERSION` value is `store/version_mismatch`, refused and never silently reinterpreted.

### Recovery

| Health | Posture |
|---|---|
| `Ok` | open |
| `TornTail` | refuse; remedy `fsm repair --truncate-torn-tail` (quarantine tail bytes, then truncate) |
| `ChainBroken` | refuse; interior; no repair; blast radius `records ≥ N unverifiable` |
| `StateHashMismatch` | refuse; no repair |
| `NonCanonical` | refuse; no repair |
| `LockIo` | refuse |

#### Durability across platforms

Every append fsyncs the segment **file** before returning, on every platform.
What differs is the enclosing directory entry: after creating or renaming a file
(segment rotation, snapshot installation, the request-id allocation file) the
store also fsyncs the containing directory, and that step is Unix-only. Windows
exposes no portable equivalent — opening a directory as a file fails outright,
and flushing a directory handle requires `FILE_FLAG_BACKUP_SEMANTICS`, which the
standard library does not offer.

The consequence on Windows is bounded: a crash in the window between a rename
and the directory metadata reaching disk can leave the entry missing even though
the file's bytes were flushed. It cannot corrupt a record, because record
durability does not depend on it. Every such case lands in the table above and
is classified on the next open rather than trusted, so the outcome is a recovery
step, not silent loss.

Interior history is never rewritten. Snapshots (`fsm.snapshot/3`) are disposable caches, never authoritative, never part of the chain. Each snapshot carries a self-checked `state_root`: `sha256:` plus the hex encoding of domain `fsm:state-root:2` over canonical `{seq,machines,instances,dedup}` using the same values and per-instance hashes as the snapshot; `last_hash` is excluded to avoid a cycle. The fast path is permitted only when the journal record at the snapshot sequence has the same hash as the snapshot's `last_hash` and carries that same `state_root` in its hash-chained body. Explicit snapshots append a `state_checkpoint`; the automatic 10,000-record snapshot commits the root in that existing boundary record. A clean-shutdown cache without a journal-bound root is accepted only after folding the complete journal prefix and proving exact state equality, so it is not a fast path. Mutable sidecar files are never trust anchors. `fsm.snapshot/1` and `fsm.snapshot/2` caches are skipped, never reinterpreted.

## Expressions

Grammar version `expr/1`. Keywords are reserved: `if then else and or not true
false ctx evt`. Mode and unit words (`half_even`, `ms`, …) are ordinary
identifiers; position, not reservation, disambiguates them. Identifiers are
`[a-z_][a-z0-9_]{0,63}`. Type identifiers are `[A-Z][A-Za-z0-9_]{0,63}`. There
are no comments. Source over 4,096 bytes is `expr/too_long`. `/` and `%` are
not tokens; `a / b` fails at the lexer with a hint naming `div(a, b, scale, mode)`.

```ebnf
expr        = if_expr ;
if_expr     = "if" , or_expr , "then" , if_expr , "else" , if_expr | or_expr ;
or_expr     = and_expr , { "or" , and_expr } ;
and_expr    = not_expr , { "and" , not_expr } ;
not_expr    = "not" , not_expr | cmp_expr ;
cmp_expr    = add_expr , [ cmp_op , add_expr ] ;          (* non-associative *)
cmp_op      = "==" | "!=" | "<=" | "<" | ">=" | ">" ;
add_expr    = mul_expr , { ( "+" | "-" ) , mul_expr } ;
mul_expr    = unary_expr , { "*" , unary_expr } ;
unary_expr  = "-" , unary_expr | primary ;
primary     = int_lit | dec_lit | str_lit | "true" | "false"
            | ( "ctx" | "evt" ) , "." , ident
            | type_ident , "." , ident
            | ident , "(" , [ arg , { "," , arg } ] , ")"
            | "(" , expr , ")" ;
arg         = expr | ident ;                               (* bare ident = Word *)
```

A second comparison operator in one `cmp_expr` is `expr/chained_cmp` with hint
exactly `use `and` to combine comparisons`. Integer literals that do not fit
`i64` are `expr/int_range`. Decimals with more than 38 digits or more than 12
fraction digits are `expr/dec_range`. More than 512 AST nodes is `expr/too_long`.
Nesting beyond depth 32 is `expr/too_deep`. Other mismatches are `expr/parse`
with the expected-token set in the hint.

### Types

`Bool`, `Int`, `Dec(scale ≤ 12)`, `Str`, machine-declared enums, `Ts`, `Dur`.
Rendered as `bool`, `int`, `decimal(N)`, `str`, `enum Name`, `timestamp`,
`duration`.

| Construct | Rule |
|---|---|
| `IntLit` | `Int` |
| `DecLit` | `Dec(s)` where `s` is the fraction-digit count |
| `+ -` | `Int×Int→Int` · `Dec(s1)×Dec(s2)→Dec(max(s1,s2))` · `Ts+Dur→Ts`, `Dur+Ts→Ts`, `Ts−Ts→Dur`, `Ts−Dur→Ts`, `Dur±Dur→Dur` · everything else `expr/type_mismatch`; `Dec` with `Int` → `expr/mixed_class` (hint: write `0.00`-style literals or `dec(x, s)`) |
| `*` | `Int×Int→Int` · `Dec(s1)×Dec(s2)→Dec(s1+s2)`, statically `expr/scale_cap` when `s1+s2 > 12` · `Dec(s)×Int→Dec(s)` and `Int×Dec(s)→Dec(s)` (exact) · `Dur×Int→Dur`, `Int×Dur→Dur` |
| unary `-` | `Int`, `Dec`, `Dur` only |
| `cmp` | both sides same class; `Dec` compares by value across scales; full order on `Int`, `Dec`, `Ts`, `Dur`; `Str`, `Enum`, `Bool` allow `==`/`!=` only, an ordering operator → `expr/cmp_unordered` |
| `and or not` | `Bool` operands |
| `if c then a else b` | `c: Bool`; branches unify in the same class; two `Dec` branches widen exactly to `Dec(max scale)` |
| `CtxRef`/`EvtRef` | declared name, else `expr/unknown_var`/`expr/unknown_field` with a Levenshtein suggestion (distance ≤ 2) plus the legal list; `EvtRef` in an invariant is `expr/evt_in_invariant`; in an entry/exit block is `expr/evt_in_block` |
| `EnumLit T.v` | `T` declared (`expr/unknown_enum`), `v` a variant (`expr/unknown_variant`); result `Enum(T)` |
| `Call` | signatures below; unknown name → `expr/unknown_builtin` listing the seven legal names |

### Builtins

Scale arguments MUST be integer literals `0..=12`. Mode and unit arguments MUST
be literal words. Otherwise the result *type* would depend on a runtime value
(`expr/scale_not_literal` / `expr/mode_invalid`). Wrong arity is `expr/arity`.

| Signature | Typing | Evaluation |
|---|---|---|
| `min(a, b)`, `max(a, b)` | both `Int` → `Int`; both `Dec` → `Dec(max scale)`; both `Ts` or both `Dur` | value comparison (Dec via `Dec::cmp`) |
| `abs(x)` | type-preserving on `Int`/`Dec`/`Dur` | checked (`abs(i64::MIN)` → `run/overflow`) |
| `dec(x, S)` | `Int → Dec(S)`; `Dec(s0) → Dec(S)` requires `s0 ≤ S` else `expr/scale_narrow` (hint: use `round`) | exact widen, total |
| `round(x, S, M)` | `Dec(s0) → Dec(S)`, `M` mandatory; warns `expr/round_widens` when `S ≥ s0` | `Dec::round` |
| `div(a, b, S, M)` | `a`, `b` each `Int` or `Dec` → `Dec(S)` | `Dec::div`; `b = 0` → `run/div_zero` |
| `dur(n, U)` | `n: Int`, `U ∈ ms s min h d` → `Dur` | checked multiply to milliseconds |

`M ∈ {down, up, floor, ceiling, half_up, half_down, half_even}`.

### Evaluation

Evaluation is total, deterministic, and strict left-to-right. `and`/`or`
short-circuit; `if` evaluates only the taken branch. One `Budget` is shared
across every evaluation of a single event; each AST-node visit decrements it;
exhaustion is `internal/budget` (an engine-invariant breach, never a user
error). All `Int`/`Ts`/`Dur` arithmetic uses checked operations, including
`-(i64::MIN)` → `run/overflow`. Decimal arithmetic delegates to the decimal
module (`Overflow` → `run/overflow`).

### Partial evaluation

`partial_eval_bool` answers “could this guard pass?” when the next event payload
is unknown. Callers supply a `Scope` with declared enums and event-field types.
Lazy `if` reduces a concrete-true or concrete-false condition to the selected
branch before payload dependence is decided, so an unreachable `evt.*` branch
does not make a context-concrete guard `Unknown`. Remaining `EvtRef` is Kleene
`Unknown`. `and`/`or`/`not` follow the Kleene tables (`False and _ = False`,
`True or _ = True`, `not Unknown = Unknown`). Comparisons and arithmetic
containing an `Unknown` operand are `Unknown`. Fully-`ctx` subtrees evaluate
concretely. A concrete sub-evaluation error — including budget exhaustion —
yields `Unknown`. This is deliberately conservative: an erroring guard is
neither definitely enabled nor definitely disabled; the authoritative loud
failure (`run/guard_error`) happens at send time.

### Expression error catalogue

| Code | Trigger | Hint policy |
|---|---|---|
| `expr/too_long` | source > 4096 bytes, or > 512 AST nodes | split or shorten |
| `expr/too_deep` | nesting beyond depth 32 | flatten |
| `expr/lex` | unexpected byte, bad number/string form, `/` or `%` | `/` names `div(a, b, scale, mode)` |
| `expr/parse` | grammar mismatch or trailing tokens | expected-token set |
| `expr/chained_cmp` | second comparison in one `cmp_expr` | exactly `use `and` to combine comparisons` |
| `expr/int_range` | integer literal does not fit `i64` | use a smaller integer |
| `expr/dec_range` | more than 38 digits or 12 fraction digits | shrink the literal |
| `expr/type_mismatch` | operand class does not match the construct | name the expected type |
| `expr/mixed_class` | `Dec` mixed with `Int` on `+`/`-`/cmp | `0.00`-style literal or `dec(x, s)` |
| `expr/scale_cap` | `Dec×Dec` scale sum > 12 | round an operand first |
| `expr/unknown_var` | unknown `ctx` name | Levenshtein ≤ 2 plus legal list |
| `expr/unknown_field` | unknown `evt` name | Levenshtein ≤ 2 plus legal list |
| `expr/unknown_enum` | unknown enum type | suggestion plus legal list |
| `expr/unknown_variant` | unknown variant | suggestion plus legal list |
| `expr/unknown_builtin` | unknown call name | the seven legal names |
| `expr/cmp_unordered` | `<`/`>` on `Str`/`Enum`/`Bool` | use `==` or `!=` |
| `expr/evt_in_invariant` | `evt` in an invariant | invariants read `ctx` only |
| `expr/evt_in_block` | `evt` in an entry/exit block | blocks read/write `ctx` only |
| `expr/scale_narrow` | `dec` would drop scale | use `round` |
| `expr/scale_not_literal` | scale is not an integer literal `0..=12` | types cannot depend on runtime values |
| `expr/mode_invalid` | bad or non-literal mode/unit | list the legal words |
| `expr/arity` | wrong argument count | expected N / found M |
| `expr/round_widens` | warning: `round` target scale ≥ operand | use `dec` |
| `run/overflow` | checked arithmetic overflow | operand canonical strings in `details` |
| `run/div_zero` | `div` by zero | name the divisor |
| `internal/budget` | shared step budget exhausted | engine invariant |

## Appendix A — Error codes

Every stable code in `fsm_core::error::ALL_CODES`:

- `def/ancestor_shadowed` — ancestor handler globally dead
- `def/assign_type` — set target type ≠ RHS
- `def/create_always_fails` — creation fails on declared inits
- `def/dup_name` — duplicate state or history name
- `def/dup_set` — duplicate set targets in one block
- `def/duplicate_guard` — identical guards in one (from, on) group
- `def/from_history` — history used as a transition source
- `def/history_target_from_inside` — history targeted from inside its owner
- `def/initial_is_history` — initial names a history node
- `def/initial_not_child` — initial is not a direct child
- `def/initial_terminal` — creation chain lands on a terminal
- `def/limit_bytes` — definition exceeds 256 KiB
- `def/limit_cell` — more than 32 transitions per (state, event)
- `def/limit_ctx` — more than 64 context variables
- `def/limit_depth` — nesting depth exceeds 12
- `def/limit_emits` — more than 8 emits per block
- `def/limit_enums` — more than 32 enums
- `def/limit_events` — more than 128 events
- `def/limit_fields` — more than 32 fields
- `def/limit_history` — more than 32 history nodes
- `def/limit_invariants` — more than 64 invariants
- `def/limit_sets` — more than 32 sets per block
- `def/limit_states` — more than 256 states
- `def/limit_transitions` — more than 2048 transitions
- `def/limit_variants` — more than 64 variants
- `def/multiple_history` — more than one history per compound
- `def/not_supported` — regions or deadlines
- `def/one_initial` — compound missing exactly one initial
- `def/reserved_ident` — `$`-prefixed identifier
- `def/shadowed` — guardless transition hides later siblings
- `def/shape` — wrong JSON type or missing field
- `def/terminal_has_transitions` — transition from a terminal
- `def/terminal_not_leaf` — terminal is not a leaf
- `def/unknown_effect` — emit names an unknown effect
- `def/unknown_enum` — unknown enum type
- `def/unknown_event` — unknown event name
- `def/unknown_key` — unknown key at a JSON-Pointer path
- `def/unknown_state` — unknown state name
- `def/unreachable_state` — state is not enterable
- `expr/arity` — wrong builtin arity
- `expr/chained_cmp` — two comparisons in one cmp_expr
- `expr/cmp_unordered` — ordering compare on unordered type
- `expr/dec_range` — decimal literal out of range
- `expr/evt_in_block` — evt in entry/exit
- `expr/evt_in_invariant` — evt in an invariant
- `expr/int_range` — integer literal out of i64
- `expr/lex` — lexer error
- `expr/mixed_class` — Dec mixed with Int
- `expr/mode_invalid` — bad rounding mode or unit
- `expr/parse` — grammar mismatch
- `expr/round_widens` — round target scale ≥ operand
- `expr/scale_cap` — Dec×Dec scale sum > 12
- `expr/scale_narrow` — dec would drop scale
- `expr/scale_not_literal` — scale is not a literal
- `expr/too_deep` — nesting beyond 32
- `expr/too_long` — source or AST too large
- `expr/type_mismatch` — operand class mismatch
- `expr/unknown_builtin` — unknown call
- `expr/unknown_enum` — unknown enum in expression
- `expr/unknown_field` — unknown evt field
- `expr/unknown_var` — unknown ctx name
- `expr/unknown_variant` — unknown enum variant
- `internal/budget` — evaluation budget exhausted
- `internal/unimplemented` — stub
- `io/read` — read failed
- `io/write` — write failed
- `req/args_invalid` — tool/CLI arguments invalid
- `req/event_unknown` — undeclared event
- `req/field_missing` — declared field absent
- `req/field_scale` — too many fraction digits
- `req/field_type` — value does not match type
- `req/field_unknown` — extra field
- `req/instance_not_found` — unknown instance
- `req/machine_ambiguous` — bare name matches several versions
- `req/machine_exists` — define refused because the spec exists
- `req/machine_not_found` — unknown machine
- `req/number_token` — raw JSON number where a string is required
- `req/payload_too_large` — journalled payload exceeds 64 KiB
- `req/request_id_conflict` — request_id reused for different content
- `req/seq_mismatch` — stale expect_seq
- `run/action_error` — block evaluation failed
- `run/create_failed` — creation failed
- `run/div_zero` — division by zero
- `run/guard_error` — guard evaluation failed
- `run/instance_cancelled` — send to a cancelled instance
- `run/instance_completed` — send to a completed instance
- `run/invariant` — enforce invariant failed
- `run/not_enabled` — all guards false
- `run/overflow` — checked arithmetic overflow
- `run/unhandled` — no candidate on the chain
- `store/chain_broken` — interior hash/seq break
- `store/lock` — lock I/O
- `store/non_canonical` — non-canonical journal line
- `store/state_hash_mismatch` — fold disagreed
- `store/torn_tail` — truncated final record
- `store/version_mismatch` — data directory VERSION is not 6 and cannot be migrated

## Appendix B — Limits

| Limit | Value |
|---|---|
| definition size | 256 KiB (`MAX_DEF_BYTES`) |
| journalled payload | 64 KiB (`MAX_PAYLOAD_BYTES`) |
| nesting depth | 12 (`MAX_NESTING`) |
| eval budget | 4096 ticks per event |
| states | 256 |
| events | 128 |
| transitions | 2048 |
| context variables | 64 |
| history nodes | 32 |
| invariants | 64 |
| enums | 32 (`MAX_ENUMS`) |
| variants per enum | 64 (`MAX_VARIANTS`) |
| transitions per (state, event) | 32 (`MAX_TRANSITIONS_PER_CELL`) |
| fields per event or effect | 32 (`MAX_FIELDS`) |
| sets per block | 32 (`MAX_SETS_PER_BLOCK`) |
| emits per block | 8 (`MAX_EMITS_PER_BLOCK`) |

These match `crates/fsm-core/src/limits.rs`.

## Appendix C — Format versions

| Tag | Role |
|---|---|
| `fsm.machine/1` | Machine definition documents |
| `fsm.journal/1` | Journal record envelopes |
| `fsm.snapshot/3` | Disposable snapshot caches optionally accelerated by a hash-chained `state_root` |
| `fsm.snapshot/1`, `fsm.snapshot/2` | Skipped, never reinterpreted; the journal is folded instead |
| `fsm.state/1` | Instance state identity hash payload |
| `expr/1` | Expression grammar |

On-disk store `VERSION` is `7`. A `VERSION` `1` through `6` directory, or a journal with no `VERSION` marker, is best-effort migrated on open (or by a successful repair) by folding the complete journal with snapshot caches ignored, then stamping `VERSION` `7`; records, machine ids, and snapshot caches are never rewritten or reinterpreted. Any other `VERSION` is `store/version_mismatch`, refused and never reinterpreted.

Because records are never rewritten, a migrated store keeps whatever its records already carried: a `request_id` claimed before `VERSION` `7` has no `request_fp`, so it can be replayed but not conflict-checked. Records written after the migration are fully checked.

Hash domains are versioned independently of these tags: `fsm:machine:1`, `fsm:record:1`, `fsm:state:1`, `fsm:state-root:2`, `fsm:snapshot:3`, `fsm:request-fp:1`.
