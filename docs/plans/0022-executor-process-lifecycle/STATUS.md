# Plan 0022 — Executor Process Lifecycle — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [lifecycle-containment-feasibility](tasks/9301-lifecycle-containment-feasibility.md) | done | 399ed6ed5636118151ffb7ad94140538f865d1a7 |
| [durable-execution-claims](tasks/9302-durable-execution-claims.md) | done | cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb |
| [contained-handler-runner](tasks/9303-contained-handler-runner.md) | done | 9f1f175ad91609359699e3a2d670119e8cbb506a |
| [executor-ownership-integration](tasks/9401-executor-ownership-integration.md) | done | 2b580fd762d9afe54f844e18514387d72b6c6bd2 |
| [bounded-executor-shutdown](tasks/9402-bounded-executor-shutdown.md) | done | 5730f17202cdeabd8c34f9b1c48fcf02f26b0e06 |
| [uncertain-run-reconciliation](tasks/9403-uncertain-run-reconciliation.md) | done | 713c90e92871efb3484b41fdd20ee211dd196569 |
| [lifecycle-crash-matrix](tasks/9404-lifecycle-crash-matrix.md) | in_progress | — |

Progress: 6/7 tasks completed.

## Frozen milestone verdicts

- Ownership integration: `26c68220..2b580fd7` completes 9401 against its
  written inventory; all six portable gates, both native matrices and sixteen
  production workflow scenarios per compiler pass in
  [CI 37713754061](https://github.com/koraytaylan/fsm/actions/runs/37713754061);
  review digest `1d12bef64d598e422de8caaa09f667e2da4e34ff9a0d5f4cf9005f596438f584`.
- Shutdown: 9402 lands at `5730f17202cdeabd8c34f9b1c48fcf02f26b0e06`;
  its task frontmatter and frozen completion review remain authoritative.
- Reconciliation: `5730f172..713c90e9` completes 9403 against its written
  inventory; six portable gates and both native matrices pass at `c7fc9e8d`,
  followed only by documentation corrections;
  review digest `22c2aa44638766b727dc041bfd62d914fc26365d69288454e5c2df347502211a`.
- Crash-matrix integration predecessor: frozen `b856165e` passes six portable
  gates, zero dependencies and both native jobs in
  [CI 37852150138](https://github.com/koraytaylan/fsm/actions/runs/37852150138);
  exact job/step verdict digest
  `192d7fad22d16ca8b6b9fb6f3f917c23db0a2e43a1d802966359cfcab160d8c5`.
  This predecessor does not prove the strict complete-journal assertion added
  at `66c785ba`, and does not establish 9404 completion.
- Current crash-matrix native checkpoint: frozen `66c785ba` independently
  verifies 60 crash cases, 48 resource observations, 101 containment cases,
  34 production workflow scenarios and two historical upgrade scenarios per
  compiler in [CI 37857295898](https://github.com/koraytaylan/fsm/actions/runs/37857295898).
  Stable crash/containment verdict:
  `bf0fc2e52261f6aa40de8bfcfc58fc384b88b3121f29af6682e26c08ef2c0e39`;
  MSRV crash verdict:
  `4770d8328bc525266229f7028cf69d23f36e7b4b3b4ac1518edd11d93eb602e3`;
  MSRV containment verdict:
  `9dab51d0fdcc9a809936b96afe7568189ae60a8ea1c4d202ab5a65a7260a8232`;
  both-compiler workflow/upgrade verdict:
  `3947f3422e4e5c242d9b277a1d9740894e7c5ef77426d725f37ec9d94ce24367`.
  Exact guard-source continuity with retained sensitivity runs is verified by
  `e338cb395e8fd300ac774a513e3b39a5518af02ac271af4a1a1dc4110d6e62d9`.
  Full portable checkpoint and final written-inventory review remain pending;
  9404 remains in progress.

Native runtime scope is the explicitly approved Linux/systemd backend;
macOS/Windows retain mandatory portable gates and capability refusal.
Local closure provides no remote cancellation, rollback or exactly-once
claim; effects remain at least once, with domain-specific reconciliation.
Artifacts report executable digests but do not retain binaries for independent
byte comparison; scoped verdicts do not release a gate or complete a task.

## Historical evidence

The complete prior STATUS is preserved outside the repository with SHA-256
`284aee1267a64e904ec1801425526adacd4e73f4bcf01b7f6f5b7007e118e0dc`.
It retains intermediate and failed checkpoints, including failures with
unproved causes; later passes do not retroactively establish their cause or
prove a behavioral fix, and obsolete pending statements are historical only.
Earlier archived evidence remains addressed by
`cdcc311f66d08ae7b640731ed4a159d2aaf25863b59dd833afca7a99b3e9a9c9`.
Task-cache verdicts and artifacts hold detailed evidence; sessions, PIDs and
intermediate logs do not establish completion.

This STATUS consolidation changes documentation only; diff and source-size
checks apply, and Cargo and native runtime gates are omitted.
