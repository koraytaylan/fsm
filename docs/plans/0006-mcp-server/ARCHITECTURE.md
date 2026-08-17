# Architecture — Plan 0006

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/corpus your task names before writing implementation code. They are the executable definition of done — when they pass, you are done; do not "improve" beyond them.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer. The command named in the Done-when is what runs them.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design, not a reason to edit it.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`. A red gate is never someone else's flake — this workspace has zero dependencies and deterministic tests.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here; where a trick is genuinely needed, this document names it — and if it doesn't, don't use one.
7. When a golden or byte-comparison test fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document; then say so in your commit message.
8. stdout carries protocol JSON only, through the single chokepoint — a stray print breaks every host. Transcript fixtures are hand-derived from the schema tables (see the authoring note in workstream 0031).

## 0028 — Protocol

`crates/fsm-cli/src/mcp/serve.rs` grows from the plan-0001 skeleton into the full lifecycle (task `2801`). It also creates stub modules `tools.rs`, `descriptions.rs`, `resources.rs`, `prompts.rs` (registered in `mcp/mod.rs`) and pre-routes every method, so the three workstream-0029/0030 fill-in tasks touch only their own files:

- **Negotiation table**, one function `fn negotiate(client: Option<&str>) -> &'static str`:

  | Client offers | Server answers |
  |---|---|
  | `2025-06-18` | `2025-06-18` |
  | `2025-03-26` | `2025-03-26` |
  | `2024-11-05` | `2024-11-05` |
  | `2025-11-25`, anything else, or missing | `2025-06-18` |

  One behavior profile regardless of the negotiated string; the value is recorded in the session state so a future gate is a one-line change.
- **Initialize gate:** before `initialize` completes, only `initialize` and `ping` are served; anything else → JSON-RPC error `-32002` ("Server not initialized"). `notifications/initialized` sets a flag; requests arriving without it log a warning to stderr and proceed. Duplicate request ids are answered normally.
- **Batch arrays** → `-32600` under every negotiated revision (batching was removed in 2025-06-18; for older clients this is a documented limitation).
- **Notification policy:** `notifications/cancelled` is parsed and stderr-logged — in a strictly sequential server the referenced request is already complete or is the one currently executing, so there is nothing to cancel; `notifications/roots/list_changed` and unknown notifications are ignored at debug level. The server emits zero server-initiated messages.
- **Capabilities** are advertised complete from this task (the stubs return empty lists until filled): `{"tools": {"listChanged": false}, "resources": {"subscribe": false, "listChanged": false}, "prompts": {"listChanged": false}}`, `serverInfo: {"name": "fsm", "title": "fsm — deterministic state machines for LLM workflows", "version": env!("CARGO_PKG_VERSION")}`, `instructions: prompts::INSTRUCTIONS` (an empty stub const until task `3002`).
- **Panic hook:** `std::panic::set_hook` writes the panic message and backtrace to stderr, then `std::process::abort()` — the journal is already durable at every instant, so the client sees a clean EOF and an acknowledged write is never lost.
- **EOF shutdown:** flush stdout, drop the store (releasing the data-dir lock), exit 0. No signal handling exists or is needed.
- **Error-channel rule** as a helper pair: `fn rpc_error(id, code, message) -> Value` for envelope faults only (parse, invalid request, unknown method, args-not-an-object, not-initialized, internal bugs) and `fn tool_error(err: &FsmError) -> Value` building `{"isError": true, "content": [{"type": "text", "text": <rendered>}], "structuredContent": {"error": {code, message, path, span?, hint, retryable, duplicate, details, docs}}}` for every domain error — hosts reliably surface tool results to the model, and the thesis is that errors teach the model the fix. `duplicate` is false on the first attempt and true on an exact-once retry.
- `serve` is parameterized for tests: `pub fn serve(store: &mut Store, clock: &mut dyn Clock, input: impl BufRead, output: impl Write) -> io::Result<()>` — the plan-0004 `Clock` trait makes transcripts deterministic.

Fixtures first: `crates/fsm-cli/tests/fixtures/transcripts/lifecycle.in.jsonl` / `lifecycle.out.jsonl` covering per-revision echo (`2025-03-26`, `2024-11-05`), unknown-version fallback, `2025-11-25` fallback, cancelled-notification silence, duplicate-id answering, and the initialized-flag warning path; `crates/fsm-cli/tests/mcp_lifecycle.rs` byte-compares the full stream.

