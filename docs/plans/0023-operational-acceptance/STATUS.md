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

Frozen standalone restart acceptance at `05a819bc19bd4aa9f6bbbc5ad380bf2c23b5e04c`
establishes eight process/MCP abort, SIGINT, SIGTERM and SIGKILL cells with
268 passing assertions and eight original successor-owner drains:
[disposable installed evidence](https://github.com/koraytaylan/fsm/actions/runs/38046296501)
proves original death and closure before quiet recovery, unchanged attempts,
exactly one acknowledgement and advance, and actual resource restoration.
Replay revalidates 1,278 source blobs and retained executable/helper bytes,
forty original domain closures, 184 journal records and 104 raw trace records.
Seventy-nine observer tests and actual unprivileged CLI setup/report checks
pass; standalone replay uses its actual `agreement` report rather than the
MCP wrapper's `matches`. Filtered proof remains release-ineligible, with
frozen author review digest
`c3785f99d144e9fb32f80a8e0f728e1c3f8ef30853056f4d4f44d92287533485`.

Frozen active-drain acceptance at `9c359b614a2e9bcdbce3fe59d1537872828c0c94`
establishes six stdio/HTTP/standalone process/MCP cells with 218 passing assertions:
[disposable installed evidence](https://github.com/koraytaylan/fsm/actions/runs/38047364906)
confirms original draining and closed admission while the original handler waits,
then original success without successor entry, followed by quiet fresh-owner
completion and restoration; all six original advances were delivered before restart.
Replay revalidates 1,282 source blobs and retained binary/helper bytes,
24 original domain closures, 120 journal records and 72 unmodified trace records,
with six original drains and four explicit HTTP/standalone successor drains.
Ninety focused observer tests pass; removing either the original-identity or
single-owner guard makes its named fault test fail and restoration re-passes.
Filtered proof remains release-ineligible, with frozen author review digest
`2b0c68fa8efa6e2201c05399c6619071b1006b39e887c4bad2c48aa79056db6a`.

Frozen stdio observer acceptance at `6a6e1625377829d08210d8a16d06cc1bf2e0a3b5`
establishes four process/MCP paused-output and retired-output cells with
98 passing installed assertions:
[disposable installed evidence](https://github.com/koraytaylan/fsm/actions/runs/38049991343)
proves actual kernel stdout writer blockage and an unread pipe through quiet
completion and restoration, with original domain closures before reader resumption;
closing the read end instead interrupts the original owner before autonomous
fresh-owner recovery, retaining the actual BrokenPipe diagnostic and failed drainage.
Replay revalidates 1,284 source blobs and retained binary/helper bytes,
18 original domain closures, 86 journal records and 50 unmodified trace records.
Installed failure exposed a dropped initiating output error: the private queue
now retains the actual write/flush error, and the composing owner consumes it
when no earlier failure exists; healthy blocked output remains error-free.
102 observer tests, nineteen focused Rust tests, CLI all-target clippy and
formatting/size checks pass, with local builds limited to one worker,
1 GiB memory and zero swap; all local build scopes have retired.
The named reply-pause, kernel-blockage and original-error tests fail when their
guards are neutralized and pass after restoration; both preceding installed
failures remain failed in the task-cache ledger.
Filtered proof remains release-ineligible, with frozen author review digest
`a225ceb8ca8bf1fa6feb29a66a97d84aece5953c2e8c7c93faced507c72bb880`.

Frozen supervisor-death acceptance at `01d79050f6edfbadce2f3e8f49df8774e2eeec0a`
establishes six stdio/HTTP/standalone process/MCP cells with 242 passing assertions:
[disposable installed evidence](https://github.com/koraytaylan/fsm/actions/runs/38051805234)
proves the Root coordinator forcibly kills and reaps its original broker child,
while original handler liveness and durable ownership remain unchanged;
independent execution-owner death and public reconciliation still supply no closure.
Uncertain reconciliation refuses without mutation, and a distinct broker child
publishes the next irreversible epoch with the original protected configuration;
epoch replacement alone supplies no native closure.
Quiet fresh-owner recovery then establishes matching original closure before entry,
receipt-only interruption without a fabricated outcome, unchanged retry count,
exact acknowledgement/advance bookkeeping and actual external restoration.
Replay revalidates 1,286 source blobs and retained executable/helper bytes,
thirty original native bindings and closures, 138 journal records and 78 raw
trace records, with four explicit HTTP/standalone successor drains and complete
fixture/helper retirement.
111 focused observer tests pass; removing either the original-configuration or
complete-response-hash guard makes its named test fail and restoration re-passes.
Filtered proof remains release-ineligible, with frozen author review digest
`fb5f47e9e7d4d153f86e693b984d0b8e9d1707d3bf013c5e23b8eb6c1a7e7b2f`.

Task 9502 remains in progress: remaining crash cuts and the pre-existing
portable acceptance inventory are outstanding.
Sustained native operation, nine human-reviewed uncoached
live-model sessions, Desktop compatibility and independent final review remain
mandatory and incomplete; no passing skip or synthetic control replaces them.
Expensive integration gates run at the end of the plan against a frozen candidate.

Earlier preparation and volatile logs stay in the task cache; historical STATUS
archive digest:
`750d2e1d8cdba2f282e9cee03d13b25fc9b9d8b1f80527825686c3efa80928e5`.
