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

Frozen a243de84..3da1a31a passes all thirteen production workflow scenarios and both enrolled-authorization and provisioned-broker cases on stable/MSRV with the frozen backend, plus focused tests, all-target CLI/executor Clippy, formatting and size checks; review digest `184976e75460ff323fcaebf9546555d1e9ae42f7822c30678e05b6a926eb1e94`. This resolves the earlier local stale-backend contention evidence and corrects warm completion delivery without loosening cold operator routing; full CI and the remaining lifecycle cutpoints are unverified.

Frozen 3da1a31a..5a826175 passes genuine standalone death after verified native closure while the writer remains held, then original timeout settlement and sequential attempt-two recovery on stable/MSRV; review digest `ab97304614bfc85675d4dfa60aca5348dd7bff6b7095cf064bd1078e951e9c71`. The new cut exposed missing native attempted settlements in watcher retry observations; removing that observation reproduces the named stall and restoration passes. This proves the new fourteenth scenario through focused checks, not the full expanded inventory or the remaining claim/stopped/settlement host cutpoints; 9401 remains in progress.

The retained helper is root-owned in the host namespace with its expected digest and 0711 permissions; the restricted sandbox maps host root to nobody. No ownership repair is required, and failed-run artifacts remain preserved.

Frozen 8d5b705e..ad0ac436 passes actual SIGKILL of an independent public Pipeline caller after its durable stopped append while holding the writer, then exact stopped-state recovery, claim exclusion until single-consumption settlement and original event delivery without another allocation on stable/MSRV; review digest `85117ec9330d163ab8c0e8a3f674a38c60a931c3205e15c81a4d064e44b1dde8`. Lifecycle tests, all-target executor Clippy, the genuine enrolled native case, formatting and size checks pass. This proves the public Pipeline cut, not every standalone/embedded/public-tick host at every boundary; remaining claim and post-settlement process cuts and full CI keep 9401 in progress.

Frozen b14c5d51..7d2954e2 passes actual SIGKILL of an independent public Pipeline caller after durable acknowledgement and before its original outcome event, then reconstructed configured-operator delivery once without another allocation on stable/MSRV; review digest `453f0b76fb31138cdd6bad8c75f1315db98a2e941f96910c02b68e82935e9797`. Reopened journal assertions, focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. This proves the public Pipeline acknowledgement cut; after-claim and post-attempt process cuts, the remaining host matrix and full CI are unverified, so 9401 remains in progress.

Frozen db5f6273..015efced passes actual SIGKILL of an independent public Pipeline caller after durable retry disposition, then exact nonmutating attempt replay and original backoff recovery on stable/MSRV; review digest `e1c7c19fb86aa1a1436fb00006c5d394d3bdf03934a4ff5c5d4fedc79473a58d`. The successor claim refuses at 1010, accepts as attempt two at 1011 and refuses changed contracts; focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. The retained initial failure was a fixture assertion reading response-envelope fields from the journal body. This proves successor claim timing, not successor launch or the remaining host matrix; after-claim process cuts and full CI remain unverified, and 9401 stays in progress.

Frozen b0e6712d..23ab4d96 extends post-attempt public-host recovery through genuine successor enrollment and final exhausted-timeout settlement on stable/MSRV; review digest `13b60c903e38c86b86088069d9d1e0f7e7da66f5dd40a6cbdf9ff209395bb63f`. The reopened attempt-two claim precedes entry, independent root/descendant membership matches allocation two while the original unit is absent, and stale original completion refuses without mutation. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass; fixture cleanup now accounts for already-retired DynamicUser IPC markers after verified closure. This closes the prior claim-only successor limit, not after-claim process cuts or the remaining host matrix; full CI is unverified and 9401 remains in progress.

Frozen 5dd4aab8..86a32177 passes actual SIGKILL of an independent fresh-claim caller before binding or handler entry on stable/MSRV; review digest `69e998248970fe5b7d2141182847a5412d720411cc488640b2b9fd22c389c834`. The reopened original claim excludes a competitor, genuine unlaunched native closure permits durable interruption, and the pending effect remains unacknowledged with no launch or entry records. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. This establishes the public claim-caller boundary, not every standalone/embedded/public-tick host cut or interrupted-successor launch; the remaining host matrix and full CI keep 9401 in progress.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cdcc311f66d08ae7b640731ed4a159d2aaf25863b59dd833afca7a99b3e9a9c9`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
