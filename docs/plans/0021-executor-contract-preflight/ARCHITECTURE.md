# Architecture — Plan 0021

> Check the whole executable contract before starting an external operation, then check the concrete admission again at the point of use.

## Implementer orientation

1. Read the assigned task and its referenced production symbols before editing; remain inside its `touches` footprint.
2. Establish normative report and refusal behavior in `docs/SPEC.md` and the executor guide before implementation; executor-specific `exec/*` codes belong in `fsm_execute::error::ALL_CODES` and `docs/EMBEDDING.md`, as SPEC Appendix A explicitly requires, rather than in the engine error registry.
3. Write fixtures from those rules, not from the analyzer's output, and keep the existing core compiler, hashes, event-selection rules, and serialization unchanged.
4. All analysis runs in `fsm-execute`; it accepts compiled data and a supplied catalogue, performs no I/O itself, and adds no dependency to the workspace.
5. Follow the complete applicable stable host gate in `CONTRIBUTING.md`; targeted tests in a task are acceptance evidence, not permission to skip that gate, and record native platforms or release environments that were not executed.
6. Each task adds the necessary public-surface inventory entries and updates the relevant specification or operator contract in the same commit.

## 0000 — The inspected starting state

- `crates/fsm-cli/src/cli/execute.rs::execute` sends `--check` directly to `resolved_handlers`, after `HandlerTable::parse`; no machine participates.
- `crates/fsm-cli/src/mcp/executor.rs::handlers` projects `HandlerSpec::required_args`, static advances, and retry settings without argv or MCP argument literals; discovery is a description rather than enforcement.
- `crates/fsm-core/src/machine.rs::CompiledMachine` carries both `spec` and `compiled_exprs`; `ExprSlot` identifies entry, exit, transition, and deadline emit arguments, whose `CompiledExpr::ty` is the authoritative inferred type.
- `crates/fsm-core/src/spec/compile.rs` checks supplied declared effect fields, while `spec/validate/blocks.rs` checks the effect name; neither promises that every declaration field is emitted, and additional emitted arguments remain legal.
- `config/template.rs::substitute` renders all core values as canonical text; `substitute_arguments` substitutes only string values, leaves object keys literal, and produces a string even when a placeholder fills the entire value.
- `crates/fsm-core/src/step/validate.rs::validate_event` already owns payload field, number-token, decimal-scale, enum, and externally-sendable-event rules.
- `crates/fsm-store/src/store/instance/send.rs::send_event_stamp_on` fills only absent stamped fields with one decimal timestamp string; it does not overwrite a static field, and stamping itself is not restricted to fields declared `ts`.
- `crates/fsm-execute/src/run/pipeline.rs::Pipeline::settle` deliberately accepts an absent advance and separately checks whether a configured event is enabled at settlement; compatibility cannot replace that runtime decision.
- `crates/fsm-execute/src/service.rs::prepare` starts `Directive::Start` before `tick_reporting` opens its writer; checking a table once at process startup cannot protect future definitions, migrated instances, or restarts.

## 0091 — The compatibility contract

### Report and identity

Introduce `fsm.executor-check/1`, returned by one analyzer and serialized identically by CLI and MCP, with these explicit concepts:

| Field | Contract |
| --- | --- |
| `format` | Literal `fsm.executor-check/1`. |
| `status` | `compatible`, `invalid`, or `unknown`; `compatible` means structural compatibility within the reported scope, never a proof of progress. |
| `machine_id` | Canonical content identity of the checked definition, or null when compilation failed. |
| `contract_id` | Digest of the sanitized handler contract and manual-effect policy, or null when no authoritative table is available. |
| `definitions` | Sorted identities of the root and every resolved invoked definition. |
| `scope` | Which definitions and constructs were checked and which boundaries remain runtime-only. |
| `findings` | Deterministically sorted typed findings with code, severity, machine id, spec path, effect, outcome where relevant, message, and actionable hint. |
| `effects` | Every emitted effect and site, inferred argument types, automatic/manual disposition, required arguments, and outcome compatibility. |
| `progress` | Separate observations such as `runtime-dependent`, `manual`, and `no-outcome`; never conflated with structural invalidity. |

Use a closed schema, version its shape, and specify ordering independently of map traversal: definition id, source path, effect, outcome, then code; preserve indices where document order identifies a source site; multiple errors are reported together under explicit bounded input/work/output accounting.

