# Plan 0020 — Autonomous Embedded Execution — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and registration and integration
evidence are coordinator-owned.

- **Status:** Registered by hand; execution-host-ownership is in progress;
  private Store owner and bounded command admission are implemented, with
  executor state, cancellation and transport integration still outstanding.
- **Goal:** accepted embedded workflows advance without client polling,
  with one writer and responsive, bounded stdio and HTTP sessions.
- **Root cause:** stdio runs an executor tick only after requests and HTTP
  does not retain its embedded executor; moving work into a timer alone
  would leave blocking client and handler paths inside the writer owner.
- **Approach:** build one bounded execution host, separate process work from
  settlement, schedule independently of input, and connect both transports
  with explicit session lifetime and output ordering contracts.
- **Progress:** 0/7 tasks done; 0 blocked; 0 dropped.
- **Integration:** Phase R bound by hand on `develop` to validation base
  `41f9350182b2437b6b97855487df253667ee90d0`, following the recorded manual
  coordinator mode of plans 0019 and 0022; this is not a Makina invocation.
  The committed scope SHA-256 is
  `0c79c9e7f23b0032852ffea32417470bfca894a254702732be7eeea0017c7239`.
  Seven closed task headers, matching IDs/titles/workstreams, ordered steps,
  unique per-task mutation footprints and six dependency edges validate as
  one acyclic local DAG; no manifest owner requires footprint adoption.
  No task has a landing OID or completed acceptance inventory; final
  integration still requires plan 0022's supervised lifecycle behavior.
- **Exceptions:** none recorded.
- **Outcome:** planned; no autonomous execution capability is claimed yet.

Registration makes the dependency-free ungated ownership task ready; it does
not establish autonomous execution or release any cross-plan prerequisite.

### Initial private owner and admission — 2026-10-07

The private `mcp/host/` owner now retains the Store and injected clock, accepts
owned tool/RPC envelopes, applies complete ordinary dispatch operations and
captures immutable results, committed sequence and appended interval. Host and
session count/owned-allocation budgets survive dequeue until retirement; stop
and original-generation close bypass application admission. Seven real Store
command-boundary cases pass on stable and Rust 1.89 with all-target CLI Clippy,
formatting and size checks in terminal session 88778. Four isolated guard
neutralizations each failed their production-envelope assertions and restored
the original mailbox bytes; session 95084 then failed only the new test's
formatting, repaired before 88778. HOST-OWNERSHIP-REVIEW.md records the scope.

This starts task 8901 without completing it: executor state, reserved cancel
control and interaction/diagnostic separation remain unfinished; transports do
not construct this owner yet, and bounded egress and autonomous scheduling are
not established. Full changed-source stable host gate and platform CI remain
pending, merged_as stays empty, and completion remains 0/7.

### Complete initial-owner stable host gate — 2026-10-07

Frozen b817071 passes all eight stable host stages in terminal session 45445,
with all seven private host cases passing in debug and release. Exact-source,
clean-worktree, log-hash and no-swap kernel readbacks are recorded in
HOST-OWNERSHIP-REVIEW.md. This evidence-only update does not repeat unchanged
code gates; cancellation, executor state, interaction and transport integration
remain unfinished, and task 8901 stays in progress at 0/7 tasks complete.
