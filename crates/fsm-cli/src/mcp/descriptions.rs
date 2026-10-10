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

pub const EXECUTOR_CHECK_TITLE: &str = "Check executor contract";
pub const EXECUTOR_CHECK: &str = "Before machine_create, check one spec or machine against this host's table; repair/recheck. Read-only fsm.executor-check/1 covers arguments, manual policy and outcomes; unknown lacks evidence, including writer/read-only/degraded modes. Private argv/MCP literals and overrides are forbidden. Compatible neither proves progress nor bypasses admission. fsm://executor explains mode; no creation, request id, clock, polling or handler starts.";

pub const MACHINE_CREATE: &str = "Create an immutable content-addressed machine; dry_run: true validates without saving. Choose state tree or orthogonal regions with typed context/events, transitions, effects, invariants and deadlines. Read fsm://docs/spec and fsm://executor for handlers/arguments/outcomes. No cross-region transitions; one transition per event/poll. Decimals are JSON strings. Errors carry path/hint/span. Same spec: same machine_id, created: false; existing instances retain definitions. Flow: executor_check → dry_run → repair → create → instance_create.";

pub const INSTANCE_SEND: &str = "Send an event; accepted/rejected events are journaled. Timeout: retry SAME request_id/payload for duplicate: true; corrections need NEW id or req/request_id_conflict. Inspect configuration (leaf/regions), transition, context, trace, deadlines_pending, effects_pending, enabled_events. Engine effects are descriptors. fsm://executor: autonomous effects/deadlines/recovery require open stdio stdin, survive HTTP disconnects; polling observes. Legacy client_requests needs requests. Writer: run effects → effect_ack → outcome event → deadline_poll when due. hint teaches repairs; expect_seq detects concurrency.";

pub const DEADLINE_POLL: &str = "Poll with host time; no implicit time advancement. Each call journals at most one deadline; even a no-op claims request_id. Retry the SAME id after timeout, a NEW id later; expect_seq detects concurrency. Inspect configuration, due flags and deadlines_pending; effect_ack clears effects.";

pub const MACHINE_LIST: &str = "When you need stored definitions, list machines by optional name substring; then call machine_get.";
pub const MACHINE_GET: &str =
    "When you need one definition, get it by id, unique 12-hex prefix, or unambiguous name.";
pub const MACHINE_ANALYZE: &str = "When you need static findings, analyze regional reachability, event completeness, deadline reachability, and shadowing before instance_create.";
pub const MACHINE_DIAGRAM: &str =
    "Render Mermaid or DOT; an instance overlay marks every active regional leaf.";
pub const INSTANCE_CREATE: &str = "When a definition is ready, create an instance with request_id; then use instance_send and deadline_poll. Decimals are JSON strings; `$` names are reserved.";
pub const EFFECT_ACK: &str = "If effects_pending is non-empty and no executor handles it, run the side effect and acknowledge with request_id/outcome; read fsm://executor first. Handled effects are acknowledged automatically. Ack only clears pending work, never transitions; even outcome: failed needs an explicit domain event via instance_send.";
pub const INSTANCE_MIGRATE: &str = "Migrate running work to a corrected definition that declares it supersedes the current machine. First dry_run: true: read-only, no request_id, reports changes. Every timer restarts from now, including deadlines about to fire.";
pub const INVOCATION_START: &str = "Create a waiting slot's child with request_id when no executor is running; otherwise automatic. The child id derives from parent/slot, so retries replay.";
pub const INVOCATION_RETURN: &str = "Return a completed/cancelled child's result to its parent with request_id. Only settled children qualify; the parent handles $done.invoke.<slot>.";
pub const SIGNAL_DELIVER: &str = "Deliver signals_pending with request_id to exactly one target. Its machine validates the event; every outcome is journaled. Signals are fire-and-forget.";
pub const INSTANCE_CANCEL: &str =
    "Cancel with reason/request_id; further sends fail and pending deadlines clear.";
