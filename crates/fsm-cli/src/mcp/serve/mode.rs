//! Session-mode instructions; preserve the established byte-exact wording.

use crate::store::Store;

/// The sentence appended to `instructions` when this server is not the plain
/// writer, so a model can tell what it is allowed to do here.
///
/// The default mode adds nothing at all: the instructions are part of a
/// byte-compared transcript, and a mode that changes them would move that
/// golden for every existing deployment.
pub(super) fn mode_note(
    store: Option<&Store>,
    embedded: bool,
    admitted_progress: bool,
    degraded: bool,
    contended: bool,
) -> &'static str {
    if contended {
        "\n\nThis server could not take the writer because another process holds it (mode=read-only, contended): the store is healthy and busy, not broken. Read tools work normally and writes are refused. Stop the other writer, or use the paired deployment where the executor writes and this server watches."
    } else if degraded {
        "\n\nThis server could not open its store (mode=degraded): every tool that reads or writes instances is refused, and each refusal carries the health, the blast radius, and the remedy. Call store_doctor for the diagnosis; journal_verify and journal_replay also answer, a machine_create with dry_run still validates, and the documentation resources still read."
    } else if store.is_some_and(|store| store.journal.is_read_only()) {
        "\n\nThis server is running read-only (mode=read-only): this connection runs no effects and cannot confirm whether an external executor is running or which handlers it has, so machine_create, instance_create, instance_send, deadline_poll, effect_ack, and instance_cancel are refused here. Read tools work normally, and a machine_create with dry_run still validates. Read fsm://executor for the limits of this connection. If an external executor is running, subscribe to fsm://instance/{id} to watch it advance a workflow."
    } else if embedded && admitted_progress {
        "\n\nThis session runs configured effects (mode=embedded): already admitted effects can finish while you are quiet, but pending work, retries and machine deadlines advance only when you send requests. Do not call effect_ack for handled effects. Read fsm://executor for the actual handler contracts and fsm://docs/embedding for setup."
    } else if embedded {
        "\n\nThis server runs the effect executor inline (mode=embedded): a handler table maps each effect name to a host command or MCP tool call, and one tick runs per request you send — so a workflow advances while you are talking to it and pauses when you stop. Effects with configured handlers are executed and acked automatically; do not call effect_ack for handled effects. Read fsm://executor for the actual handler contracts. Keep sending instance_get or ping through completion and failure recovery: subscribing alone does not advance the workflow. Read fsm://docs/embedding for setup."
    } else {
        ""
    }
}
