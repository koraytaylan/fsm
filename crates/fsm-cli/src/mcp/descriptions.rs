//! Shipped tool description prose.
//!
//! Writing guidelines (keep future edits inside these):
//! 1. Open with when-to-use.
//! 2. Name the next tool in the golden loop.
//! 3. State decimals as JSON strings.
//! 4. State `request_id` retry semantics.
//! 5. State `$`-reserved names.
//! 6. Pre-teach the two or three commonest errors.
//! 7. Workhorses stay ≤ 180 words.
//! 8. List/get tools stay ≤ 40 words.

pub const MACHINE_CREATE: &str = "Create a state machine definition from a complete JSON spec, or validate without saving (`dry_run: true`). A spec declares a state tree (one initial child per compound state; terminal states are leaves), typed context variables, typed event payloads, and guarded transitions; read the resource `fsm://docs/spec` for the spec format and expression grammar before authoring your first machine. Definitions are immutable and content-addressed: `machine_id` derives from the canonical spec, so creating an identical spec twice returns the same id with `created: false` — never an error. Running instances keep the definition they started with. All decimal values are exact JSON strings (`\"19.99\"`), never numbers. On failure you get `def/*` or `expr/*` findings, each with a `path` into your spec, a character `span` for expression errors, and a `hint` stating the fix — correct the spec and call again. On success, review `warnings` before creating instances. Typical flow: `machine_create(dry_run: true)` → fix → `machine_create` → `instance_create`.";

pub const INSTANCE_SEND: &str = "Deliver one event to a running instance — the only way to advance it; every accepted or rejected event is appended to a tamper-evident journal. `request_id` is required and is an idempotency key over this request's content: resending it with the same event and payload never applies twice, it returns the original outcome with `duplicate: true` — after a timeout, retry with the SAME `request_id`. Reusing that id for a DIFFERENT event or payload is `req/request_id_conflict`, never a replay, so derive ids per attempt rather than per task. The response carries the whole situation: the transition taken (source state, exited and entered states), full updated `context`, a guard-by-guard `trace`, `effects_pending`, and `enabled_events` — what this instance can accept next; consult it instead of guessing. Execute each pending effect yourself, acknowledge with `effect_ack`, then advance with a normal domain event. Rejections (`run/unhandled`, `run/not_enabled`, `run/invariant`, `req/*` payload errors) include the same trace and `enabled_events`: read the `hint`, fix the event or payload, send again with a NEW `request_id`. Pass `expect_seq` to fail fast if the instance advanced since you last read it.";

pub const MACHINE_LIST: &str = "When you need stored definitions, list machines by optional name substring; then call machine_get.";
pub const MACHINE_GET: &str =
    "When you need one definition, get it by id, unique 12-hex prefix, or unambiguous name.";
pub const MACHINE_ANALYZE: &str = "When you need static findings, run analysis: reachability, completeness, and shadowing before instance_create.";
pub const MACHINE_DIAGRAM: &str = "When you need a picture, render mermaid or dot; optional instance overlay marks the current leaf.";
pub const INSTANCE_CREATE: &str = "When a definition is ready, create a running instance with a request_id; then drive it with instance_send. Decimals in context are JSON strings. `$` names are reserved.";
pub const EFFECT_ACK: &str = "When effects_pending is non-empty, acknowledge each executed effect with request_id, then instance_send a domain event. An ack only clears the pending effect — it never fires a transition, and `outcome: \"failed\"` is no exception: report the failure with an explicit domain event.";
pub const INSTANCE_CANCEL: &str = "When work must stop, cancel a running instance with a reason and request_id; further sends fail.";
pub const INSTANCE_GET: &str = "When you need one instance's leaf, context, pending effects, and enabled_events, get it by id.";
pub const INSTANCE_LIST: &str =
    "When you need running work, list instances filtered by machine, state, or status.";
pub const INSTANCE_HISTORY: &str = "When you need the audit trail, page journaled records; set include_trace to recompute decision traces.";
pub const SIMULATE: &str = "When you want a what-if, simulate events without journaling; rejections are findings, not failures.";
