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

pub const MACHINE_CREATE: &str = "Create an immutable, content-addressed machine, or validate without saving (`dry_run: true`). Choose one state tree or orthogonal `regions`; specs may declare typed context/events, transitions, effects, invariants, and explicit `deadlines`. Read `fsm://docs/spec` first. Cross-region transitions are invalid, and one event or deadline poll applies at most one transition. Decimal values are JSON strings (`\"19.99\"`), never numbers. Failures include a spec `path`, repair `hint`, and expression `span` when relevant. Identical specs return the same `machine_id` with `created: false`; running instances retain their definition. Flow: `machine_create(dry_run: true)` → fix → `machine_create` → `instance_create`.";

pub const INSTANCE_SEND: &str = "Deliver one event to a running instance; accepted and rejected events are journaled. `request_id` is idempotent over the request content: after a timeout retry the SAME id and payload for the original outcome with `duplicate: true`; corrected content needs a NEW id. A conflict is `req/request_id_conflict`. Inspect the tagged `configuration` (one leaf or region map), transition, `context`, trace, `deadlines_pending`, `effects_pending`, and `enabled_events`. Execute effects and acknowledge them with `effect_ack`. Deadlines never advance implicitly; call `deadline_poll` when due. Rejections include a repair `hint`. Pass `expect_seq` to detect concurrent advancement.";

pub const DEADLINE_POLL: &str = "Poll one instance with host time; time never advances implicitly. One call journals at most one deadline; a no-op also claims `request_id`. Retry the SAME id after a timeout and use a NEW id later. `expect_seq` detects concurrency. Inspect tagged `configuration`, due flags, and `deadlines_pending`; acknowledge effects with `effect_ack`.";

pub const MACHINE_LIST: &str = "When you need stored definitions, list machines by optional name substring; then call machine_get.";
pub const MACHINE_GET: &str =
    "When you need one definition, get it by id, unique 12-hex prefix, or unambiguous name.";
pub const MACHINE_ANALYZE: &str = "When you need static findings, analyze regional reachability, event completeness, deadline reachability, and shadowing before instance_create.";
pub const MACHINE_DIAGRAM: &str =
    "Render Mermaid or DOT; an instance overlay marks every active regional leaf.";
pub const INSTANCE_CREATE: &str = "When a definition is ready, create an instance with request_id; then use instance_send and deadline_poll. Decimals are JSON strings; `$` names are reserved.";
pub const EFFECT_ACK: &str = "When effects_pending is non-empty, acknowledge each executed effect with request_id, then instance_send a domain event. An ack only clears the pending effect — it never fires a transition, and `outcome: \"failed\"` is no exception: report the failure with an explicit domain event.";
pub const INSTANCE_CANCEL: &str = "When work must stop, cancel an instance with a reason and request_id; further sends fail and pending deadlines are cleared.";
pub const INSTANCE_GET: &str = "When you need an instance's tagged configuration, context, deadlines, effects, and enabled_events, get it by id.";
pub const INSTANCE_LIST: &str =
    "When you need running work, list instances by machine, active state in any region, or status.";
pub const INSTANCE_HISTORY: &str = "When you need the audit trail, page event and deadline records; include_trace recomputes decision traces.";
pub const SIMULATE: &str = "When you want a what-if, simulate events without journaling or implicit time advancement; rejections are findings.";
