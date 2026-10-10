# Plan 0023 — Operational Acceptance — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [acceptance-evidence-reports](tasks/9501-acceptance-evidence-reports.md) | done | 83de6523169394fa5bc329034a9b857d489c73ad |
| [installed-executor-acceptance](tasks/9502-installed-executor-acceptance.md) | in_progress | — |
| [operational-soak-harness](tasks/9601-operational-soak-harness.md) | planned | — |
| [native-operational-evidence](tasks/9602-native-operational-evidence.md) | planned | — |
| [live-model-acceptance-protocol](tasks/9701-live-model-acceptance-protocol.md) | planned | — |
| [candidate-evidence-gate](tasks/9702-candidate-evidence-gate.md) | planned | — |
| [operational-review-closure](tasks/9703-operational-review-closure.md) | planned | — |

Progress: 1/7 tasks completed.

Manual Phase R binds the seven-task bundle to clean validation base
`106c833cfc1b9aee18947164f752ee7f244476e0` after plans 0022, 0020 and 0021
complete; closed frontmatter, relative footprints and the acyclic DAG pass.
The critic assessment is by-hand author review, not an independent model receipt.
Registration validation digest:
`6531b4b11621359e05836bbff606eb2644895f7e5ff4a408b86a98a73d5315bf`.

Frozen reporter acceptance through `83de6523` closes its nine written criteria:
13 reporter tests, actual host-installed reports, an actual Podman consumer
with 80 passing offline assertions and retained reports after container removal,
verified executable bytes, installed identity refusals, labelled synthetic
negative controls and an actual unwritable-directory failure with nonzero exit.
The consumer's effective limits are 1 GiB memory and zero swap; filtered reports
are valid but ineligible for full release proof. Frozen author review:
`a9fec0b99f35caf8ee1f49a897db9f7fcc8e3ea2c29078300bdc49409513685b`.

Frozen installed acceptance at `a6d3d73e5e731d6a6527fa4a0d7b6a7d2e48da1a`
establishes all sixteen stdio/HTTP outcome cells across process and MCP handlers:
consumer `cargo install`, 464 passing assertions, barrier-confirmed control
responsiveness, quiet-client completion, honest compensation/restoration state,
external mutation order without overlap, exact native settlement and advance,
journal verification/replay, EOF retirement, eight explicit HTTP owner drains
and 52 original domain closures followed by owned fixture/helper removal;
HTTP session deletion leaves the shared host alive before its actual owner drain.
[Disposable native evidence](https://github.com/koraytaylan/fsm/actions/runs/38038741845)
independently revalidates all 1,275 source blobs and retained executable bytes;
each protected native binding matches its original journal claim hash and domain.
The filtered report remains ineligible for full release proof.
Frozen author replay digest:
`a1767394088ed6666b69ea4b0574ecef92a50e1c47f1a2dbbba19cb803a82181`.
The preceding catalogue-provisioning failure remains failed; compact canonical
table serialization fixes its cause, and shared observation slots preserve
ownership across DynamicUser retirement rather than losing fixture evidence.
HTTP harness corrections use the documented launch options and original stop
control; their preceding failed runs remain failed, and SIGTERM is not drain.

Frozen admission/manual acceptance at `8e1689e13346232122b0484a508ae74df8f2567f`
establishes four stdio/HTTP process/MCP cells with 176 passing assertions:
exact draft and runtime incompatibility diagnostics, unchanged incompatible
and manual pending effects while valid work completes, no fabricated claim,
attempt or acknowledgement, sixteen original domain closures and two HTTP drains.
[Disposable installed evidence](https://github.com/koraytaylan/fsm/actions/runs/38039558162)
revalidates all 1,275 source blobs and retained binary/helper bytes;
filtered proof remains release-ineligible, with frozen author replay digest
`0c4e019c1862d5eb0eb27e20703a235ff767cd8fda15ef3f35db0b6f795c6c7b`.

Frozen refusal/session acceptance at `35bfb1bc3da6e17ec142e72ceae9664896d61147`
establishes twelve installed cells with 336 passing assertions:
[eight read-only/degraded cells](https://github.com/koraytaylan/fsm/actions/runs/38041903984)
preserve pending work and journal bytes, retain exact diagnostics and prove zero
native allocations from the original protected counters;
[four active HTTP observer cells](https://github.com/koraytaylan/fsm/actions/runs/38041913801)
complete with an unread subscription or deleted session before reconnect,
with sixteen original domain closures and four original-owner drains.
Both receipts independently revalidate 1,276 source blobs and the same retained
binary/helper bytes; filtered reports remain release-ineligible.
Installed refusal exposed and repaired last-line JSON routing: degraded
initialization now returns its matching response and retains the diagnostic
in the original session's bounded SSE history, including actual stream delivery.
The named endpoint test fails at exit 101 with its ID guard neutralized and
re-passes after restoration; fourteen focused Rust tests, seventy observer
tests, CLI all-target clippy, formatting and size checks pass with local builds
limited to one worker, 1 GiB memory and zero swap.
Frozen author review and independent artifact replay digest:
`ce77ca78c96dc4aeb18b16d4ee00bead818362d5c9277f11db90a29be8c27447`.
The original failed attempts remain failed in the task-cache evidence.

Frozen owner restart acceptance at `364fb6b3257050287018ac8545410aaf5ad7b054`
establishes eighteen process/MCP cells with 520 passing assertions across actual
stdio EOF and stdio/HTTP out-of-band abort, SIGINT, SIGTERM and SIGKILL:
[ten stdio cells](https://github.com/koraytaylan/fsm/actions/runs/38045653179)
and [eight HTTP cells](https://github.com/koraytaylan/fsm/actions/runs/38045650739)
prove original fixture death before replacement, original protected closure
before retry, preserved retry count, exactly one acknowledgement and advance
per recovered effect, autonomous completion, actual resource restoration and
eight original successor-owner drains distinct from HTTP session closure.
Both receipts revalidate 1,278 source blobs and identical binary/helper bytes;
replay checks ninety original domain closures, 414 journal records and 234
unmodified fixture trace records.
The observer preserves an interrupted result only with its matching complete
protected response attestation; the original interrupted start stays in the
raw trace, and the two preceding observer failures remain failed.
Installed HTTP recovery also exposed stale endpoint ambiguity: discovery now
preserves original files and excludes only actual refused connections, retaining
one connected owner while every other error or multiple connected owners refuse.
Its failed installed attempt remains failed; 35 focused Rust tests, CLI all-target
clippy, formatting/size and 79 observer tests pass, with local builds limited to
one worker, 1 GiB memory and zero swap.
The named response-hash and refused-socket guard tests fail when neutralized
and pass after restoration; no refusal establishes native cleanup.
Filtered proof remains release-ineligible, with frozen author review digest
`4b937650daabc000e16ddefea3c38d03d9474d1b5df2a6771b280202ecafffe6`.

Task 9502 remains in progress: slow/retired stdio observers, active drain,
standalone lifecycle paths, remaining crash cuts and the pre-existing
portable acceptance inventory are outstanding.
Sustained native operation, nine human-reviewed uncoached
live-model sessions, Desktop compatibility and independent final review remain
mandatory and incomplete; no passing skip or synthetic control replaces them.
Expensive integration gates run at the end of the plan against a frozen candidate.

Earlier preparation and volatile logs stay in the task cache; historical STATUS
archive digest:
`750d2e1d8cdba2f282e9cee03d13b25fc9b9d8b1f80527825686c3efa80928e5`.
