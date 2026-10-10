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

Frozen installed acceptance at `eeb88ed58b46df109a60687980a73dc6cdfd85f8`
establishes one actual stdio/process/success path: consumer `cargo install`,
24 passing assertions, barrier-confirmed control responsiveness, quiet-client
completion, external mutation order without overlap, exact native settlement
and advance, journal verification/replay, EOF retirement and four original
domain closures followed by owned fixture/helper removal.
[Disposable native evidence](https://github.com/koraytaylan/fsm/actions/runs/38037206190)
independently revalidates all 1,275 source blobs and retained executable bytes;
the filtered report remains ineligible for full release proof.
Frozen author replay digest:
`85aceee9d378e91f032556a098489bf97db0b8ee1f34624db326b7802d42e094`.
The preceding catalogue-provisioning failure remains failed; compact canonical
table serialization fixes its cause, and shared observation slots preserve
ownership across DynamicUser retirement rather than losing fixture evidence.

Task 9502 remains in progress: its other transport/handler/outcome cells and
complete lifecycle inventory are outstanding.
Sustained native operation, nine human-reviewed uncoached
live-model sessions, Desktop compatibility and independent final review remain
mandatory and incomplete; no passing skip or synthetic control replaces them.
Expensive integration gates run at the end of the plan against a frozen candidate.

Earlier preparation and volatile logs stay in the task cache; historical STATUS
archive digest:
`750d2e1d8cdba2f282e9cee03d13b25fc9b9d8b1f80527825686c3efa80928e5`.
