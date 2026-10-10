# Plan 0021 — Executor Contract Preflight — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [executor-contract-effects](tasks/9101-executor-contract-effects.md) | done | efcb4f9e2811fc388d65af9ce13f6fe5b3068b97 |
| [executor-contract-outcomes](tasks/9102-executor-contract-outcomes.md) | done | ed811ab651cfc25e53b5e7a207355c9c204e3503 |
| [executor-contract-admission](tasks/9103-executor-contract-admission.md) | in_progress | — |
| [executor-contract-cli](tasks/9201-executor-contract-cli.md) | done | 59b7e2da846ecba0baa40c05d15945de6d45a9cc |
| [executor-contract-mcp](tasks/9202-executor-contract-mcp.md) | planned | — |
| [executor-contract-acceptance](tasks/9203-executor-contract-acceptance.md) | planned | — |

Progress: 3/6 tasks completed.

Manual Phase R binds the six-task bundle to validation base
`15172d8298f271abfd0bd1d5b47617d1108ceefc` after plans 0022 and 0020 complete;
closed frontmatter, footprints and the acyclic DAG pass, with validation digest
`4298d27510c4726206598590ca50efd51e0aeca49c5605aae9f004697820f1f7`.
The CLI task closes its eleven written acceptance items at the landing OID;
frozen self-review `75972bfcb23446c8ead6c05c9410b31bc9b50b06cfffabbc0a5a4d5ae0ea39da`.

Frozen integration range `15172d82..59b7e2da` passes all six portable stable/MSRV
legs, both provisioned Linux/systemd native jobs and zero-dependency checks in
[CI 38016795840](https://github.com/koraytaylan/fsm/actions/runs/38016795840).
Downloaded logs/reports independently verify the exact source, complete native
inventories and original-run workflow/upgrade transcripts; binary identities
remain producer-attested. Frozen integration verdict:
`21fdeda473a6748f9f11b6158d16046e41fcd68faa234a8a1d5224db89de739c`.
The 83-item source audit remains applicable; final current-candidate integration
and remaining task promotion are pending.

**Historical failure disposition:** source `a6a1f815` failed the
standalone/process/collected-timeout case in CI 38014300746; that run remains
failed and its lost cleanup error leaves the cause unknown. Its retained closing
marker, missing receipt and unresolved claim are consistent with SPEC's bounded
cleanup-uncertainty contract, which forbids settlement from native absence alone;
this observation does not by itself establish a contract violation. The earlier
requirement to recover that lost error before integration is withdrawn, without
claiming a production repair or changing the failed test's verdict.

At `7e7a3011`, the five original preceding cases and collected-timeout pass in
order twenty times per compiler, retaining all closure and recovery assertions:
[CI 38026217445](https://github.com/koraytaylan/fsm/actions/runs/38026217445),
240 scenario executions independently verified. Frozen self-review:
`f678d87ea7d4ce68189fd09674fd3fd515e9f43ee22a8300be90245098a0bdad`.
These diagnostics do not replace the final complete applicable integration gate;
plan 0023 still requires plan 0021 integration before registration.

Earlier guard sensitivity, workflow milestones and reviews remain in the task
cache, including the 48 original/neutralized/restored guard proofs at `80366824`
(`975da1d2cc40c89ec89f4ea3c8d9f6b925069240b50c90e1530a756ec1b47a02`).
Historical status archive:
`773ddfb971969a6b00d65e43e225575a64c94a367628c393e74aca3d9bc2a90e`.
Volatile sessions, PIDs and intermediate logs stay outside the repository.
