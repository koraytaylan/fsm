# Plan 0008 — Effect Executor — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.

- **Goal:** a standalone `fsm execute` process that watches a store's effect outbox, runs operator-configured handlers as subprocesses, acknowledges each outcome into the tamper-evident journal, and polls due deadlines — so a triggered workflow proceeds gate-to-gate unattended while the engine's semantics stay untouched.
- **Root cause:** effects are an outbox, not an executor — nothing in the engine runs real work, so a workflow stalls the moment the driving chat session ends, and a modeled rollback path can only fire while a session is alive to send the failure event.
- **Approach:** a new zero-dependency `fsm-execute` library that observes through `Store::open_read_only` (no lock, coexists with the MCP writer) and owns one writer handle for acks; separation of a pure scheduler (all decisions, unit-testable with a `FixedClock`) from the impure subprocess runner; a golden two-process session plus a kill-mid-effect chaos harness as proof.
- **Progress:** 0/12 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Define a machine once by conversation; trigger it with one event; the executor runs it gate-to-gate with no babysitting, and `instance_history --trace` reconstructs every decision afterward.

_Task frontmatter is authoritative; this file is the roll-up._
