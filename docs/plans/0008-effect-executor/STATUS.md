# Plan 0008 — Effect Executor — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** a standalone `fsm execute` process that watches a store's effect outbox, runs operator-configured handlers as subprocesses, acknowledges each outcome into the tamper-evident journal, and polls due deadlines — so a triggered workflow proceeds gate-to-gate unattended while the engine's semantics stay untouched.
- **Root cause:** effects are an outbox, not an executor — nothing in the engine runs real work, so a workflow stalls the moment the driving chat session ends, and a modeled rollback path can only fire while a session is alive to send the failure event.
- **Approach:** a new zero-dependency `fsm-execute` library that re-derives each pending effect's name and args by replaying the record that emitted it, observes through `Store::open_read_only` (no lock, coexists with the MCP writer), and takes the writer only for the ticks that write; separation of a pure scheduler (all decisions journal-derived, unit-testable under explicit `now_ms` values) from the impure subprocess runner; a golden two-process session plus a restart-at-every-point chaos harness as proof.
- **Progress:** 1/13 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `d36b9545cd9aefe659106a78b74ff661e89a0bae`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Define a machine once by conversation; trigger it with one event; the executor runs it gate-to-gate with no babysitting, and `instance_history --trace` reconstructs every decision afterward.

_Task frontmatter is authoritative; this file is the roll-up._
