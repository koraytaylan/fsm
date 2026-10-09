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

Admission latest focused range `b373a453..4cac61bb` passes nine contract/service
cases, 25 mocked producer/verifier cases, feature-enabled executor clippy and
format/size checks, including unknown-refusal guard sensitivity; self-review
digest `c9f789207e48c7c0714449a06e13a8d070c6322abf7cf5c31f3b2a4f5e025962`.
This cache verdict indexes preceding guard, writer, cache and fairness reviews
and the archived detailed STATUS; those reviews retain their original scopes.
The current native inventory has sixteen unexecuted process/MCP axes and eight
protected Rust cases ignored locally; complete task 9103 acceptance and plan-end
gates remain outstanding, with journal-prefix reconstruction cost documented.

Recovery range `49e2c0c4..64ba1e25` verifies both production service writer
entries refuse an incompatible acknowledged outcome without mutation, then
repair and advance exactly once without selecting the acknowledged handler.
Ten admission and fifteen pipeline cases, executor feature-enabled all-target
clippy and format/size/diff checks pass; removing only the outcome guard makes
the named recovery case fail, and restored suites pass. Frozen verdict
`fdbaff3bd6b77b65c3a5335ebf6e2b19b657c533dd5e702e3c98419ef92ba9d3`
indexes the prior admission review; six provisioned cases remain ignored, and
unprovisioned recovery routing does not establish native side-effect acceptance.

Migration range `ad380abe..f2d81c29` exercises warmed service evidence through
both writer entries after receiver/context migration: incompatible outcomes
refuse without mutation, historical pending arguments survive, and corrected
configuration restores eligibility. Eleven admission and nine historical-effect
cases, executor feature-enabled all-target clippy and format/size/diff checks
pass; frozen review `34d1d6c54e2da29ec54357ff1e2758d5d4a106e8a879cb63bab71566546c6a6a`
indexes the recovery verdict; genuine native acceptance and task closure remain open.

Native argument preparation `607601d4..613a9d22` adds required-placeholder
refusal/repair for both writer entries and handler kinds. Eleven portable
admission and 25 mocked producer/verifier cases, feature-enabled executor
all-target clippy and format/size/diff checks pass; eight native tests remain
ignored locally, and the expanded sixteen-axis physical inventory is unexecuted.
Frozen review `6d255c84bfecbcbea00aa2e8bf0074803215bd40bf9771995c7d870b65386278`
indexes the prior migration verdict; task 9103 and plan-end acceptance remain open.

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
