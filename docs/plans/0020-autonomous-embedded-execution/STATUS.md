# Plan 0020 — Autonomous Embedded Execution — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [execution-host-ownership](tasks/8901-execution-host-ownership.md) | done | 26c682203df56c498132ab3881b48bd04cdbb46f |
| [nonblocking-execution-completions](tasks/8902-nonblocking-execution-completions.md) | done | ee296852b5cdaad2d1f88781fe6a390248233240 |
| [autonomous-host-scheduling](tasks/8903-autonomous-host-scheduling.md) | done | 6666a55b57a47bd2568bc5113231f76d7a0fdea6 |
| [bounded-session-channels](tasks/8904-bounded-session-channels.md) | done | 25129d2dd238f717e9b2e4ec2e0c148fa4633c13 |
| [autonomous-stdio-transport](tasks/9001-autonomous-stdio-transport.md) | planned | — |
| [autonomous-http-transport](tasks/9002-autonomous-http-transport.md) | planned | — |
| [autonomous-execution-contract](tasks/9003-autonomous-execution-contract.md) | planned | — |

Progress: 4/7 tasks completed.

Frozen 26c68220 closes 8901's written ownership inventory: all six stable/MSRV portable gates and both native jobs pass, with named debug/release ownership cases, verified 82-case containment matrices and twelve production workflow scenarios per toolchain. The independent frozen review has task-cache digest `24b207c7e2fd741f7d298a80c329778fad60d3c374787e5633a6300a18b16cc4`. Completion covers the owned command boundary; sibling completion, scheduling, channel and transport inventories remain separate. Plan 0022 separately completes its lifecycle inventory at `66c785ba`; final transport integration still requires plan 0020's own written acceptance.

Frozen `ee296852` closes 8902's original completion inventory in
[CI 37894146107](https://github.com/koraytaylan/fsm/actions/runs/37894146107):
six stable/MSRV portable gates, both native jobs and zero-dependency acceptance
pass. Independent frozen verdict:
`a91f05d4cc8d7b374681f327ffea2163630f7dd87cad9dded6209cf820cc43af`.
Per native toolchain: twelve completion cases, sixty crash cases with forty-eight
resource observations, 101 containment cases, thirty-four workflow scenarios
and two original-executor upgrade scenarios pass. Held-handler responsiveness,
inherited output pipes, duplicate/stale completion, writer refusal, exhausted
capacity and interrupted settlement are covered through the real lifecycle
adapter. Reports and logs bind the frozen source and compiler; executable bytes
are not independently compared. Native scope remains Linux/systemd.
The eight-stage stable host report at `0bd1e14e` has digest
`11d09f239210cfd4a4b7f71a7ba738902ed478da3ace92b5182684aa67dc5a4c`.
Scheduling, channels and transport inventories remain separate and incomplete.

Frozen `ee296852..6666a55b` closes 8903's focused scheduling inventory in
[CI 37931177530](https://github.com/koraytaylan/fsm/actions/runs/37931177530):
both native toolchains pass fourteen genuine process/MCP cases, including
seven matched original results ready before owner resumption and all nine
settlements under continuously queued application work. Nine local scheduling
tests, exact command/decision guard sensitivity, affected CLI test-target
clippy, formatting and frozen-range diff checks pass. The shared eight-tick
owner loop and one-original-native-action-per-tick wiring establish the bound;
the seven-inflight native fixture alone is not an exact eight/nine distinction.
Independent verdict:
`3475897e52a3d6f09290ba4c385cdc2cd6c46178ed8413357f59979a7fd9a41a`.
Executable bytes were not independently compared; native scope remains
Linux/systemd. Full stable host and portable integration gates remain due at
plan completion under the explicit user cadence, with no current gate claim.
Task 8904 now owns the unchanged bounded-session-channel inventory; stdio,
HTTP and public contract tasks remain incomplete.

Frozen `6666a55b..25129d2d` closes 8904's original focused channel inventory:
29 channel cases, eighteen elicitation/cancellation cases and seven subscription
cases pass, with affected test-target clippy, formatting, frozen-range diff
checks and named retirement/publication guard sensitivity. The review repaired
separate-feed elicitation ordering and added production isolation proof for
all three egress ceilings; its final self-review found no further task-scope
defect. Task-cache verdict digest:
`fe569635e90f67cf49457be2beab4cd5eba679b5990152fcf23effd1c0d8bccb`.
No independent reviewer or fresh full/platform gate is claimed; plan-end gates
and real-binary stdio/HTTP acceptance remain outstanding.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cced858136f38ba34f1ae340015acc6d79ca7ce8e0d1a40b9c3e5cc88099701b`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
