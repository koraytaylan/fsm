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

### Emitting-record coverage and migration fixture correction — 2026-10-07

Commit `835befea` adds independent identity assertions for creation, transition,
deadline and invoked-child emissions through resolve and Watcher; child
identity is compared with the authored child definition, not its parent.
The first frozen verification session 45085 at `376ff605` is terminal exit 101:
the migration fixture was refused before reconstruction with
`def/supersedes_target_terminal` because its supersedes mapping targeted the
terminal `closed` state. This was a fixture error, not a passing regression or
an implementation change; the original failed log remains retained.
Commit `1187b05f` maps both historical leaves to continuing `intake`, following
SPEC's terminal-target restriction; active pending work and original args
remain the property under test. Corrected session 96331 now verifies frozen
`1187b05f` in the same isolated worktree after the original run was confirmed
terminal, using stable/MSRV executor unit and integration tests, all-target
Clippy and a current-definition substitution that must fail the migration
regression. The original log is historical-effect-definition-check.log and the
new run uses historical-effect-definition-corrected-check.log in the task
cache; results remain pending and no runtime admission or task completion is
claimed.

### Terminal historical-definition reconstruction verification — 2026-10-07

Corrected session 96331 exits zero at frozen `1187b05f`: stable and Rust 1.89
executor unit/all integration targets and all-target Clippy pass, including
all nine effect-resolution cases, composition identity assertions and the
provisional public-surface inventory. Formatting and file-size checks pass.
The isolated mutation replacing the original emitting prefix identity with
the current instance definition fails exactly the migration/reopen regression
(0 passed, 1 failed, no compiler failure); exact source restoration then passes
all nine resolution cases and leaves the frozen source clean. The corrected
log SHA-256 is
`d22690c794a68964a2096e3fc82c7e4f0beabf4a3d9fcb102b566ccd7af224eb`;
the sensitivity log SHA-256 is
`b9f67e986e8beb6c188100fe40bd9008002c487ff4085f1438bb29f77588362a`.
Every stage validates actual 1 GiB/zero-swap kernel limits and serial Cargo
ownership, and the complete committed diff range is clean. These are focused
Linux checks, not a new eight-stage workspace gate or native macOS/Windows
acceptance; ignored native fixtures do not count as execution. This verifies
the historical-definition prerequisite only: shared admission before external
starts, the migration/current-outcome matrix and task completion remain open.

### Borrowed definitions for writer-state admission — 2026-10-07

Commits `8a7dbba6` and `3ce50e11` introduce private borrowed-definition resolver
paths under both analyzers while keeping public owned-catalogue signatures and
report semantics unchanged; the future admission component can use immutable
verified Store definitions without constructing a copied compiled catalogue.
Self-review preserves the distinction between invocation-digest keys and full
compiled definition identities: arbitrary caller catalogue keys must not
substitute another outcome definition. Commit `419f8be2` adds an independent
public-entry regression with a parent invoking a string-payload child and an
extraneous full-identity key pointing at a boolean-payload definition.

Session 2145 verifies frozen `419f8be2` on stable/MSRV effect/outcome matrices,
public-surface inventory and all-target executor Clippy, then substitutes the
incorrect key-first lookup to require the new regression to fail before exact
restoration; borrowed-contract-resolver-check.log and the sensitivity log are
retained in the task cache. The original historical-definition run is already
terminal and restored before this source advance. Results remain pending;
this is isolated internal preparation for task 9103, with no changed persisted
bytes, public signature, spawn authorization, runtime admission guarantee or
task completion. Integration footprint needs the two private analyzer modules
and the outcome regression in addition to task 9103's original admission paths.

Session 2145 is now terminal exit zero: stable and Rust 1.89 effect/outcome
matrices, public-surface inventory, all-target executor Clippy, formatting and
file-size checks pass at frozen `419f8be2`. All eleven outcome cases pass,
including the independent extraneous-key regression; substituting key-first
lookup fails exactly that public-entry case (0 passed, 1 failed, no compiler
failure), and restored source passes all eleven cases. The main log SHA-256 is
`543cf7f231639ac923ad667d606a8511af267a7489f935f2e6e2cfe9a31d9c61`;
the sensitivity log SHA-256 is
`fe2e02879200fc330ebfb25103d2b5cacdbc1667384259a5736241d44bd438a3`.
These focused Linux checks preserve analyzer behavior and do not replace a
full workspace gate or native platform acceptance; service admission is still
unwired and task completion remains 0/6.

