# Plan 0022 — Executor Process Lifecycle — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [lifecycle-containment-feasibility](tasks/9301-lifecycle-containment-feasibility.md) | done | 399ed6ed5636118151ffb7ad94140538f865d1a7 |
| [durable-execution-claims](tasks/9302-durable-execution-claims.md) | done | cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb |
| [contained-handler-runner](tasks/9303-contained-handler-runner.md) | done | 9f1f175ad91609359699e3a2d670119e8cbb506a |
| [executor-ownership-integration](tasks/9401-executor-ownership-integration.md) | in_progress | — |
| [bounded-executor-shutdown](tasks/9402-bounded-executor-shutdown.md) | planned | — |
| [uncertain-run-reconciliation](tasks/9403-uncertain-run-reconciliation.md) | planned | — |
| [lifecycle-crash-matrix](tasks/9404-lifecycle-crash-matrix.md) | planned | — |

Progress: 3/7 tasks completed.

Frozen 26c68220 passes all six portable gates and both independently verified 82-case native matrices, plus twelve production workflow scenarios per toolchain, including competing live-tree exclusion and final drain; the frozen review has task-cache digest `24b207c7e2fd741f7d298a80c329778fad60d3c374787e5633a6300a18b16cc4`. This establishes the production ownership path used by 8901, not completion of 9401's launch/settlement crash matrix or 9402's full shutdown inventory. Those remaining lifecycle requirements continue to gate final transport integration.

Frozen 89cf8f0c..e501d571 passes genuine native acknowledged-event recovery through a reconstructed paired driver with removed and changed handler tables on stable/MSRV, plus focused lifecycle tests, all-target executor Clippy, formatting and size checks; disabling handoff adoption fails the new recovery assertion and restoration passes. The frozen review has task-cache digest `f11fef60205f63f2791ac541c68926c543b94b660515f4769b86c2b1e7ff3d84`, including the corrected uncertainty-observer race and retained failed evidence. This proves original-event delivery once without another allocation after writer reopen, not executor-kill cutpoints or the full host crash matrix; 9401 remains in progress.

Frozen e3147b2d..0417f9e6 passes the genuine standalone-kill-after-launch path and warm completion reconciliation after another host delivers the original event on stable/MSRV, with focused tests, all-target CLI/executor Clippy, formatting and size checks; neutralizing accepted-event retirement fails the named native assertion and restoration passes. Review digest: `478b2ceee9925d014b51c7295c3ed56b95685fb4a79f0c4465515eaccb38b8ce`. The broader standalone/embedded workflow still fails when native execution is refused as authority busy after a claim, and full CI and the remaining crash cutpoints are unverified; 9401 remains in progress.

Local broker milestones before 7b978a42 served through the retained installed backend; their source pin establishes client and directly invoked helper assertions, but cannot establish changed backend behavior. Fixtures now stage the currently executing backend bytes under the protected authority, while preserving the installed helper's client and gate roles; backend acceptance must use that corrected boundary.

The retained helper is root-owned in the host namespace with its expected digest and 0711 permissions; the restricted sandbox maps host root to nobody. No ownership repair is required, and failed-run artifacts remain preserved.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cdcc311f66d08ae7b640731ed4a159d2aaf25863b59dd833afca7a99b3e9a9c9`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
