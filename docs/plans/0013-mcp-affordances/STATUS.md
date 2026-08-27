# Plan 0013 — MCP Affordances — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** publish the facts the server already holds — derived tool annotations and titles so a host can auto-approve reads and gate writes, completion of machine ids, instance ids, and enabled event names, and an elicitation path that turns a machine's typed event fields into a form a person can fill in.
- **Root cause:** `tools/list` carries no `title` and no `annotations` even though `MUTATING_TOOLS` already encodes the read/write split and every mutating tool is exactly-once by `request_id`; `completion/complete` is unimplemented even though the server holds every id and computes `enabled_events` for its own purposes; and a workflow at a human gate stalls until somebody thinks to send the event.
- **Approach:** derive every annotation from the code that already owns the fact rather than hand-writing a second table that can disagree with the gate; complete resource-template variables and prompt arguments only, which is what the protocol actually defines, and use the resolved-argument context so `event` completes from the named instance's own analysis; and make elicitation compatible with the never-parse-natural-language rule by deriving a flat typed schema from declared fields, coercing the structured response through the same path an external payload takes, and journaling the event rather than the conversation.
- **Progress:** 6/10 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A host stops treating `instance_cancel` like `instance_get`, a model stops guessing at identifiers it could have been offered, and a workflow waiting on a person can ask.

_Task frontmatter is authoritative; this file is the roll-up._
