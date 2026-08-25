//! Derived idempotency keys.
//!
//! The executor survives its own death by never inventing a key. Every
//! `request_id` here is a pure function of content the journal already holds,
//! so a restarted executor re-issues the identical key for the identical
//! intent and the store answers `duplicate: true` instead of applying it a
//! second time.
//!
//! # Why derivation is enough
//!
//! The store keys idempotency on the pair `(request_id, request fingerprint)`
//! — see `fsm_store::store::Store::lookup_request`. Because both halves derive
//! from journaled state, a re-issue after a restart matches on both and
//! replays. A key re-used for *different* content is refused with
//! `req/request_id_conflict` rather than replayed, which is the behaviour that
//! makes these keys safe to derive rather than merely convenient.
//!
//! # The one collision that can still happen
//!
//! An ack's fingerprint covers its `result`, and `result` carries the
//! handler's captured output. One executor never re-acks a settled effect —
//! the ack clears it from `effects_pending`, so nothing restarts it — but two
//! writers racing the same effect (an executor plus an embedded serve, say)
//! produce equal keys over *different* captured output, and the loser is
//! refused with `req/request_id_conflict`. The pipeline surfaces that as
//! `exec/store` and halts that directive instead of writing twice. That
//! refusal is the design working; the run modes exist to prevent the race.

/// The key for acknowledging one effect.
///
/// `effect_id` is already `{instance}/{seq}/{k}`, unique for the life of the
/// journal, so it needs no further qualification.
pub fn ack_rid(effect_id: &str) -> String {
    format!("exec-ack-{effect_id}")
}

/// The key for the advance event one effect's outcome triggers.
///
/// The event name is part of the key because one effect's `on_ok` and
/// `on_failed` name different events, and each is its own write.
pub fn event_rid(effect_id: &str, event: &str) -> String {
    format!("exec-ev-{effect_id}-{event}")
}

/// The key for one observation of one due deadline.
///
/// `due_ms` is in the key so a *new* due time gets a new key, honouring SPEC
/// §Deadline poll's rule that a caller uses a new `request_id` for a new
/// observation: a rescheduled deadline is a different observation, and
/// replaying the old key would answer with the old poll's outcome.
///
/// The deadline name is a key *ingredient*, not a selector. The store applies
/// whichever deadline is next due by `(due_ms, document index)` and takes no
/// name, so two due deadlines mean two directives under two keys, each of
/// which polls once.
pub fn poll_rid(instance_id: &str, deadline: &str, due_ms: i64) -> String {
    format!("exec-poll-{instance_id}-{deadline}-{due_ms}")
}
