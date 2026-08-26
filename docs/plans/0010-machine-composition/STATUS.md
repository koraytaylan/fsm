# Plan 0010 — Machine Composition — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** a state can invoke another machine by content hash, wait for it, and read its result through `$done.invoke.<slot>`; one instance can signal exactly one other; and every edge between instances is a journal record rather than an inference.
- **Root cause:** a 256-state definition ceiling is only defensible if large workflows compose out of small machines, and nothing composes — there is no invocation, no correlation beyond free-text tags, and no path between two instances that does not leave through a subprocess and come back.
- **Approach:** copy the outbox pattern the engine already uses for effects — the pure core emits a pending invocation, two idempotent store operations enact it one record at a time, and the parent advances on a generated event that plan 0009's bounded queue already knows how to deliver; derive the child instance id from `(parent, slot)` so enactment is idempotent by construction and a reader can compute it; pin the invoked machine by hash so the invocation graph is statically checkable for cycles and depth; and refuse broadcast signalling outright, because a query-targeted delivery would stop the store being a function of its journal.
- **Progress:** 3/14 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A workflow too big for one definition is authored as several, and the store can answer "why does this instance exist" and "what is it waiting for" from records rather than from a naming convention.

_Task frontmatter is authoritative; this file is the roll-up._
