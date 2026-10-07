# Plan 0020 — Autonomous Embedded Execution — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [execution-host-ownership](tasks/8901-execution-host-ownership.md) | in_progress | — |
| [nonblocking-execution-completions](tasks/8902-nonblocking-execution-completions.md) | in_progress | — |
| [autonomous-host-scheduling](tasks/8903-autonomous-host-scheduling.md) | planned | — |
| [bounded-session-channels](tasks/8904-bounded-session-channels.md) | planned | — |
| [autonomous-stdio-transport](tasks/9001-autonomous-stdio-transport.md) | planned | — |
| [autonomous-http-transport](tasks/9002-autonomous-http-transport.md) | planned | — |
| [autonomous-execution-contract](tasks/9003-autonomous-execution-contract.md) | planned | — |

Progress: 0/7 tasks completed.

Frozen 55efd81a passed the eight-stage stable Linux host gate; later focused stable/MSRV stdio checks passed at c83bbb59, but provisioned handler acceptance and the 8901 ownership inventory remain incomplete. Freeze 8901 at its written acceptance inventory: sibling scheduling and channel work does not complete or expand it. Prioritize the 0022 host-namespace native acceptance and shutdown prerequisite before further 0020 implementation.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cced858136f38ba34f1ae340015acc6d79ca7ce8e0d1a40b9c3e5cc88099701b`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
