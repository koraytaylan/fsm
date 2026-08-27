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
pub const INSTANCE_MIGRATE: &str = "When a definition bug must reach instances still running, migrate one onto the corrected definition, which must declare it supersedes the machine the instance is on. Dry-run first with dry_run: true: no request_id, works read-only, reports what would change. Migration reschedules every timer from now — a deadline about to fire starts over.";
pub const INVOCATION_START: &str = "When an instance is waiting on an invocation slot, create its child with request_id. The executor normally does this unattended; call it when none is running. The child id is derived from the parent and the slot, so a retry is a replay.";
pub const INVOCATION_RETURN: &str = "When an invoked child has completed or been cancelled, hand its result to the parent with request_id. Legal only once the child has settled; the result arrives at the parent as $done.invoke.<slot>, which its transition handles.";
pub const SIGNAL_DELIVER: &str = "When signals_pending is non-empty, deliver each signal with request_id. It reaches exactly one instance, the target's own machine validates the event, and whatever happens is journaled — a signal is fire-and-forget.";
pub const INSTANCE_CANCEL: &str = "When work must stop, cancel an instance with a reason and request_id; further sends fail and pending deadlines are cleared.";
pub const INSTANCE_GET: &str = "When you need an instance's tagged configuration, context, deadlines, effects, and enabled_events, get it by id.";
pub const INSTANCE_LIST: &str =
    "When you need running work, list instances by machine, active state in any region, or status.";
pub const INSTANCE_HISTORY: &str = "When you need the audit trail, page event and deadline records; include_trace recomputes decision traces.";
pub const SIMULATE: &str = "When you want a what-if, simulate events without journaling or implicit time advancement; rejections are findings.";

pub const INSTANCE_ELICIT: &str = "Ask the user for a declared event's fields, then send it. Use at a human gate when you lack the values: the form comes from the machine's own field types, and the answer is validated and journaled exactly like instance_send. Needs a client advertising elicitation; otherwise use instance_send. A declined ask writes nothing and leaves request_id unclaimed.";

pub const EXPLAIN_STEP: &str = "Reach for this when a workflow did something surprising. Given one journaled seq from instance_history, it reconstructs the decision: which transitions were candidates, which guard decided and how it evaluated, what each action computed with before and after values, and which invariants held. Read-only. A seq that is not this instance's, or is not in the journal, is an error rather than an empty trace.";

pub const JOURNAL_VERIFY: &str = "Check that the journal is what it says it is: every record's hash chains to the one before it, and the folded state matches. Reports one of the seven recovery-table health names, the records actually walked, and — where the table prescribes one — the exact remedy command, which it never runs. Optional from_seq/to_seq check a window. Takes no lock, so it is safe beside a running executor.";

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