Distinguish three cases: a known contradiction is `invalid`, missing authoritative evidence is `unknown`, and all required structural checks satisfied is `compatible`; invalid takes precedence over unknown, and runtime-dependent guards alone do not turn a structurally compatible report into unknown.

Register an explicit family in the executor registry: `exec/contract_invalid`, `exec/contract_unknown`, `exec/contract_limit`, `exec/contract_handler_missing`, `exec/contract_argument_missing`, `exec/contract_argument_unknown`, `exec/contract_outcome_event`, `exec/contract_outcome_payload`, and `exec/contract_definition_unknown`; preserve an underlying compiler or payload code as structured cause rather than inventing an engine code.

`contract_id` is computed only from already public contract fields, never secret argv, fixed MCP arguments, or executable paths; the admission cache has a separate private full-table identity or loaded-table generation so a change to a hidden literal cannot reuse an old approval; neither fingerprint changes machine, event, journal, or idempotency hash domains.

### Emitted effects and composition

Walk every entry and exit block at every nested state and region, every ordinary or eventless transition, every internal/generated-done-event transition, and every deadline block; guards and apparent unreachability do not remove a site from the checked set, because the report is a conservative structural check rather than a new reachability engine.

For each site, match `required_args()` against its actual argument keys and report the compiler's inferred types; do not require undeclared fields merely because a declaration lists them, reject legal additional fields, or require a string where the renderer accepts an integer, decimal, enum, timestamp, duration, or boolean; missing compiler evidence is unknown, not a guessed type.

Add optional operator-owned `manual_effects` to the closed handler table schema, with a default empty set, bounded unique names, and a refusal when a name appears in both `handlers` and `manual_effects`; an emitted name without either disposition is structurally invalid for automatic execution, while an explicitly manual effect remains pending for the existing manual acknowledgement path and is reported as manual progress; unused handlers and manual names are informational because a shared table can serve many machines.

This is deliberately stricter admission for executable workflows: an old table relying on implicit manual effects requires explicit classification on upgrade; document the changed default and release-tag consequence, preserve non-executing machine validation and manual `serve` behavior, and do not reinterpret previously written records or acknowledge refused effects.

Follow static `invoke.machine` identities through the supplied catalogue with a visited set and explicit closure limits; offline missing child definitions are `unknown`, not silently checked as if the parent were the whole workflow; store checks resolve the same content-addressed catalogue used by `compile_accepted_with_catalogue`; child invocation/return and signals are store directives rather than external handler names, and never require fabricated handlers.

A signal's target is an expression, so report that boundary as runtime-dependent without claiming to validate an unknown recipient; each actual recipient's future external effect is admitted against its own definition before spawn; no static analysis claims to prove arbitrary cross-instance liveness.

### Outcomes and stamps

For each emitted automatic effect's configured `on_ok` and `on_failed`, check the definition whose instance receives the event: the event must be externally sendable and its final payload must obey the existing validator; report missing/extra fields, numeric JSON tokens, scalar types, exact decimal scale, enum membership, unknown names, and internal/generated names through the stable contract finding with the original typed cause; a shared table's unused handlers do not impose their outcome events on an unrelated definition.

Static payload values are literal; braces do not make them templates; apply the store's fill-only-if-absent stamping rule symbolically, preserving a supplied value and treating an absent stamped value as an arbitrary in-range signed-millisecond string, without reading or advancing a clock; accept a stamp only when that generated family is compatible with the field's existing parser, reject incompatible families, and use unknown where the available proof is insufficient rather than validating one convenient timestamp; duplicate stamps obey the table parser's established rule.

An absent `on_ok` or `on_failed` is a supported outcome without an advance, not a missing-event defect; report it, test it, and do not require one outcome to resemble the other; a declared event without a matching transition is valid input but uncertain progress, and guards remain the runtime engine's decision.

### Admission at execution

One shared admission component authorizes every new start from both standalone and embedded service entry points before dispatch to the plan 0020 supervisor and eventual `Runner::spawn`; low-level `Runner` remains a mechanism and does not pretend to know a machine it was never passed; direct library callers are told where they must opt into service-level admission.

Before authorizing a new start, obtain the authoritative writer view for the short decision/dispatch phase, resolve the pending effect's emitting definition and the instance's current definition, validate the full executable definition closure, check actual argument values against required placeholders, and validate configured outcomes against the current receiving definition; standalone authorization no longer precedes writer acquisition, and writer contention authorizes no new handler while timeout/kill and child reaping remain serviceable without the writer.