## 0029 — Tools

`crates/fsm-cli/src/mcp/tools.rs` (tasks `2901`, `2903`):

- `pub struct ToolSpec { pub name: &'static str, pub description: &'static str, pub input_schema: fn() -> Value, pub output_schema: fn() -> Value, pub run: fn(&mut Store, &mut dyn Clock, &Value) -> Result<Value, FsmError> }` and `pub fn registry() -> Vec<ToolSpec>` — 13 entries in this fixed order: `machine_create`, `machine_list`, `machine_get`, `machine_analyze`, `machine_diagram`, `instance_create`, `instance_send`, `effect_ack`, `instance_cancel`, `instance_get`, `instance_list`, `instance_history`, `simulate`. Descriptions reference `descriptions::*` consts so prose and code live in separate files.
- **Schemas** are canonical `Value` constants: inputs declare `type: "object"`, `properties`, `required`, `additionalProperties: false`; outputs promise only guaranteed fields with `additionalProperties: true`. Field lists (inputs → outputs):

  | Tool | Input fields | Output fields (guaranteed) |
  |---|---|---|
  | machine_create | `spec` (object, req), `dry_run` (bool, default false), `if_exists` (`"return_existing"`\|`"error"`) | `machine_id`, `name`, `created`, `dry_run`, `warnings[]`, `summary{initial, states, events, transitions, terminal_states[]}` |
  | machine_list | `name_contains?`, `limit` (≤200, default 50), `cursor?` | `machines[]{machine_id, name, defined_seq, states, events, instances{running, completed, cancelled}}`, `next_cursor?` |
  | machine_get | `machine` (id \| unique prefix \| unambiguous name) | `machine_id`, `name`, `spec`, `summary` |
  | machine_analyze | `machine` | `machine_id`, `findings[]{severity, code, message, path, hint}`, `reachability{unenterable[]}`, `completeness{by_leaf}`, `shadowing[]` |
  | machine_diagram | `machine`, `format` (`"mermaid"`\|`"dot"`), `instance?` | `format`, `diagram` |
  | instance_create | `machine`, `context?`, `request_id` (req), `tags?` | `instance_id`, `machine{machine_id, name}`, `status`, `state`, `configuration[]`, `seq`, `context`, `effects_pending[]`, `enabled_events[]`, `state_hash` |
  | instance_send | `instance_id`, `event{name, payload?}`, `request_id` (req), `stamp?[]`, `expect_seq?` | `applied`, `duplicate`, `seq`, `transition{source_state, transition_idx, internal, from_leaf, to_leaf, exited[], entered[]}`, `state`, `configuration[]`, `status`, `context`, `effects_pending[]`, `monitor_flags[]`, `trace`, `enabled_events[]`, `state_hash` |
  | effect_ack | `instance_id`, `effect_id`, `outcome` (`"ok"`\|`"failed"`), `result?`, `request_id` (req) | `instance_id`, `effect_id`, `acked`, `duplicate`, `seq`, `effects_pending[]` |
  | instance_cancel | `instance_id`, `reason`, `request_id` (req) | `instance_id`, `status`, `seq`, `state`, `context`, `state_hash` |
  | instance_get | `instance_id` | `instance_id`, `machine{…}`, `status`, `state`, `configuration[]`, `seq`, `context`, `history`, `effects_pending[]`, `enabled_events[]`, `state_hash` |
  | instance_list | `machine?`, `state?`, `status?` (`"running"`\|`"completed"`\|`"cancelled"`\|`"all"`), `tag?`, `limit`, `cursor?` | `instances[]{instance_id, machine_name, state, status, seq, tags}`, `next_cursor?` |
  | instance_history | `instance_id`, `from_seq?`, `limit` (≤500, default 50), `include_trace` (default false), `include_rejected` (default true) | `instance_id`, `entries[]{seq, ts, kind, event?, request_id?, from_leaf?, to_leaf?, context_after?, trace?, hash}`, `next_from_seq?`, `chain_verified` |
  | simulate | `machine` XOR `spec`, `context?`, `events[]{name, payload?}`, `on_reject` (`"stop"`\|`"continue"`) | `initial{state, context}`, `steps[]{index, event, applied, from_leaf, to_leaf, context, effects[], trace, error?}`, `final{state, context, terminal}`, `stopped_at?` |

  Every mutating operation requires `request_id` (the idempotency key backing exact-once application); `instance_send` additionally accepts `stamp` (declared timestamp fields the shell resolves before journaling) and `expect_seq` (optimistic concurrency; mismatch → `req/seq_mismatch`, retryable, request_id not consumed).