### Shared pending-contract evidence component — 2026-10-07

Commit `cae7af9a` adds provisional `contract::check_pending`, borrowing verified
Store definitions to check running membership, historical emission identity,
reconstructed concrete arguments, the current executable definition closure
and the configured pending outcomes against the current receiving definition.
It uses existing typed contract diagnostics and performs no journal mutation,
attempt consumption or request-key allocation; SPEC, API policy, embedding,
release notes and public-surface inventory move with this API addition.
Three independent public-entry cases cover compatible read-only checking,
an incompatible later restore followed by repair, and forged concrete args,
with complete Store-state invariance assertions. Commit `03187966` classifies
unavailable effect reconstruction as unknown evidence rather than forwarding
an unrelated resolution-domain error.

Session 13715 verifies frozen `03187966` on stable/MSRV admission, effect,
outcome and public-surface targets plus all-target executor Clippy, then
neutralizes only the concrete-effect equality guard to require the substituted
argument case to fail before restoration. The log is
pending-contract-evidence-check.log with pending-contract-evidence-sensitivity.log
in the task cache; verification is queued behind unrelated Cargo work using
the existing serial 1 GiB/zero-swap scope. This component is not wired to
service dispatch yet, so no before-spawn guarantee, native marker acceptance,
admission-cache completion or task completion is claimed.

### Manual-policy review and API inventory repair — 2026-10-07

Review found that the pending checker required an automatic handler even for
an explicitly manual effect; commit `f4e290b0` preserves manual compatibility
without acknowledging work or granting spawn permission, with an independent
pending/outbox and complete Store-state invariant case and matching normative
documentation. Initial session 13715 at frozen `03187966` is terminal exit 101:
the new checker cases reached execution, but the independent public-surface
scanner found the intended check_pending re-export missing from its inventory.
Commit `0fc018ea` adds that exact re-export entry; the original failure log is
retained and not reported as a pass. Corrected session 44579 verifies frozen
`0fc018ea` only after the original run retired, including all four pending
checker cases, existing analyzer matrices, public inventory and stable/MSRV
all-target Clippy, followed by concrete-argument guard neutralization and
restoration under serial 1 GiB/zero-swap limits. The corrected log is
pending-contract-evidence-corrected-check.log in the task cache; results are
pending and production dispatch wiring remains unfinished.

### First shared production preparation check — 2026-10-07

Commit `b0a97c49` calls the pending checker at Runner's actual native queue
entry before backend authority lookup or a preparation reservation. Both
shared tick entry points use this boundary; service refusal releases the
unclaimed scheduler slot and preserves pending work without journal writes.
The independent queue-entry case uses a valid first effect and an invalid
later restore, requires the typed contract refusal before backend capability
refusal, and asserts complete Store/record invariance plus zero preparation
inventory. This fixture proves preparation wiring, not Root closure or an
external marker scenario; actual native provisioning remains unavailable.

Session 59714 waits for corrected checker session 44579's original live PID
2206291 to retire successfully before advancing the same isolated source to
frozen `b0a97c49`; it runs stable/MSRV executor unit/integration targets and
all-target Clippy, then removes only the actual queue contract call to require
the named typed-refusal fixture to fail before restoration. Its log is
native-contract-preparation-check.log and sensitivity log is
native-contract-preparation-sensitivity.log in the task cache, using serial
one-worker and actual 1 GiB/zero-swap limits. Results are pending: final
writer-held claim checks, bound-entry checks, cache invalidation, acknowledged
outcome recovery and the full provisioned side-effect inventory remain open.
The integration footprint additionally requires run/native_host.rs for this
first service hook; no full admission guarantee or task completion is claimed.
