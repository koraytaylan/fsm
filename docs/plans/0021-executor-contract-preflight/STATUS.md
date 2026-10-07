# Plan 0021 — Executor Contract Preflight — Unregistered

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file; task-level truth lives in [tasks/](tasks/) frontmatter, and the integration coordinator owns lifecycle changes.

- **Status:** Unregistered; independent effect-contract implementation started;
  effect analysis landed in `f0a489ad8d3738b6d9e497b6bbe04856a5249592`;
  outcome checks landed in `aa6fc9004bfc990885429a74fa5a27e1667ddfe2`;
  execution admission is not integrated.
- **Goal:** diagnose machine-handler incompatibilities before authoring completes and prevent incompatible workflows from starting external operations.
- **Root cause:** machine compilation and handler-table parsing validate separate inputs, while discovery and `--check` do not enforce their shared contract before spawn.
- **Approach:** one bounded structural analyzer, explicit manual policy and uncertainty, shared admission at the production execution boundary, and equivalent CLI/MCP reports verified with independent fixtures.
- **Progress:** 0/6 tasks done; 0 blocked; 0 dropped.
- **Integration:** not registered; no validation base, task landing OIDs, or integrated task completion exists yet; this bundle describes future work and must be committed before Phase R can bind its validation base.
- **Exceptions:** none.
- **Effect-policy progress:** optional bounded `manual_effects` now parses
  as an explicit operator disposition, including manual-only tables, with
  duplicate, overlap and malformed-name refusals. Independent tests cover
  the exact 256-name limit and limit-plus-one. The provisional public-surface
  inventory, SPEC, API policy and embedding guide move with this change.
  The pure effect-analysis API now walks nested entry/exit, transition,
  deadline and parallel sites, follows static invocation closure once per
  identity, uses authoritative compiled types, and emits bounded sanitized
  reports. Fourteen independent tests and a handwritten report golden pass on
  MSRV, covering every scalar type, manual policy, privacy, unknown/invalid
  precedence, shared children, signal boundaries and exact resource limits.
  Configuration and public-surface tests pass. The final stable workspace debug/release, Clippy and documentation gates
  pass on Linux; zero-dependency and embedding checks also pass. Native
  macOS/Windows execution is unexecuted. Configured outcomes remain explicitly unknown in the effect-only API;
  the complete API below validates them.
  No admission before spawn or task completion is claimed yet.
- **Analyzer self-review:** found that a later signal boundary could shorten
  the serialized scope by one byte and make a prefix reject an exact final
  report limit. Scope is now computed before site accounting, with an
  independent parallel-region regression. Limits also retain their hard
  ceilings when callers request larger values. Dense fixtures exercise 4096 sites
  and 512 findings; exact byte accounting serializes each row once and scans
  handler placeholders once, avoiding repeated prefix work. Full workspace
  review caught and repaired the error-registry count assertion (23 codes).
  Final Linux gates pass after discarding only disposable task build caches
  to recover temporary-storage quota. Frozen implementation review of
  `0e00d30313cbd419ceee36757b65d9073562a277..f0a489ad8d3738b6d9e497b6bbe04856a5249592`
  is complete with these repairs and no outstanding effect-analysis finding.
  Task registration/integration, admission and remaining native
  platform evidence are still pending.
- **Outcome-validation progress:** complete `analyze_contract` API landed
  independently in `aa6fc9004bfc990885429a74fa5a27e1667ddfe2` with ten independent matrix/golden tests passing on MSRV.
  Shared traversal suppresses temporary out-of-scope findings; static outcome
  payload validation is cached per definition, handler and outcome. A dense
  2560-site fixture passes with a zero-finding budget. Task 9102 footprint
  includes the private traversal change in `effects.rs` for bounded reuse.
  Final stable workspace debug/release, Clippy and documentation gates pass
  on Linux. MSRV targeted checks pass (10 outcome, 14 effect, 35 config and
  16 API tests); full workspace gates include embedding and zero-dependency
  checks. Frozen review of
  `f0a489ad8d3738b6d9e497b6bbe04856a5249592..aa6fc9004bfc990885429a74fa5a27e1667ddfe2`
  is complete. Review repairs eliminated temporary placeholder accounting
  and repeated static payload validation; exact limit and dense-site fixtures
  prove the repairs. Native macOS/Windows checks remain unexecuted.
- **CLI-check progress:** task 9201 landed independently in
  `04dbb17fe615edc05745998022af62f9c18a60f7`. Seven
  real-binary tests pass on MSRV, including three handwritten report goldens and a usage-error golden,
  held-writer byte invariance, offline inaccessible/nonexistent data paths,
  unresolved and invalid children, distinct exits, private-table redaction, stdin mode refusals and manual policy.
  Existing execute tests pass (15 cases). Final stable workspace debug/release,
  Clippy and docs gates pass on Linux, with the final seven CLI checks also
  passing on stable debug/release. Release validation required disposable
  release-cache cleanup after temporary-storage quota exhaustion. Frozen
  review of `1f97188e11d0311ab300e143991af5c4588acb5e..04dbb17fe615edc05745998022af62f9c18a60f7`
  is complete. Review distinguishes writer-drop snapshot writes from check
  writes, forces file selectors to be file reads, and verifies sanitized
  errors and direct common-analyzer output. Native macOS/Windows remains
  unexecuted; shared admission and MCP draft checking remain unfinished.
- **Outcome:** effect and outcome analysis implemented and reviewed; complete contract admission remains unfinished; completion requires the task acceptance evidence and the stable host and relevant platform gates, with unexecuted environments recorded.

_Task frontmatter is authoritative; this file is the roll-up._

### Historical emitting-definition context — 2026-10-07

PendingEffect now retains emitting_machine_id reconstructed from the same
verified pre-emission prefix used to recover its actual arguments: creation
and invocation use their recorded definition identities, and transition and
deadline emissions use the historical instance definition. Current migrated
instance state does not replace that context. The independent migration case
creates a pending entry effect, migrates to a distinct definition, reopens
read-only, and checks the original identity and concrete args without writes.
SPEC, API policy, embedding guidance, release notes and the provisional Rust
surface inventory move with the added public struct field; downstream literal
construction requires adaptation and the pre-1.0 minor consequence is explicit.
This supplies task 9103's historical evidence without integrating admission,
persisting authorization, changing journal/hash bytes, or completing a task;
focused verification and guard sensitivity remain pending.
