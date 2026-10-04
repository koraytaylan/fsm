# Plan 0020 — Autonomous Embedded Execution — Unregistered

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and registration and integration
evidence are coordinator-owned.

- **Status:** Unregistered; implementation has not started.
- **Goal:** accepted embedded workflows advance without client polling,
  with one writer and responsive, bounded stdio and HTTP sessions.
- **Root cause:** stdio runs an executor tick only after requests and HTTP
  does not retain its embedded executor; moving work into a timer alone
  would leave blocking client and handler paths inside the writer owner.
- **Approach:** build one bounded execution host, separate process work from
  settlement, schedule independently of input, and connect both transports
  with explicit session lifetime and output ordering contracts.
- **Progress:** 0/7 tasks done; 0 blocked; 0 dropped.
- **Integration:** not started; no validation base, landing OIDs, or test
  results have been recorded; final integration requires plan 0022's
  supervised lifecycle behavior.
- **Exceptions:** none recorded.
- **Outcome:** planned; no autonomous execution capability is claimed yet.

_A committed bundle remains Unregistered until exact Phase R binds its
validation base; a working-tree-only bundle is AwaitingCommit._