- **Argument validation** (task `2901`): `fn validate_args(schema: &Value, args: &Value) -> Result<(), FsmError>` implementing the schema subset we emit (`type`, `required`, `properties`, `enum`, `additionalProperties`), producing `req/args_invalid` whose `details` lists each offending field with expected-vs-got and whose `hint` names the first fix.
- **Dispatch** (task `2903`): each `run` resolves machine references (full `name@sha256:…` id, unique hash prefix ≥ 12, or bare name when exactly one version exists — else `req/machine_ambiguous` listing the versions), calls the plan-0004 `Store` / plan-0003 core, and assembles the response. Every mutating response carries the full post-state: leaf-path `state` plus `configuration[]`, full `context`, `effects_pending`, the decision `trace`, `enabled_events` (three-valued report over the ancestor chain), `seq`, and `state_hash`. `structuredContent` is canonical; the human `text` block is produced from it by the plan-0005 `render.rs` renderer — one source, two projections, byte-tested in workstream 0031. `retryable` comes only from `fsm_core::error::retryable(code)`.

Tool descriptions (task `2902`) live in `crates/fsm-cli/src/mcp/descriptions.rs` as `pub const` strings following the shipped guidelines (when-to-use first sentence; name the next tool; state what schemas cannot: decimals as strings, `request_id` retry semantics, `$`-reserved names; pre-teach the two or three commonest errors; ≤180 words for the workhorses, ≤40 for list/get tools). The two workhorse texts ship verbatim as:

> **machine_create** — Create a state machine definition from a complete JSON spec, or validate without saving (`dry_run: true`). A spec declares a state tree (one initial child per compound state; terminal states are leaves), typed context variables, typed event payloads, and guarded transitions; read the resource `fsm://docs/spec` for the spec format and expression grammar before authoring your first machine. Definitions are immutable and content-addressed: `machine_id` derives from the canonical spec, so creating an identical spec twice returns the same id with `created: false` — never an error. Running instances keep the definition they started with. All decimal values are exact JSON strings (`"19.99"`), never numbers. On failure you get `def/*` or `expr/*` findings, each with a `path` into your spec, a character `span` for expression errors, and a `hint` stating the fix — correct the spec and call again. On success, review `warnings` before creating instances. Typical flow: `machine_create(dry_run: true)` → fix → `machine_create` → `instance_create`.

> **instance_send** — Deliver one event to a running instance — the only way to advance it; every accepted or rejected event is appended to a tamper-evident journal. `request_id` is required and is an idempotency key you choose: resending the same `request_id` never applies twice, it returns the original outcome with `duplicate: true` — after a timeout, retry with the SAME `request_id`. The response carries the whole situation: the transition taken (source state, exited and entered states), full updated `context`, a guard-by-guard `trace`, `effects_pending`, and `enabled_events` — what this instance can accept next; consult it instead of guessing. Execute each pending effect yourself, acknowledge with `effect_ack`, then advance with a normal domain event. Rejections (`run/unhandled`, `run/not_enabled`, `run/invariant`, `req/*` payload errors) include the same trace and `enabled_events`: read the `hint`, fix the event or payload, send again with a NEW `request_id`. Pass `expect_seq` to fail fast if the instance advanced since you last read it.

The budget test `crates/fsm-cli/tests/tools_budget.rs` builds the full `tools/list` response and asserts its canonical serialization is ≤ 20,000 bytes (≈ 5k tokens), and that the two workhorse descriptions stay ≤ 190 words each.

## 0030 — Extras

`crates/fsm-cli/src/mcp/resources.rs` (task `3001`):

