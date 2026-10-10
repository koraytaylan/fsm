# Plan 0021 — Executor Contract Preflight — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [executor-contract-effects](tasks/9101-executor-contract-effects.md) | done | efcb4f9e2811fc388d65af9ce13f6fe5b3068b97 |
| [executor-contract-outcomes](tasks/9102-executor-contract-outcomes.md) | done | ed811ab651cfc25e53b5e7a207355c9c204e3503 |
| [executor-contract-admission](tasks/9103-executor-contract-admission.md) | in_progress | — |
| [executor-contract-cli](tasks/9201-executor-contract-cli.md) | in_progress | — |
| [executor-contract-mcp](tasks/9202-executor-contract-mcp.md) | planned | — |
| [executor-contract-acceptance](tasks/9203-executor-contract-acceptance.md) | planned | — |

Progress: 2/6 tasks completed.

Manual Phase R binds the six-task bundle to validation base
`15172d8298f271abfd0bd1d5b47617d1108ceefc` after plans 0022 and 0020 complete;
closed frontmatter, repository-relative footprints and the acyclic DAG pass,
with validation digest
`4298d27510c4726206598590ca50efd51e0aeca49c5605aae9f004697820f1f7`.
Effects and outcomes complete their written focused inventories at the landing
OIDs above; subsequent tasks remain subject to their full plan-end gate.

Native admission checkpoint `84ee51de` passes all forty actual process/MCP
standalone/borrowed axes on stable and MSRV, including refusal/repair, fairness,
contention cleanup, warm/cold retry, migration, manual work, ack-only settlement,
acknowledged recovery and bound cancellation, with authenticated original
closures and exact settlement inventory; frozen verdict
`2d7bf695f4e4e5e0d7633ea426af27d030633c446e4a0c3ec47b81d082365989`.
Guard checkpoint `80366824` passes 48 original/neutralized/byte-restored
structural and bound-entry proofs on both toolchains; only neutralized guards
allow entry and fail at exit 101 after original cleanup; frozen verdict
`975da1d2cc40c89ec89f4ea3c8d9f6b925069240b50c90e1530a756ec1b47a02`.

Contract workflow checkpoint `0666db57` passes five real stdio MCP-client
workflows on stable and MSRV: draft/repair plus independently specified staged
refusal/recovery across process and MCP handlers and embedded/standalone drivers.
Invalid admission preserves historical work, store bytes and absent side effects;
repair permits ordered operations and compensation, with 32 authenticated
original closures across the ten physical reports and matched cleanup.
MCP axes explicitly require MCP handlers and zero server starts during refusal.
All ten downloaded reports/logs independently match the frozen source;
executable bytes remain producer-attested. Frozen verdict
`8a4a4b0c2ba52166da9bdfd0e9f8da06f230a43eaad0112ff47ba6e213dec8fc`
links the independent golden, marker, protected-binding and producer reviews.

Final focused range `0666db57..bc27be60` passes real degraded writer/embedded
stdio checks: independent draft findings survive unavailable authority,
loaded private tables are discarded, output schemas hold and retained bytes
and lock contents do not change. Ten runnable MCP cases and focused lint pass.
Historical upgrade source verification uses an exact Git archive instead of a
worktree; seven adversarial cases and the actual historical archive smoke pass,
including changed-manifest refusal. Frozen focused verdict
`a424aef9e79e6e8d9de7525e8340847adde7dcb57e676b07634c39c502a2fd7a`; the final integration matrix remains outstanding.
Tasks 9103/9201 remain in progress and 9202/9203 remain planned pending their
written acceptance inventories and plan-end gates; live-model acceptance belongs
to plan 0023 and is not claimed by these protocol clients.

Earlier scoped milestones and reviews are retained outside the repository in
the task-cache plan-status-archives directory, addressed by SHA-256:
`773ddfb971969a6b00d65e43e225575a64c94a367628c393e74aca3d9bc2a90e`.
Volatile sessions, PIDs and intermediate logs stay in the task cache.
