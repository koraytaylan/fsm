# Plan 0020 — Autonomous Embedded Execution — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [execution-host-ownership](tasks/8901-execution-host-ownership.md) | done | 26c682203df56c498132ab3881b48bd04cdbb46f |
| [nonblocking-execution-completions](tasks/8902-nonblocking-execution-completions.md) | in_progress | — |
| [autonomous-host-scheduling](tasks/8903-autonomous-host-scheduling.md) | planned | — |
| [bounded-session-channels](tasks/8904-bounded-session-channels.md) | planned | — |
| [autonomous-stdio-transport](tasks/9001-autonomous-stdio-transport.md) | planned | — |
| [autonomous-http-transport](tasks/9002-autonomous-http-transport.md) | planned | — |
| [autonomous-execution-contract](tasks/9003-autonomous-execution-contract.md) | planned | — |

Progress: 1/7 tasks completed.

Frozen 26c68220 closes 8901's written ownership inventory: all six stable/MSRV portable gates and both native jobs pass, with named debug/release ownership cases, verified 82-case containment matrices and twelve production workflow scenarios per toolchain. The independent frozen review has task-cache digest `24b207c7e2fd741f7d298a80c329778fad60d3c374787e5633a6300a18b16cc4`. Completion covers the owned command boundary; sibling completion, scheduling, channel and transport inventories remain separate. Plan 0022 separately completes its lifecycle inventory at `66c785ba`; final transport integration still requires plan 0020's own written acceptance.

Frozen `217e5e84` proves the private host portion of 8902's held-handler slice:
process and MCP handlers permit an owner read and unrelated durable mutation
before release, then native abort retires the original attempt without release,
on both stable and MSRV in
[CI 37863129424](https://github.com/koraytaylan/fsm/actions/runs/37863129424).
Independent retained-artifact verdict:
`568c9272b2fafe095aedb1b439b0bd33f41d0fb95b16ffe42e68d25f129250d7`.
The required public `async_completion` target now compiles but awaits native
execution; capacity, generation, settlement/recovery and full-gate requirements
remain incomplete, so 8902 retains no landing OID.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cced858136f38ba34f1ae340015acc6d79ca7ce8e0d1a40b9c3e5cc88099701b`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
