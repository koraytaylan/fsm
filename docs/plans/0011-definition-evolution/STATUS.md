# Plan 0011 — Definition Evolution — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** an explicit, journaled, idempotent migration that moves a running instance onto a corrected definition under a mapping the new definition itself declares — with a preview that answers "what would this do" before anything is written.
- **Root cause:** `machine_id` is a content hash, so editing a machine mints a different machine and every in-flight instance stays bound to the old one for life; a guard fixed in week one cannot reach a workflow that started in week zero without cancelling it and discarding its context, history, and journal continuity.
- **Approach:** declare the mapping in the new definition so it is part of that machine's canonical bytes and can never be reinterpreted later; make the migration a pure function with a refusal for every case it cannot cover, including four explicit carry-over rulings for history, deadlines, pending effects, and invocation slots; journal one record carrying both machine ids and the report, and teach replay to track the current machine per instance so one instance's records legitimately span two definitions; and ship the cohort command as N idempotent operations that resume rather than as a transaction that cannot exist.
- **Progress:** 10/11 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A definition bug found on day thirty is fixed for the instances that are still running, without cancelling them and without rewriting a byte of what they already did.

_Task frontmatter is authoritative; this file is the roll-up._