pub const INSTANCE_GET: &str = "When you need an instance's tagged configuration, context, deadlines, effects, and enabled_events, get it by id.";
pub const INSTANCE_LIST: &str =
    "When you need running work, list instances by machine, active state in any region, or status.";
pub const INSTANCE_HISTORY: &str = "When you need the audit trail, page event and deadline records; include_trace recomputes decision traces.";
pub const SIMULATE: &str = "Simulate what-if events without journaling or implicit time advancement; rejections are findings.";

pub const INSTANCE_ELICIT: &str = "At a human gate, ask for a declared event's fields; the machine supplies types, then the answer is validated/journaled like instance_send. Requires client elicitation; otherwise use instance_send. Decline writes nothing and leaves request_id unclaimed.";

pub const EXPLAIN_STEP: &str = "Explain a seq from instance_history: transition candidates, guard evaluations, computed actions with before/after values and invariants. Read-only; a missing seq or another instance's seq is an error.";

pub const JOURNAL_VERIFY: &str = "Read-only hash-chain and folded-state verification, safe beside an executor without locking. Reports seven recovery-table health names, records walked and the prescribed remedy command without running it. from_seq/to_seq restrict the window.";

pub const JOURNAL_REPLAY: &str = "Re-execute recorded outcomes through the engine; journal_verify checks bytes/chain, replay checks semantics, so verified stores may still diverge. Reports recomputed state_root for comparisons across runs/machines/backups and earliest divergent seq. to_seq limits the prefix.";

pub const STORE_DOCTOR: &str = "Diagnose health/format, record/segment counts, snapshot presence/lag and writer ownership, even when the store cannot open. Returns the recovery table's exact remedy command without running it; a person decides whether to destroy anything.";

pub const INSTANCE_ANNOTATE: &str = "Add an audit note with request_id; instance_history shows its seq. No logical change; completed/cancelled work accepts notes. req/payload_too_large claims no key.";

/// The display names a host shows beside each tool.
///
/// They live here, beside the descriptions, because a title and a
/// description that drift apart are two answers to the same question. Short
/// enough to read in a menu; the description says the rest.
pub const MACHINE_CREATE_TITLE: &str = "Create machine";
pub const MACHINE_LIST_TITLE: &str = "List machines";
pub const MACHINE_GET_TITLE: &str = "Get machine";
pub const MACHINE_ANALYZE_TITLE: &str = "Analyse machine";
pub const MACHINE_DIAGRAM_TITLE: &str = "Draw machine";
pub const INSTANCE_CREATE_TITLE: &str = "Start instance";
pub const INSTANCE_SEND_TITLE: &str = "Send event";
pub const DEADLINE_POLL_TITLE: &str = "Poll deadlines";
pub const EFFECT_ACK_TITLE: &str = "Acknowledge effect";
pub const INSTANCE_CANCEL_TITLE: &str = "Cancel instance";
pub const INSTANCE_MIGRATE_TITLE: &str = "Migrate instance";
pub const INVOCATION_START_TITLE: &str = "Start child";
pub const INVOCATION_RETURN_TITLE: &str = "Return from child";
pub const SIGNAL_DELIVER_TITLE: &str = "Deliver signal";
pub const INSTANCE_GET_TITLE: &str = "Show instance";
pub const INSTANCE_LIST_TITLE: &str = "List instances";
pub const INSTANCE_HISTORY_TITLE: &str = "Read history";
pub const SIMULATE_TITLE: &str = "Simulate events";
pub const INSTANCE_ELICIT_TITLE: &str = "Ask and send";
pub const EXPLAIN_STEP_TITLE: &str = "Explain a step";
pub const JOURNAL_VERIFY_TITLE: &str = "Verify the journal";
pub const JOURNAL_REPLAY_TITLE: &str = "Replay the journal";
pub const STORE_DOCTOR_TITLE: &str = "Diagnose the store";
pub const INSTANCE_ANNOTATE_TITLE: &str = "Leave a note";