Only an admitted effect/attempt generation enters the bounded supervisor start queue; cancellation, stop and stale-generation controls follow the same owner/supervisor protocol as plan 0020, and no queue retry may reinterpret a start against different machine or table inputs without returning to admission; OS spawn and waits remain outside the writer owner, preserving the nonblocking control path.

The writer is not retained across child lifetime or sleep; admission does not claim to freeze future context, cancellation, or migration during an external action, so settlement keeps the current runtime checks; plan 0022 owns cross-process child containment, and admission must not weaken that ownership boundary.

Cache only successful structural analyses, keyed by analyzer/report version, emitting and receiving definition identities, resolved closure identities, and private loaded-table identity; concrete effect arguments and lifecycle are rechecked on every new start, including retries; clear unavailable/invalid admissions on relevant input changes so a corrected configuration or definition can recover, and never persist an authorization token that another process can trust after restart.

An incompatible machine blocks all of its automatic starts, including an otherwise valid first step before a broken later compensation step; report deterministic reasons, keep effects pending, consume no attempt, write no synthetic outcome, and continue cleanup and unaffected compatible instances; retrospective outcome recovery has its own current-definition check and must never rerun a previously acknowledged external operation just to repair an incompatible event.

## 0092 — The authoring surfaces

### CLI

Preserve `fsm execute --check --handlers <file>` as a table-only inspection, but identify its scope explicitly so it cannot be mistaken for a machine compatibility pass; it never creates a store.

Add mutually exclusive `--machine-file <file>` and `--machine <name-or-id>` selectors used with `--check`: the former compiles a draft without opening the data directory, the latter opens an existing store read-only and resolves the root plus static invocation closure; a missing store is an error, never a reason to create it; stdin can supply either the draft or the table but not both.

The report uses exit 0 for compatible, 1 for invalid, 2 for usage/input/store failures, and 3 for unknown, documented as this command's contract; emit the structured report even on invalid/unknown; existing table-only exit semantics remain unchanged, and inspection while another process holds the writer remains supported.

### MCP

Add `executor_check` with exactly one of `spec` or `machine`; a draft compiles and checks before `machine_create`, with no definition, instance, request id, journal record, or handler process created as a consequence of the check; `machine` resolves an existing definition and its invocation catalogue read-only.

The active execution host supplies an immutable view of its loaded operator table to each attached session; all sessions of one shared HTTP host see the same table, while distinct hosts remain isolated; callers cannot pass executable paths, handler tables, or alternate policy; writer-only, read-only, and degraded sessions report unknown active execution compatibility with explicit reasons, even if another process might be executing, and never borrow a different host's table; valid draft compilation can still be reported independently of that uncertainty.

Expose schemas, annotations, tool descriptions, prompts, and resource guidance together; instructions ask clients to discover the executor, check and repair the draft, then create and run it; the real admission check remains mandatory when a client skips, replays, or races the draft check; no new argv or private MCP literal disclosure is added to results, errors, hints, or notifications.

Consume the `fsm.executor/2` discovery contract published by plan 0020, retaining its embedded `progress: "autonomous"`; this plan's separate `fsm.executor-check/1` format describes structural analysis, and its progress observations describe manual/runtime workflow boundaries rather than replacing the discovery scheduling enum; a check or subsequent polling observes autonomous execution and is never its required trigger.

MCP analysis itself is read-only; in an autonomous embedded server, unrelated existing workflows may continue under plan 0020, so purity tests attribute journal changes to the request rather than demanding global quiescence under a running executor.

## Verification and release boundary

Use handwritten paired definition/table fixtures with expected reports and independent side-effect marker handlers; do not derive expected results through the analyzer or let a report-only unit test substitute for a production spawn-path refusal.

Run the same fixture cases through offline CLI, store CLI, MCP draft checking, standalone admission, and embedded admission where applicable; cover both process and MCP handlers; demonstrate zero starts for a known incompatibility in a later workflow phase and successful execution after repair, while explicit manual/no-outcome cases preserve their behavior.

The format is additive, but stricter automatic execution and the new table field are compatibility decisions: update SPEC, API-POLICY, EMBEDDING, README, examples, release notes, schemas, tool goldens, and the provisional executor public inventory in their owning tasks; do not claim a journal version change when no persisted format changes, and do not raise core API or language requirements for this analyzer.

Plan 0020 consumes the same service admission in its autonomous scheduling path; plan 0022 keeps lifecycle ownership around an admitted spawn; root acceptance work exercises their combination without unresolved cross-plan task dependencies in this bundle.
