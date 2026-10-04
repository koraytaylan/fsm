# Plan 0022 — Executor Process Lifecycle — Unregistered

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and the integration coordinator owns
lifecycle updates.

- **Status:** Unregistered; planned, not implemented.
- **Goal:** prevent a successor from overlapping a surviving local handler
  tree, with bounded shutdown and evidence-based restart for both process and
  MCP handlers.
- **Root cause:** direct-child handles and short-lived writer locks do not
  contain descendants or persist execution ownership across executor death.
- **Approach:** resolve the native containment prerequisite, journal claims
  before launch, prove tree closure before reuse, and drive every execution
  host through the same shutdown and recovery protocol.
- **Progress:** 0/7 tasks done; 0 blocked; 0 dropped; 1 planned gated task.
- **Integration:** not started; no validation base, landing OIDs, native
  feasibility result or implementation gate evidence is claimed.
- **Exceptions:** task `lifecycle-containment-feasibility` requires a proved
  charter-compatible backend or an explicit project decision before dependent
  implementation; durable fail-closed behavior alone cannot complete this plan.
- **Outcome:** pending implementation and native lifecycle evidence.

_A committed bundle remains Unregistered until Phase R binds its validation base._
