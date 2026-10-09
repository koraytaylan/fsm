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
Frozen completion checkpoint `465c248a..c4f57c15` wires twelve process/MCP
cases: private owner responsiveness, inherited output pipes, public held
polling, duplicate settlement, writer refusal and durable capacity exhaustion.
The writer-refusal cases also reject an authentic completion bound to a
successor generation before recovering and settling the original.
Preliminary review confirms shared reservation ownership across helper phases
and retention until settlement; 27 mocked artifact/evidence tests, 49 portable
private-host tests, 25 public-surface/retry tests and 13 worker regressions pass.
Focused all-target clippy, formatting, size and diff checks pass.
Pre-dispatch capacity refusal stays queued (`631e7496`, load-bearing 0/101/0
regression). All eight stable host gate stages pass at frozen `0bd1e14e`;
retained task-cache report digest
`11d09f239210cfd4a4b7f71a7ba738902ed478da3ace92b5182684aa67dc5a4c`.
The twelve native cases and remaining CI platform matrix remain unexecuted,
so 8902 retains no landing OID and the original acceptance inventory stays open.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cced858136f38ba34f1ae340015acc6d79ca7ce8e0d1a40b9c3e5cc88099701b`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
