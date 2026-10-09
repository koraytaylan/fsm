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

Admission milestone `b373a453..f07056df` has 61 passing focused cases:
twelve admission/service, fifteen pipeline recovery, nine historical-effect
and 25 mocked native producer/verifier cases; feature-enabled executor
all-target clippy and format/size/diff checks pass. Both writer entries retain
manual work, refuse changed receiver contracts after migration and preserve
acknowledged recovery keys until repair. Prior guard sensitivity and scoped
reviews retain their original ranges, indexed by frozen verdict
`636348f8b670a0efa300617801dc592875fd261b9d4086e6d0f51f0a2d5e1163`.
Twenty-eight provisioned process/MCP axes remain unexecuted and fourteen protected Rust
cases remain ignored locally; unprovisioned routing proves no native side-effect
acceptance. Complete task 9103 inventory and plan-end gates remain outstanding;
full-prefix reconstruction cost stays documented.

Cache/bound-entry proof range `909cb8e0..5918b02e` has six passing cache cases
and four bound-entry cases, with executor all-target clippy/format/size/diff
checks. Catalogue identity and bound contract guard neutralization each fail
their named case; restored suites pass. Cancellation/acknowledgement preserve
the retained bound owner and unconsumed entry permission. Frozen review
`a4a864a0e44c963e09801401785884712c4211490d0bc8e8335de5b6ed4cae97`
indexes prior scoped proofs; metadata fixtures do not close physical acceptance.

Focused regression checkpoint `8b659d64` passes 49 tick, retry, scheduler,
composition and chaos cases; verdict
`eedbf3f61b365721543985fc1eadab72632c3e2511c5b8f0232ae554659bf0e9`.
The provisioned inventory now includes both initial writer entry paths crossed
with process/MCP timeout cleanup under a competing writer: original process
identities must disappear before writer release, blocked work creates no claim
or marker, and the retained original owner subsequently settles once.
Manual-to-handler repair and intentional no-outcome execution are also crossed
with both writer entry paths and handler kinds; ack-only execution requires
one acknowledgement, no applied event and no further journal changes.
Focused admission cases (12) and mocked producer/verifier cases (25) pass;
executor feature-enabled all-target clippy and format/size/diff checks pass.
Content-addressed focused review:
`5cd1565e580013e44b2ab8e0ed833c62634a6ac71bf54a60979b0da5a1c892f7`;
physical execution remains outstanding until provisioned plan-end CI.

CLI focused range `62bdbd5d..2cd48842` passes ten real-binary contract cases,
17 executor documentation cases and five legacy session cases, CLI all-target
clippy, format/size checks and control-option guard sensitivity; self-review
digest `eee78cc01606dac313ddc3f177301e7abff6d34d618fd3e2a48c084d4436daa0`.
Task 9201's written focused inventory passes on stable Linux; it remains in
progress until the plan-end portability and integration gates succeed.

MCP independent preparation range `746ec763..df3539b1` advertises and dispatches
read-only `executor_check` through the original session's private host table.
120 distinct focused stable Linux cases pass: three real stdio/schema cases,
76 host cases, eight draft-analysis cases and 33 discovery/schema/transcript
cases; nine provisioned native host cases remain ignored. CLI all-target clippy
and format/size/range-diff checks pass, including restored exact queue-byte
boundaries; review digest
`4538b02dda6d1a1fd01290fd9dd51f57702762f9817c3563b90373e152f60dde`
indexes the prior preparation reviews with their original scopes. Task 9202
remains planned behind admission acceptance; genuine embedded draft-to-execution,
actual HTTP authority, complete mode/bypass inventory and plan-end gates remain
outstanding, with no task-completion or native-handler acceptance claim.

Native MCP preparation range `5fe38b22..a49b0a83` adds the protected original-host
draft/repair/create/quiet-execution case to the existing disposable workflow.
Four focused Rust cases and 22 mocked artifact/producer cases pass, with
feature-enabled CLI/executor all-target clippy and format/size/diff checks;
the genuine native case remains ignored locally and unexecuted. Frozen review
digest `314b7a451cf586eca53de2a358bf938d299009729e0bf4e497ca9a5192765496`
indexes the preceding MCP verdict; task 9202 and plan-end acceptance remain open.

Manual Phase R binds the six-task bundle to validation base `15172d8298f271abfd0bd1d5b47617d1108ceefc` after plans 0022 and 0020 complete: closed frontmatter, repository-relative footprints and the acyclic local dependency graph pass, with uncreated deliverables explicitly inventoried; task-cache validation digest `4298d27510c4726206598590ca50efd51e0aeca49c5605aae9f004697820f1f7`. Task 9101 completes its focused inventory at `efcb4f9e`, with 67 stable effect/configuration/public-surface cases and focused lint/format/diff checks passing; self-review verdict `dfd37ee9a1867119e4c63a048ad58b1030b7db590fe118b95dcee1edff82f9a1`; task 9102 completes at `ed811ab6` with eleven focused outcome tests and self-review verdict `b9a671329f5a8177a34529118e6925015757c21b9a064f5d5371820bd71ac271`; task 9103 is in progress; prior independent preparation remains subject to current task acceptance, and admission/MCP/final acceptance remain incomplete. Full gates run at plan completion.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`42955b257c7a71bef27c91619e986eab1f853b54c21298f65c486860bffdfd12`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