- `resources/list` → `fsm://docs/spec` (`text/markdown`, name "Machine spec & expression reference") and `fsm://docs/examples` (`text/markdown`) followed by the 50 most recent machines (`fsm://machine/{machine_id}`, `application/json`, newest first). `resources/templates/list` → the single template `fsm://machine/{id}`.
- `resources/read` → for docs URIs, the embedded bytes (`include_str!("../../../../docs/SPEC.md")` and `include_str!("../../../../docs/EXAMPLES.md")`); for `fsm://machine/{id}`, the exact stored canonical spec bytes. Unknown URI → JSON-RPC `-32002` with message "Resource not found" (the MCP-conventional code; distinguished from the initialize gate by message text — both are envelope-level).
- This task creates `docs/EXAMPLES.md` as a one-paragraph placeholder stating that worked examples land in plan 0007 (task `3402` completes it), so `include_str!` compiles.

`crates/fsm-cli/src/mcp/prompts.rs` (task `3002`):

- `prompts/list` → one prompt: `{"name": "author_machine", "description": "Guided flow to author, validate, and prove a new machine from a goal.", "arguments": [{"name": "goal", "description": "What the workflow must accomplish.", "required": true}]}`.
- `prompts/get` interpolates `goal` into a single user message: read `fsm://docs/spec` → draft the spec JSON (state tree, typed context, typed events, guarded transitions, invariants) → `machine_create(dry_run: true)` until clean → create → `simulate` one happy path and one rejection path, checking traces → `instance_create` and drive with `instance_send`, consulting `enabled_events`.
- `pub const INSTRUCTIONS: &str` (~120 words), shipped in the initialize result:

  > fsm runs deterministic, auditable state machines. You author a machine as a JSON spec (a state tree with typed context, typed events, and guarded transitions), then create instances and drive them by sending events. Workflow: read fsm://docs/spec if unsure of the spec format → machine_create (use dry_run: true to check first) → instance_create → instance_send. Every response includes enabled_events — the events the instance can accept next; consult it instead of guessing. All decimal values are JSON strings ("125.50"), never numbers. When a response lists pending effects, execute them, acknowledge each with effect_ack, and advance the workflow with a normal domain event. Every error includes a hint stating the fix — correct the input and retry: the SAME request_id after a timeout, a NEW one after a correction. Use simulate to test event sequences without recording anything.

## 0031 — Proof

Golden transcripts (task `3101`): fixtures-first `crates/fsm-cli/tests/fixtures/transcripts/full_2025-06-18.{in,out}.jsonl`, `full_2025-03-26.{in,out}.jsonl`, `full_2024-11-05.{in,out}.jsonl` — the same session content per revision: initialize → resources/list → prompts/get author_machine → machine_create dry-run with a deliberate expression error → corrected create → instance_create → instance_send (applied, with effects) → effect_ack → domain event advancing to terminal → instance_history with `include_trace: true` → simulate → EOF. `crates/fsm-cli/tests/mcp_full.rs` runs each through `serve` with a temp store and a `FixedClock` (steps 1000 ms per journal append) and byte-compares the full stdout stream. `crates/fsm-cli/tests/mcp_structured_parity.rs` replays the operations behind the plan-0005 `tests/fixtures/structured/*.json` fixtures through tool dispatch and asserts `structuredContent` is byte-identical to those CLI `--json` fixtures. Authoring the `.out` fixtures by hand: derive each response from the workstream-0029 schema tables — fields appear in canonical (alphabetical) key order because the writer sorts keys — with `FixedClock` timestamps and seq-derived ids; then run the test and read the byte diff. When they differ, fix the server, unless the fixture contradicts this document or `docs/SPEC.md` — changing a fixture requires saying so in the commit message.

Naive-caller suite (task `3102`): `crates/fsm-cli/tests/naive_caller.rs` — a table of scripted wrong calls, one per error code (float-where-decimal payload, unknown event name, guard-failing payload, terminal-instance send, ambiguous machine ref, stale `expect_seq`, unknown effect id, oversized spec, malformed expression, duplicate-key spec JSON, …); for each: assert the expected `code`, then build the corrected call *from the error's `details`/`hint` data* and assert it succeeds in exactly one step. Coverage: `fsm_core::error::ALL_CODES` (a `pub const` slice added there if not yet present) minus an explicit justified allowlist (`io/*`, `internal/*`, `store/*` infrastructure codes not reachable through well-formed tool calls) must all be exercised across this suite and the goldens.
