# Plan 0022 — Executor Process Lifecycle — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and the integration coordinator owns
lifecycle updates.

- **Status:** Registered by hand; native prerequisite complete;
  durable-claim primitives are being implemented.
- **Goal:** prevent a successor from overlapping a surviving local handler
  tree, with bounded shutdown and evidence-based restart for both process and
  MCP handlers.
- **Root cause:** direct-child handles and short-lived writer locks do not
  contain descendants or persist execution ownership across executor death.
- **Approach:** resolve the native containment prerequisite, journal claims
  before launch, prove tree closure before reuse, and drive every execution
  host through the same shutdown and recovery protocol.
- **Progress:** 1/7 tasks done; 0 blocked; 0 dropped; durable claims In progress.
- **Integration:** Phase R bound by hand on `develop` to validation base
  `8e3a8bbaed670ee9b3d7f354142090c9abc1d78d`; mode `by-hand`, following
  plan 0019's recorded execution mode. Committed scope identity, closed task
  frontmatter, task IDs/titles/filenames/workstreams, ordered steps, unique
  mutation footprints and all six dependency edges validate as one acyclic
  local DAG. No manifest owner requires footprint adoption. This is a manual
  coordinator binding, not a claimed Makina invocation. Task 9301 lands at
  `399ed6ed5636118151ffb7ad94140538f865d1a7`; its native gate is released by
  the terminal nine-job CI verdict and reviewed native evidence below.
  Final production integration remains incomplete.
- **Exceptions:** task `lifecycle-containment-feasibility` requires a proved
  charter-compatible backend or an explicit project decision before dependent
  implementation; durable fail-closed behavior alone cannot complete this plan.
- **Feasibility evidence:** independent probe/decision commit
  `4206e8e27c853741d9f2a290090f0c8ed8edf7a3` retains the unreleased gate. [lifecycle decision record](../../EXECUTOR-LIFECYCLE.md)
  and `crates/fsm-execute/tests/lifecycle_platform.rs` demonstrate surviving
  descendants and retained pipes after root kill and normal root exit on
  Linux, at Rust 1.89.0 and stable 1.98.1. These are negative probes, not
  positive containment acceptance. Delegated user authority now selects a
  provisioned Linux/systemd backend
  for the initial contained executor, preserving safe Rust, zero crates and
  MSRV 1.89; see the decision record. A protected dynamic-UID host experiment
  contained an orphan descendant and closed its pipe on unit stop. The
  reproducible compiled-Rust driver passes four partial native cases at MSRV
  and stable: root exit, root kill, changed process group and launcher death,
  with protected UID/membership checks and unrelated-process survival. Full
  native proof and gate release remain pending; no task is marked done or
  given a landing OID.
- **Protected native review:** decision/probes landed in `3f397f5`, with
  failed-unit collection repair in `dd97260`. Frozen range
  `69ad2aa3b12080407314c8be86bac741ac4cbc68..dd9726081b86e06b41d0c08295c06fac71b8d35f`
  passes both toolchain probes from a clean checkout. Retained reports are
  `/tmp/fsm-systemd-final-msrv.json` and `/tmp/fsm-systemd-final-stable.json`:
  exact source, dirty false, four cases each, gate unreleased. Review separates
  failed service outcomes from actual domain/pipe closure and collects stopped
  probe records; no task-owned units remain. Formatting, all-targets Clippy,
  warning-free docs, zero-dependency, embedding and full stable debug/release
  workspace gates pass. MSRV negative probes and all 45 Python harness tests
  also pass. Native macOS/Windows remain unexecuted. Permanent identity,
  journal-bound launch authorization, spawn during stop, privileged supervisor
  recovery, signals and uncertainty refusal still need positive native proof.
- **Shutdown-race preparation:** the native probe now additionally proves
  two descendants can fork after a real stop job reaches deactivating and
  remain contained until final killing. A separate kernel freeze/kill case
  observes frozen=1 and populated=1 before arming a fork, then requires no
  leaf marker, domain removal and bounded pipe EOF. All six positive cases
  pass at MSRV and stable. A separately labelled SendSIGKILL=no mutation
  fails with an observed populated domain; emergency fixture cleanup cannot
  turn that failed result into a pass and leaves unrelated processes alive.
  UID/effective-capability/no-new-privilege assertions cover root, initial
  descendant and the late descendants. These remain partial native proofs;
  permanent identity, privileged supervisor recovery, atomic authorization,
  signal notification and uncertain replacement refusal are pending.
  Landed independently in `8b2dd03`; frozen review `3be7292..8b2dd03`
  is complete. Clean reports `/tmp/fsm-systemd-race-clean-msrv.json` and
  `/tmp/fsm-systemd-race-clean-stable.json` retain six positives per toolchain;
  the corresponding `clean-negative-msrv`/`clean-negative-stable` reports
  retain the expected live-domain failure. Exact source, dirty false and gate
  unreleased are verified. All required stable Linux workspace gates,
  existing MSRV negative probes and 45 Python harness tests pass. No probe
  units remain; native macOS/Windows remain unexecuted.
- **Native identity preparation:** a private root-owned prototype records
  empty-domain inode/boot identity and fsyncs monotonic counters, checks full
  supplied handles, retains closed tombstones and refuses unknown/aliased
  domains or lost authority before launch. Ten real native cases pass at
  MSRV and stable, including a killed privileged controller after utility
  handoff, preserved active ownership and subsequent matched-domain cleanup.
  An unrelated same-name replacement unit survives stale-handle refusal.
  This is a task-9301 feasibility prototype, not production broker/admission
  or journal implementation; queued launch cancellation, every crash window,
  cross-boot persistence, authenticated bounded IPC and signal notification
  remain unproved. The full gate is unreleased. Landed independently in
  `8f57dc3`; frozen review `5f3b5ce..8f57dc3` is complete. Clean reports
  `/tmp/fsm-native-identity-clean-msrv.json` and
  `/tmp/fsm-native-identity-clean-stable.json` pass ten identity cases each;
  their `clean-containment-msrv`/`clean-containment-stable` companions pass
  the six existing containment cases. Exact source and clean status are
  verified. Review requires intended refusal diagnostics and unchanged
  protected state, rejects missing authority, and retains full identity on
  idempotent close. All required Linux workspace gates, MSRV negative probes
  and 45 Python harness tests pass. No probe units remain; native
  macOS/Windows remain unexecuted.
- **Private privilege/entry preparation:** independent commit `f95b4e3`
  adds a fixed bounded Unix-socket protocol, operator-only access and
  root-owned launch grants checked against actual kernel enrollment before
  handler fixture work. Frozen review `f545be7..f95b4e3` checks closure
  revocation ordering and bounded utility reap polling. Exact clean reports
  `/tmp/fsm-native-broker-clean-msrv.json` and
  `/tmp/fsm-native-broker-clean-stable.json` pass nine broker cases each;
  companion `clean-identity-*` and `clean-containment-*` reports pass ten
  identity and six containment cases per toolchain. All identify `f95b4e3`,
  dirty false and gate unreleased. Copied armed grants in other domains and
  submissions replayed after closure are refused with unchanged protected
  authority. Removing only the entry gate in the isolated checkout makes
  native enrollment proof fail at both toolchains; separately labelled
  `/tmp/fsm-native-broker-neutralized-msrv.json` and
  `/tmp/fsm-native-broker-neutralized-stable.json` retain that expected failure,
  never a passing backend verdict. No task-owned units remain. Full queued-job cancellation,
  every handoff/closure crash window, journal-bound authorization, restart,
  backend-authority loss and signal notification remain pending. This is
  private feasibility preparation, not a production broker or task landing.
  Full stable Linux debug/release workspace tests, formatting/file-size,
  all-targets Clippy, warning-free docs, zero-dependency and embedding gates
  pass. Final frozen lifecycle tests pass at MSRV and stable; all 45 Python
  harness tests pass. Native macOS/Windows remain unexecuted. No public
  journal/API/hash bytes change, so prior-format migration is not exercised
  by this private probe-only range.
- **Pending-job/closure review:** independent proof commits `3f792fd`,
  `eb06aeb` and `6889589` are frozen and reviewed in `81aab9a..6889589`.
  Eleven native window cases pass at MSRV and stable, including a genuine
  pending manager job retained after controller death, its cancellation
  before handler work, interrupted revocation and live-tree finalization
  recovered only from a matching protected closure receipt. Missing or
  mismatched receipts and unrelated aliases refuse recovery. The stronger
  live-tree case required fresh fixture output files for each DynamicUser;
  the owned work directory is rebuilt only after verified domain closure.
  Exact clean reports `/tmp/fsm-native-window-clean-msrv.json` and
  `/tmp/fsm-native-window-clean-stable.json` retain eleven cases each; their
  `clean-broker-*`, `clean-identity-*` and `clean-containment-*` companions
  retain nine, ten and six respectively. All identify `6889589`, dirty false
  and gate unreleased. Removing only the receipt inode check makes the
  mismatch case fail at both toolchains; `neutralized-msrv` and
  `neutralized-stable` reports retain expected negative failures. All required
  stable Linux host gates, final MSRV/stable lifecycle tests and 45 Python
  harness tests pass. No task-owned units remain; native macOS/Windows remain
  unexecuted. The gap before receipt persistence still refuses missing native
  state, and full signal/authority-loss/restart/window proof remains pending.
  Production journal/contract authorization belongs to downstream 9302/9303,
  rather than a circular prerequisite for the native feasibility task.
- **Socket/signal review:** independent commit `ce3d163` is frozen and
  reviewed in `776f02a..ce3d163`. A permissive creation mask is rejected before
  privileged socket publication, and explicit UMask=0077 preserves operator-only
  access. Two native signal cases prove SIGTERM and SIGKILL reach a surviving
  broker as EOF and close the handler domain within two seconds while an
  exec'd client descendant remains alive in a separate domain. This proves
  notification and verified closure, not production graceful joining.
  Exact clean reports `/tmp/fsm-native-signal-clean-msrv.json` and
  `/tmp/fsm-native-signal-clean-stable.json` retain those two cases; their
  `clean-broker-*`, `clean-window-*`, `clean-identity-*` and
  `clean-containment-*` companions retain ten, eleven, ten and six per
  toolchain. All identify `ce3d163`, dirty false and gate unreleased.
  Separately neutralizing the mask and EOF guards makes the corresponding
  native proof fail at both toolchains; `negative-mask-*` and `negative-eof-*`
  reports retain expected negative failures. The isolated checkout is restored
  and no task-owned units remain. Required stable Linux host gates, final
  MSRV/stable lifecycle tests and 45 Python harness tests pass. Native
  macOS/Windows remain unexecuted. Broker restart, authority loss, other crash
  windows and production shutdown remain pending; no task is marked done.
- **Supervisor restart review:** independent commits `5f5125a` and
  `75dceda` are frozen and reviewed in `7824c4b..75dceda`. Ten native cases
  prove lifetime exclusive broker authority, monotonic endpoint epochs,
  preserved live claims after broker death, refusal of overlapping allocation,
  survival of an unrelated old-endpoint listener, matched cleanup, missing
  epoch-authority refusal, rollback refusal and succession. Clean review
  exposed a readable-route publication race; public permissions now precede
  fsync and rename, and an unprivileged reader acknowledges all 64 complete
  publications. The initial failed report is retained rather than relabelled.
  Exact final reports `/tmp/fsm-native-restart-final-msrv.json` and
  `/tmp/fsm-native-restart-final-stable.json` pass ten restart/publication
  cases each; `final-signal-*`, `final-broker-*`, `final-window-*`,
  `final-identity-*` and `final-containment-*` companions pass two, ten,
  eleven, ten and six. All identify `75dceda`, dirty false and gate unreleased.
  Separately neutralizing lifetime authority, rollback and publication ordering
  fails the intended native cases at both toolchains; `negative-lock-*`,
  `negative-rollback-*` and `negative-publication-*` reports retain those
  expected failures. Required stable Linux host gates, final MSRV/stable
  lifecycle tests and 45 Python harness tests pass. The isolated checkout is
  restored; no task-owned units remain; native macOS/Windows remain unexecuted.
  Directory replacement, other authority loss, stale-socket storage budgets,
  remaining windows and production integration still need completion.
- **Directory and facility review:** frozen range `0fbff45..9c4abdd`
  passes twelve restart cases on MSRV and stable with exact clean source
  reports; neutralizing either directory identity check fails its intended
  native case on both toolchains. Required stable host gates and 45 Python
  tests pass; prebuilt canonical stable regression evidence passes 33 signal,
  broker, window and identity cases. Further builds are deferred under the
  repository high-swap rule. A new facility probe passes five exploratory
  cases with the existing canonical stable fixture: blocked manager access
  refuses launch and successor allocation, matched native closure succeeds,
  and read-only cgroups refuse allocation while burning identities. Frozen
  MSRV/stable facility evidence remains pending; host manager death,
  whole-namespace loss, storage budgets, remaining windows and production
  integration remain incomplete. Task status and native gate do not change.
- **Aggregate native review:** `native_matrix.py` now binds all seven
  exact native inventories to one clean source/compiler, validates unrelated
  survival and requires the final-kill negative control to fail with a live
  domain. Frozen local source `3f9f0d1` passes 56 cases and its expected
  negative at each compiler; frozen facility source `809978f` passes five
  cases each. Separate review branch `codex/plans-20-23-native-review`
  executes the actual CI matrices. Run `37241901509` failed workflow
  validation before jobs; `bac926e` run `37241946083` exposed unsupported-host
  unreachable returns and a protected-path Python permission assertion.
  Repairs `bac926e` and `f1ccee1` preserve those failed reports. The latter
  source passes all 56 native cases and the expected negative on Ubuntu
  kernel 6.17/systemd 255 at Rust 1.89 and stable 1.99, in run `37242072483`.
  Candidate `6852cf3` adds six-mode unsupported-host refusal with no fixture
  writes; both its native jobs also pass in run `37242284024`. The complete
  six-leg portable gates are still running; source-bound artifact review and
  final host gates remain in progress. No task or gate is marked complete.
- **Authority-loss review finding:** directory identity did not cover losing
  the active pointer in place or reprovisioning private records plus the
  public route while the old domain survives. A bounded native namespace
  inventory now refuses unknown domains before startup epoch or allocation
  counter mutation. New restart cases exercise both losses and restoration;
  frozen review, guard neutralization and updated native CI remain pending.
- **Frozen native and I/O checkpoint:** source
  `46d0db24df7c5f108cdf8631259a3b81c4023a01` passes the 61-case matrix
  locally on MSRV and stable and in both native jobs of CI run
  `37244150329`; downloaded artifacts bind that exact clean source and all
  suite, I/O-log and final-kill-negative SHA-256 digests verify. The inventory
  includes sixteen restart cases and a real blocked socket reader whose
  cancellation joins within 250 ms while the descendant and outside writer
  survive. Removing only the candidate read shutdown fails the intended
  bounded-join assertion on both compilers; startup, allocation and canonical
  namespace guard negative controls likewise fail their intended cases.
  These are primitive feasibility proofs, not production lifecycle claims.
  The MSRV CLI retry passes seven tests after a disk-quota build failure;
  the required stable host gate and six portable CI legs remain running.
  Earlier run `37242284024` passed both macOS legs but failed both Windows
  legs on reading the held journal lock; `fedfc47` repairs those test
  snapshots, with actual Windows validation still pending in the current
  run. Task status and native gate remain unchanged.
- **Portable review repair:** all required local stable host gates pass at
  frozen source `46d0db2`, including debug/release workspace, all-target
  Clippy, warning-free rustdoc, zero dependencies and external embedding.
  Both Ubuntu CI gates pass. The macOS stable release job of run
  `37244150329` fails the embedded tick test when reopening a freshly released
  setup writer during a parallel fixture's fork window; its completed job
  log is retained. Repair `399ed6e` uses the existing bounded reopen helper
  and explicitly asserts refusal of a competing writer while the embedded
  writer is held. All four targeted release tick tests pass locally.
  Production locking is unchanged. Fresh exact-source CI run `37245431479`
  and the serialized stable host gate are active; no old or unfinished job
  is counted as proof for the repair, and the prerequisite remains unreleased.
- **Repaired-source host verdict:** the serialized stable gate at clean
  `399ed6ed5636118151ffb7ad94140538f865d1a7` completes successfully:
  formatting, file-size checks, complete debug/release workspace tests,
  all-target Clippy, warning-free rustdoc, zero dependencies and external
  embedding. Both native jobs of run `37245431479` also pass all 61 cases;
  downloaded source bindings and every retained report/log digest verify.
  All six portable jobs remain live at this checkpoint. No task completion
  or native prerequisite release is inferred from their unfinished state.
- **Prerequisite closure:** run `37245431479` completes successfully at exact
  clean source `399ed6ed5636118151ffb7ad94140538f865d1a7`: both 61-case
  Linux native jobs, all six executed portable stable/MSRV gates and the
  separate zero-dependency job pass. The frozen range passes diff checks;
  local full stable gates and native negative controls are recorded above.
  The accepted initial platform is provisioned Linux/systemd; macOS/Windows
  contained capability remains excluded, with executed refusal tests and
  existing portable functionality validated. The separately recorded signal
  decision promises explicit drain/abort control and fail-closed signal-death
  recovery, not graceful SIGTERM. Coordinator release completes task 9301
  and makes task 9302 Ready. Private reports remain unchanged with
  `gate_released: false`; they are evidence, not coordinator authorization.
  Production claims, journal binding and supervisor installation remain the
  downstream tasks' requirements; the plan and thread goal remain incomplete.
- **Outcome:** native prerequisite complete; production lifecycle pending.

- **Durable-claim implementation:** SPEC now reserves the claim/stopped/single-
  record settlement schemas, durable retry eligibility, preserved run counter,
  explicit execution quarantine, new root/snapshot/base formats and exact
  accounting bounds before implementation. API-POLICY distinguishes this
  reserved contract from the still-shipped VERSION 10 behavior and records
  the breaking-minor consequence. Production APIs, error registration,
  format migration and crash/seal/boundary tests remain in progress.
  Pure native identity and retry-policy constructors/closed decoders now
  implement the specified value shapes without I/O, record kinds or persisted
  format changes. Initial MSRV boundary/canonical/arithmetic tests and the
  external construction test pass. Frozen range `522df7c..0ffdca6` passes the
  complete stable host gate; review-branch CI run `37248754934` has both native
  jobs and zero dependencies passing, with its six portable jobs still live.
  Production claim-state folding remains pending.

- **Pure ownership implementation:** claim, stop and single-consumption
  settlement transitions and closed state codecs are implemented as core
  primitives; pending-effect truth and native receipt authentication remain
  explicit caller obligations. Independent recovery, stale-run, retry-policy,
  interruption, cancellation and exact count/byte fixtures are added, with
  downstream API construction acceptance. All 16 ownership fixtures and five
  value fixtures pass on MSRV; downstream use passes on both toolchains, and
  stable all-targets core/embed Clippy passes in an isolated target directory.
  The complete frozen host/portable gates and guard-negative review remain
  pending for this unit. No production record kinds or VERSION 11
  store integration have landed, and task 9302 remains In progress.

- **Ownership review follow-up:** direct fixtures now also reach quarantined
  admission with a pending effect, mismatched full claim identity and a new
  claim against an exactly-full state block; all 16 stable ownership tests pass.
  These close coverage gaps found while choosing individual guard mutations;
  final frozen review uses the follow-up commit rather than the earlier code
  head alone, and production store wiring remains incomplete.

- **Frozen ownership guard review:** range `e03ad45..39ec8c6` has a clean
  MSRV baseline and 15 isolated negative controls, each failing its named
  production-facing pure API fixture when only the selected guard is removed.
  The controls cover quarantine, unresolved ownership, next-run allocation,
  retry deadline, repeated stopping, native domain/run binding, full claim
  binding, entry/metadata/outcome limits, decoder/transition block budgets,
  an independent stopped-result budget call site, and disposition selection.
  Every captured log digest is verified and the disposable clone is restored
  clean. Full stable host gate `39ec8c6-host-gates.log` completed successfully in
  an isolated target directory; exact-source review CI `37249663619` at
  `39ec8c67d4b7c5777192c36cee7d9c3138a169d0` is terminal success across
  all nine jobs, including Linux/macOS/Windows at stable/MSRV, zero dependencies
  and both provisioned native jobs on the authorized review branch. These are core primitive proofs, not store crash/migration/seal
  or native receipt authentication acceptance; task 9302 remains In progress.

- **Production persistence design:** the reserved contract now makes genesis
  admission, legacy base/1 quarantine, migration ordering, crash-stable
  quarantine reconstruction, recorded root discriminators and request replay
  explicit before VERSION 11 implementation. The exact frozen ownership CI
  handle is `37249663619` at `39ec8c6`; it validates ownership primitives,
  not the subsequent production persistence draft.

- **Production persistence implementation in progress:** the implementation unit adds
  all four claim-era record kinds and pure fold, VERSION 11 with unchanged
  historical journal bytes, root/4, snapshot/6 and authoritative base/2 with
  explicit root/3/base/1 readers. Independent base/2 and snapshot/6 fixtures
  pass alongside unchanged historical root/base/snapshot/archive goldens.
  Production allocation, stop, settlement and verified legacy-enable APIs are
  written with projection-before-append and cold request replay; protected
  receipt readers are opaque, bounded and confined to the recorded root
  authority directory. A separately sealed claim-hash root avoids a root
  self-reference and retains original unresolved claim hashes through two
  seals. MSRV tests prove the two-writer allocation race, stale/removed effect
  refusal, cold claim replay, read-only refusal, repeated seals, every VERSION
  1–10 migration, failed settlement and exact retry eligibility. Transition
  tests using preauthenticated fixture proofs do not establish native receipt
  authentication. The preliminary complete stable workspace gate completed
  with ten failing targets; affected format/fixture checks were repaired and
  focused reruns pass, including isolated-Git regeneration tests, nine
  production claim tests, thirteen historical/current seal tests and all-target
  Clippy. The raw golden input capture supports independent historical-chain
  validation before hash updates; the new audit root was independently derived
  from unchanged SPEC state material. Native authority publication, all durable
  fault boundaries, legacy replay guard proof, full limits/negative controls
  and the final frozen full host/portable review remain outstanding.
  The coordinator adopts the specific CLI audit/transcript/hygiene/snapshot
  tests and raw input-capture helper into task 9302's mutation footprint; these
  are format integration checks and no active manifest owner requires adoption.
  Task count and dependency edges are unchanged. No task status or landing OID
  is advanced by these partial checks.

- **Frozen persistence review:** implementation commit `b482fe0` is frozen in
  a clean detached checkout with a checkout-specific Cargo target; its full
  stable host gate completed successfully: formatting, file size, debug and
  release workspace suites, all-target Clippy, warning-free rustdoc, zero
  dependencies and external embed acceptance all passed in sequence.
  Review CI run `37254257934` is bound to
  `b482fe03d9bedc0626d8a4e8ef06f82a005840cc` on the authorized review branch;
  both provisioned native jobs and zero-dependencies completed successfully,
  while all six portable legs are still running. Both native artifacts were
  retained and independently checked against that exact source, all seven
  suite-report digests, the I/O-cancellation log and expected failing final-kill
  control: each toolchain proves the existing 61 native prerequisite cases,
  not production receipt authentication. Follow-up tests
  now model empty, partial and complete production-generated claim appends
  and semantically invalid claims with recomputed chain hashes. Additional
  production cases exercise a claim at record 10,000 with a root-bound snapshot
  and cold replay, plus exact 4 KiB UTF-8 request IDs and limit-plus-one refusal
  without allocation; all thirteen production integration cases passed at
  `1364405` on stable and MSRV, and all-target Clippy passed on that source.
  These cases do not prove native
  launch/authentication or every durability fault boundary. Task 9302 remains
  In progress with no landing OID or task-count change.

- **Production guard negative controls:** an isolated `1364405` checkout
  passed thirteen integration and four transition baselines before separately
  disabling stale-head refusal, preservation of legacy failed counts, original
  claim-hash receipt binding and request UTF-8 byte limits. Every mutation
  produced exit 101 with its named runtime assertion failure, never a compiler
  failure; the restored checkout again passed all thirteen plus four cases.
  Source restoration and every retained log digest were checked. An initial
  incorrect module filter was rejected for running no tests and corrected
  before accepting this evidence. These four controls supplement the earlier
  fifteen ownership primitive controls; full production entry/metadata/outcome/
  aggregate limits, native receipt publication and remaining durability faults
  are still required before task 9302 can finish.

- **Production claim metadata boundary:** a new integration case derives
  canonical accounting from the seven claim fields, creates a real pending
  effect with a correspondingly long instance identity, and admits exactly
  4,096 metadata bytes through the production API. A policy differing by one
  canonical byte is measured at 4,097 and refused before append, request-ID
  allocation or run allocation; the exact claim survives snapshot selection
  and cold reopen. All fourteen integration cases passed on stable and MSRV;
  the final explicit exact/plus-one accounting assertion passed separately on
  both toolchains, and all-target Clippy is green. This covers producer metadata
  admission and exact-size restoration; hostile-loader plus-one controls,
  production entry/outcome/aggregate bounds and native receipt authentication
  remain outstanding, so task 9302 stays In progress.

- **Hostile metadata loading:** the exact metadata case now replaces the
  persisted claim with its measured 4,097-byte variant and recomputes the
  snapshot checksum or journal chain hash to isolate bounded decoding from
  ordinary hash damage. Direct snapshot decoding refuses the `bytes` field;
  read-only store loading skips that hostile cache, recovers the original
  journal claim and leaves cache bytes unchanged. Both read-only and writer
  opens refuse the oversized journal claim without changing journal bytes;
  restoring the original bytes restores ownership. The expanded case passes
  on stable and MSRV, and all-target Clippy passes. Authoritative-base plus-one
  controls and the other production resource bounds remain outstanding.
  CI run `37254257934` now has successful Linux stable/MSRV legs alongside both
  native jobs and zero-dependencies; both macOS and Windows toolchains remain
  live, so the portable matrix is not yet accepted and task 9302 remains
  In progress.

- **Authoritative-base metadata bound:** the metadata case now cancels the
  instance while retaining its unresolved exact-size claim, creates a real
  seal/archive, verifies that archive, and loads base/2 with the 4,096-byte
  claim intact. Replacing only the persisted retry policy produces the measured
  4,097-byte metadata variant; direct base decoding refuses `bytes` before
  root comparison, and both read-only and writer opens refuse the hostile
  authoritative base without changing its bytes or falling back to a cache.
  Restoring the original base restores sealed ownership. The expanded test
  passes on stable and MSRV, and all-target Clippy passes. Production entry,
  outcome and aggregate bounds, the remaining durability faults and native
  receipt publication/authentication are still required; task 9302 remains
  In progress.

- **Stopped outcome byte budget:** a production-transition case accounts for
  a six-byte escaped control character and a two-byte UTF-8 glyph, accepts
  exactly 65,536 canonical outcome bytes, and rejects 65,537 at the public
  bounded constructor. With an explicitly preauthenticated fixture proof, the
  exact outcome is published through `stop_execution_on`, retains exclusive
  ownership, agrees with full replay, and survives snapshot/6 and base/2 codec
  round trips. Snapshot decoding with a recomputed checksum and authoritative
  base decoding both reject the oversized outcome at `bytes`; a rehashed
  stopped record is also refused by replay. All five transition cases pass on
  stable and MSRV, and all-target Clippy passes. This is transition/codec
  evidence, not native receipt authentication or a disk durability proof;
  entry/aggregate resource bounds and the remaining native/fault acceptance
  inventory remain outstanding, so task 9302 stays In progress.
  Review CI `37254257934` has additionally completed macOS stable successfully;
  macOS MSRV and both Windows legs remain live.

- **Production ownership entry ceiling:** a bounded structural fixture seeds
  4,095 unresolved owners, then the production claim API admits the final
  pending effect as run 4,096. Snapshot/6 and base/2 decode the exact 4,096-entry
  state. Admission of an additional pending effect is refused with
  `store/execution_limit` while preserving the complete state, high-water mark,
  journal head and unclaimed request slot. Rechecksummed snapshot and base
  values containing 4,097 ordered claims are rejected at `entries`. All six
  transition cases pass on stable and MSRV, and all-target Clippy passes.
  This fixture models state restoration and producer enforcement; it does not
  assert that 4,096 native domains were launched or claims durably allocated
  one by one. Aggregate-size admission, the complete fault inventory and
  native receipt publication/authentication remain outstanding, and task 9302
  stays In progress. Review CI `37254257934` now has both macOS and both Linux
  toolchains successful; the two Windows legs remain live.

- **Complete execution block budget:** a restored structural fixture combines
  128 bounded claim/closure identities and individually bounded stopped results,
  accounting for every canonical key, delimiter and value. The production stop
  API refuses a result that would make the complete block 8 MiB plus one byte
  without changing state, head or request ownership, then admits the exact
  8 MiB result while retaining all unresolved owners. Snapshot/6 and base/2
  restore the exact block; recomputing the snapshot checksum cannot bypass
  plus-one refusal, and authoritative base decoding also refuses `bytes`.
  All seven transition cases pass on stable and MSRV, and all-target Clippy
  passes. This is resource enforcement from structural restored state with
  preauthenticated fixture evidence, not native authentication or proof that
  every fixture result was durably produced. Resource guard negative controls,
  the remaining fault inventory and native receipt publication/authentication
  are still required before task 9302 can finish.

- **Resource guard negative controls:** an isolated frozen `7d8f94a` checkout
  passed all fourteen integration and seven transition baselines. Raising each
  shared bound independently by one byte or entry (metadata, outcome, ownership
  count and complete block) made its named production-boundary case fail at a
  runtime assertion with exit 101; compilation failures were explicitly
  excluded. The original source was restored after each mutation and the
  complete fourteen-plus-seven baselines passed again; every retained log
  digest and the restored tracked source were verified. These four controls
  supplement the four earlier production authorization controls and fifteen
  pure ownership controls. They prove those tests depend on their resource
  guards, without turning structural/preauthenticated fixtures into native
  authentication evidence. Native receipt publication/authentication and the
  remaining durability fault inventory still prevent task 9302 completion.

- **Abrupt writer death after durable operations:** a subprocess now pauses
  with the writer held after claim, stop, failed settlement, acknowledgement
  settlement or interruption settlement, and the parent kills and reaps that
  exact child without clean Drop/snapshot publication. Cold read-only and
  writer opens preserve the published head/hash and monotonic identity;
  unresolved/stopped ownership excludes successors, original requests replay
  without append, consumed results cannot be stopped or settled twice, failed
  settlement retains the 109/110 eligibility boundary, and interruption keeps
  pending work/count zero while the successor receives run 2. Full replay and
  journal verification agree in all five stages. The new cold double-consumption
  check exposed a real diagnostic bug: absent ownership was mapped to
  `store/execution_owned`. Core matching now reports a claim-binding mismatch,
  and stop checks current ownership before original-record evidence lookup,
  producing the specified `store/execution_stale` even when that old record
  could be archived. SPEC makes the absent-owner rule explicit. All nine
  execution transition/crash cases pass on stable and MSRV, core ownership and
  replay cases pass on both, and all-target Clippy passes. These tests use
  preauthenticated fixture proofs and abrupt writer death after completed
  APIs; they do not establish native closure authentication, power-loss safety
  or every in-append/fsync/rotation fault boundary. Task 9302 remains In progress.

- **Writer death across journal rotation:** the abrupt-death harness now runs
  every durable claim/stop/settlement stage both within one segment and with a
  forced production rotation before each execution operation. Verified segment
  counts prove the rotated cases span two, three or four real segment files.
  All ten barrier/kill/reopen cases preserve head/hash, exclusion, idempotent
  replay, stale double-consumption refusal and retry/interruption semantics;
  the harness passes on stable and MSRV and all-target Clippy passes. This
  proves recovery after completed rotation/publication, not interruption inside
  rotation, failed fsync, power loss or native closure authentication. Task 9302
  remains In progress with those acceptance gaps explicit.

- **Initial production persistence CI complete:** review run `37254257934`
  completed successfully at exact source
  `b482fe03d9bedc0626d8a4e8ef06f82a005840cc`: all six Linux/macOS/Windows
  stable/MSRV legs, both provisioned native jobs and zero-dependencies passed.
  This completes the portable matrix for that original persistence unit; it
  does not validate the subsequent resource/crash tests or stale-completion
  correction. A new frozen review must cover that later range before task
  acceptance, and native authentication plus remaining fault boundaries still
  keep task 9302 In progress.

- **Legacy failed-count replay regression expanded:** the production admission
  test now reaches a selected journal-bound checkpoint cache, cache-free cold
  replay, a seal/archive/reopen cycle and a repeated pinned-seal refusal for
  an effect with a historical
  unbound failed attempt. Each phase checks contract refusal, unchanged head
  and hash, no request-id consumption and no run allocation. Formatting,
  source-size and diff checks pass; execution is pending the serialized frozen
  `9e4119a` host gate, whose debug workspace suite has passed and whose release
  suite is running. Review run `37257557135` covers that frozen source, not
  this later regression expansion; its two native prerequisite jobs and
  zero-dependencies job have passed while the six portable legs remain live.
  Task 9302 remains In progress and this entry does not claim new acceptance.

  Pre-execution fixture review corrected the archive setup: a real segment
  boundary now precedes the pending effect, permitting the first prefix seal;
  a further seal has no remaining admissible boundary below the live effect
  and must refuse without writing archive contents or changing state/head.
  The failed-count refusal is then checked again after cold reopen. This
  preserves the normative pending-effect pin rather than assuming two seals
  can advance through unresolved legacy work; runtime verification is pending.

- **Automatic rotation refusal harness added:** the production claim, stop
  and settlement APIs now each have a filesystem-obstruction case at their
  append's automatic rotation boundary. The test accelerates the public
  segment-record threshold, then makes the next segment path a directory;
  no synthetic I/O result or poison flag is injected. It checks unchanged
  state/head/hash/segment bytes, unused request identity, poisoned-writer
  refusal after removing the obstruction, cold read-only recovery and success
  only after writer reopen. Closure values remain preauthenticated fixtures.
  Formatting, source-size and diff checks pass; runtime execution awaits the
  existing serialized frozen review. This adds a real pre-publication rotation
  failure case, not failed-fsync, partial-write or power-loss evidence, and
  does not change task 9302's In progress status.

- **Legacy and rotation regressions executed:** at committed source `5deaeb3`,
  both the corrected legacy cache/cold/seal-pin admission case and the
  claim/stop/settlement automatic-rotation obstruction case pass on stable and
  MSRV 1.89.0, followed by passing workspace all-target Clippy; the serialized
  command group exits successfully. The task cache retains
  `legacy-rotation-5deaeb3.log` with the four named test results. The earlier
  isolated, clean `9e4119a` frozen host review also completed all eight stable
  commands successfully, covering workspace debug/release, formatting, source
  size, Clippy, rustdoc, zero dependencies and external embed acceptance;
  `9e4119a-host-gates.log` retains that evidence. The `5deaeb3` additions remain
  outside that frozen range and require the next complete review/matrix.
  Neither result establishes production native receipt authentication or
  failed-fsync/partial-write/power-loss coverage, so task 9302 stays In progress.

- **Execution descriptor fault harness added:** a crate-private `cfg(test)`
  seam swaps only the journal's file descriptor while leaving the production
  append path and its OS errors unchanged. Claim, stop and settlement each
  exercise a real write refusal using a read-only segment descriptor, then
  require poisoned refusal even after restoring a writable descriptor and
  successful retry only after cold reopen. Linux additionally uses `/dev/null`
  with an independently successful write and failed `sync_all` probe to reach
  the production write-all/failed-fsync path for all three APIs. Each case
  checks unchanged real segment bytes, logical state/head/hash and request
  identity plus read-only reconstruction and verification. The fsync case
  discards the attempted bytes on a substituted descriptor; it does not prove
  recovery when real segment bytes reached storage before fsync failed, power
  loss, or native receipt authentication. No production API or persisted byte
  changes. Formatting/source-size/diff checks pass; execution is pending.
  Task 9302 remains In progress.

- **Descriptor faults and poisoned-writer controls executed:** committed
  source `7ad7c89` passes both named write/fsync fault cases on stable and
  MSRV 1.89.0 plus workspace all-target Clippy; the serialized command exits
  successfully. In a separate clean checkout of that exact source, all three
  rotation/write/fsync tests pass before mutation; changing only the disk
  poisoned-append predicate produces a named runtime failure with exit 101
  in each test, without compilation failure. Restoring the exact source
  returns all three to passing and leaves the checkout clean. The task cache
  retains `execution-descriptor-faults.log`, `poison-negative-7ad7c89.json` and
  its SHA-bound logs; report/log digests and named failures were checked.
  These cases make poisoned exclusion load-bearing through each production
  execution mutator, without claiming partial persistence before failed fsync
  or native closure authentication. Frozen CI run `37257557135` now has both
  Linux stable/MSRV legs, both native prerequisite jobs and zero-dependencies
  passing; macOS and Windows legs remain live. Task 9302 stays In progress.

- **Native receipt/public-store bridge authored:** task 9302 adopts the
  new `evidence_probe.py` driver alongside its existing store-test footprint.
  A native-only store fixture reuses the proved root identity helper, binds
  the exact durable claim/hash in protected authority state before authorizing
  launch, and issues a protected canonical store receipt only after the
  helper's real closed-domain observation. The driver requires a real root
  and descendant before native closure, exercises public opaque receipt reads,
  stop and atomic acknowledgement, and covers unprivileged binding, unbound
  launch, premature receipt publication, writable receipt and symlink refusal.
  All units/directories use fresh task-specific namespaces; unrelated-process
  survival and cleanup are checked. This issuer is proof infrastructure, not
  the shipped production authority or runner. Initial fixture compilation on
  stable/MSRV and all-target Clippy passed before the latest binding/launch
  wiring review; final compilation and provisioned native execution are still
  pending, and no native receipt/publication acceptance is claimed. Task 9302
  remains In progress.

- **Native bridge first-run correction:** source `13e4456` compiled both
  fixture binaries, then the provisioned driver failed before claim binding
  because it tried to read the helper's deliberately root-private response
  directly. Owned unit/namespace cleanup ran. The driver now reads that
  response through the existing explicit privileged read path, preserving its
  permissions; native acceptance remains unproven until a complete rerun.

- **Native bridge liveness barrier corrected:** the `fca4f6b` rerun reached
  real durable binding, both pre-launch refusals, premature-publication refusal
  and native launch, then failed because the driver requested a nonexistent
  descendant-specific readiness filename. The reused fixture's documented
  `ready` handshake now replaces it, with exact descendant cgroup membership
  and a fresh liveness challenge/response before closing. Owned cleanup ran;
  this failed run does not establish native receipt acceptance.

- **Genuine native receipt/public-store bridge executed:** exact clean source
  `f802c0acb4473b2cb8c03c3b5a62f48ece9bfcf0` passes all nine driver inventory
  cases on stable and MSRV 1.89.0, followed by passing workspace all-target
  Clippy. Both reports bind source, compiler and both fixture executable
  digests; inventories, passing flags, unrelated-process survival and the
  explicit `production_backend: false` scope were checked. A live descendant
  answers a fresh challenge in the claimed cgroup before real root-protocol
  revocation/closure; only its protected closed observation permits receipt
  issuance, then public `VerifiedClosure::read`, durable stop and atomic ack
  with cold duplicate replay succeed. Unbound launch, unprivileged binding,
  premature publication, writable receipts and symlinks refuse. The task cache
  retains `native-evidence-liveness-rerun.json` and
  `native-evidence-f802c0a-msrv.json` plus invocation logs. This establishes
  genuine closure-to-public-reader integration with a fixture issuer; the
  production authority/runner and full new-source matrix remain unfinished.

- **Frozen portable CI finding:** run `37257557135` at `9e4119a` now has both
  Linux legs and macOS stable passing, but macOS/MSRV release tests fail in
  `journal_io::tests::version_marker_preflight` at `init` with `Io("locked
  40023")`; Windows legs are still running. The completed failed job log was
  fetched directly and retained as `ci-37257557135-macos-msrv-failed.log`.
  This matrix is not accepted. The immediate init guard-drop/reacquire path
  and possible transient descriptor inheritance beside the new subprocess
  crash tests require a load-bearing investigation before the next review.
  Task 9302 remains In progress.

- **Writer lease retained-descriptor defect reproduced and fixed:** a named
  Unix test keeps a real duplicate of the owned lock descriptor alive after
  dropping its writer; the original code fails at immediate reinitialization
  with `Locked { pid: <same process> }` and runtime exit 101. One internal
  `WriterLock` guard now attempts explicit unlock before closing, including
  init/migration/repair error paths. The same test passes and independently
  confirms the replacement writer stays exclusive; all journal/migration tests
  pass on stable/MSRV, all five execution crash/fault harness tests pass on
  stable, and all-target Clippy passes with terminal command success. The task
  cache retains `writer-lease-original-failure.log` and `writer-lease-fix.log`.
  SPEC/API/embedding/release docs bind this no-byte/no-public-API correction.
  The deterministic defect is consistent with the failed macOS init path
  beside concurrent subprocess spawning, but a new native macOS/MSRV CI leg
  must validate that axis rather than treating Linux as proof. Full frozen
  host review and portable/native matrix remain required; task 9302 remains
  In progress.

- **Receipt bridge integrated into native matrix:** task 9302 adopts the
  matrix orchestrator alongside its proof driver. Both provisioned CI jobs
  now require the nine-case native receipt/public-store inventory in addition
  to the existing 61 cases, for 70 required cases total. The orchestrator
  verifies the bridge's explicit fixture-issuer scope and both executable
  digest fields while retaining exact source/compiler/inventory checks. This
  wiring still needs a full clean-source matrix run and does not identify the
  fixture issuer as a shipped production authority. Task 9302 stays In progress.

- **New frozen review launched:** exact source
  `bcf663297f872d1232f97351b7771c0878d27685` is isolated in a clean detached
  checkout with its own Cargo target, covering the writer-lock correction,
  descriptor/rotation faults, legacy seal-pin case and genuine native receipt
  bridge/matrix wiring. The eight-command stable host gate is running with
  serialized workers. Publication to the existing authorized review branch
  succeeded; CI run `37259980746` is queued for six portable legs, both now
  70-case provisioned native inventories, and zero dependencies. The older
  `9e4119a` matrix remains unaccepted because of its recorded macOS/MSRV
  release failure. No task acceptance advances until the new source-bound
  review, required controls and matrix complete.

- **Expanded native CI inventory verified:** both provisioned jobs in run
  `37259980746` completed successfully at exact clean source
  `bcf663297f872d1232f97351b7771c0878d27685`, with all 70 cases passing.
  Downloaded stable/MSRV artifacts were checked independently against each
  frozen suite's declared inventory: eight suite reports, nine genuine
  receipt/public-store bridge cases, source/compiler identities, fixture
  digest fields, per-report SHA digests, I/O cancellation log digest, unrelated
  process survival and expected final-kill negative-control evidence match.
  Bridge scope remains explicitly fixture issuer / non-production backend.
  The task cache retains both artifacts under `ci-37259980746/`. The frozen
  local gate has passed debug workspace tests and entered release compilation;
  six portable CI legs remain running, so the overall matrix is not accepted.
  An exact-source isolated checkout and source-restoring harness are prepared
  for explicit-unlock and native no-follow causal controls; they have not run
  while the host gate occupies the serialized build slot. Task 9302 remains
  In progress.

- **In-append writer-death harness authored:** test-only barriers now pause
  the actual segment append before `write_all`, after its successful write and
  after successful `sync_all`, before the store API can return. A new owned
  child harness kills/waits the writer at all three phases for claim, stop and
  atomic failed settlement, then checks the cold observable prefix, head/hash,
  request ledger, monotonic run state, exclusion and retry deadline. No normal
  Drop/snapshot publication is permitted. The after-write phase establishes
  recovery of a complete observable record before the explicit fsync call;
  it does not simulate host power loss or claim unsynced bytes are physically
  durable. Native receipt values remain preauthenticated transition fixtures,
  separate from the genuine nine-case receipt bridge. Production append keeps
  its original write/sync order and bytes. Formatting and source-size checks
  pass; runtime compilation/execution awaits the existing serialized frozen
  host gate. These later tests are outside frozen source `bcf6632`, and task
  9302 stays In progress.

- **Frozen host gate complete; append fixture type corrected:** the isolated
  `bcf6632` host command group exits successfully with all eight commands and
  its exact-source completion marker. The first build of the later `5eb09cb`
  append harness stops before execution because its expected attempt count
  used `u32` while the public store accessor returns `u64`; the expectation
  now uses that public type. This is a test compile correction, not a runtime
  defect or passed crash proof. The original log is retained and the corrected
  stable/MSRV run remains pending; portable CI is still running.

- **Append fixture now checks the execution ledger:** corrected source
  `e543549` compiled and reached actual writer-death recovery, then failed
  because it expected a claim-era failed settlement to change the separate
  legacy `effect_attempted` accessor. The test now checks the public execution
  state's durable failed count and separately asserts that legacy count stays
  zero. This fixture expectation is grounded in the existing settlement
  contract; no production behavior changes. The failed runtime log is retained
  and complete stable/MSRV execution still requires a corrected rerun.

- **Append phases and authentication/release controls executed:** committed
  source `f27b132` passes the nine actual claim/stop/settlement writer-death
  phases on stable and MSRV, followed by passing all-target Clippy; the
  serialized command exits successfully. `append-death-ledger-rerun.log`
  retains the named parent/fixture results. These later tests remain outside
  the completed eight-command `bcf6632` frozen host gate.

  In the separate exact-`bcf6632` control checkout, the duplicate-descriptor
  lock regression and all nine genuine native receipt cases pass. Neutralizing
  only explicit writer unlock yields the named runtime failure with exit 101;
  neutralizing only the reader's no-follow flag makes the genuine symlink
  refusal case fail at runtime with an unexpectedly accepted opaque proof
  (driver exit 1). Each temporary committed mutation changes one source file;
  neither failure is a compilation error. Restoring the exact base yields
  passing lock/native baselines and a clean checkout. The report, mutation
  source identities, named phase, log/report SHA digests and restored source
  were checked; `auth-lock-negative-bcf6632.json` and its artifacts remain in
  the task cache. Both Linux portable CI legs now pass at `bcf6632`, while
  macOS/Windows legs remain live. Task 9302 remains In progress with final
  review and remaining acceptance requirements explicit.

- **Stopped-result archive lifecycle exercised:** a new real disk-store case
  allocates/stops a claim, cancels the pending effect without consuming its
  owner, then seals/archives/reopens twice. Both verified archives and base/2
  carry the identical stopped result and original claim-record hash; read-only
  and writer reopen retain the owner and run high-water. Atomic interruption
  then consumes the cancelled effect's stopped owner without a failed count;
  a third archive removes its original evidence from the live/base indexes.
  A subsequent production stop refuses `store/execution_stale` before missing
  evidence lookup, without state/head/hash/request-slot changes. Stable and
  MSRV named tests and all-target Clippy pass with terminal command success;
  `stopped-seal-proof.log` retains execution evidence. Receipt values in this
  persistence test are preauthenticated fixtures; genuine receipt reading is
  covered separately by the nine-case native bridge. This later test remains
  outside frozen `bcf6632` review and needs the next source-bound full gate.
  The earlier failed macOS/MSRV axis now passes at `bcf6632`; macOS stable and
  both Windows legs remain running. Task 9302 remains In progress.

- **Archived stale-completion guard made load-bearing:** an isolated clean
  checkout of `4e6c3be81298fa577d8d46b18006e67dcbbdac67` passes the real
  stopped-owner/base/archive lifecycle case. Disabling only the producer's
  owner-match check before original evidence lookup changes the spent archived
  completion from `store/execution_stale` to `store/execution_evidence`, causing
  the named runtime test to fail with exit 101 without compilation failure.
  Exact source restoration returns the same test to passing and leaves the
  checkout clean. The report and all log SHA digests were checked and retained
  as `spent-stale-negative-4e6c3be.json` and associated logs in the task cache.
  Both macOS stable/MSRV legs now pass at frozen `bcf6632`; both Windows legs
  remain running. No later test is attributed to that older frozen range, and
  task 9302 remains In progress.

- **Latest persistence range frozen for final gates:** clean detached source
  `1f21fe480440261eae20a75d066d3cb903643026` includes the nine in-append
  process-death cases and repeated stopped-owner archives omitted from the
  earlier `bcf6632` review. All eight stable host checks are running serially
  in an isolated checkout with a separate target directory and retained
  `1f21fe4-host-gates.log`; completion has not yet been observed. The same
  exact source is pushed only to the authorized native-review branch, and
  CI run `37262415626` is running its six portable legs, two native matrices
  and zero-dependency job. The workflow has no cancellation rule, so the
  earlier `37259980746` Windows release-test legs continue independently;
  that older run currently has seven completed successes. The archived stale
  control's three log digests and runtime-only failure were independently
  rechecked. Task 9302 remains In progress pending review and terminal gates;
  no contained runner, authority publisher or legacy-quiescence issuer is
  advertised as shipped by these fixture and persistence proofs.

- **Latest native artifacts independently verified:** both native jobs in
  run `37262415626` pass all 70 cases on exact clean source `1f21fe4`.
  Downloaded reports are retained under `ci-37262415626/{stable,msrv}` in the
  task cache. Independent verification checks all eight report SHA digests,
  their literal source case inventories and success rows, compiler/source
  identity, executable digest fields, unrelated-process survival, native I/O
  cancellation log digest and the deliberately failing live-domain final-kill
  control. Stable used Rust 1.99.0; MSRV used Rust 1.89.0. Receipt publication
  remains explicitly a fixture issuer, not a shipped production authority.
  The host gate and six portable jobs remain running; no task completion is
  inferred from native success alone.

- **Format documentation review finding resolved:** current migration text
  and the API format/domain inventory now consistently name VERSION 11,
  root/4 and snapshot/6, while the VERSION 10 seal rules are explicitly
  historical. The execution section distinguishes implemented store mutators
  from pending acceptance and native runtime integration. Source constants
  were inspected, and diff/file-size checks pass; this documentation-only
  clarification changes no behavior, persisted bytes or source covered by
  the frozen code gates, so the full build gates are not repeated for it.

- **Task 9302 acceptance audit in progress:** the race and observation tests
  in `execution_claims.rs` cover the writer and pending-membership preconditions;
  the stopped-owner/retry and atomic-ack producer tests cover exclusion through
  settlement, exact retry eligibility and double-consumption refusal. The
  completed-API and in-append child-death suites, torn records, descriptor
  failures and rotation refusal cover observable recovery boundaries without
  claiming host power-loss proof. The cancelled and stopped-owner archive
  cases cover original claim hashes and results through real repeated bases,
  archives and cold replay. VERSION 1–10 migration, future-version refusal,
  checkpoint/cache tests and semantic tampering cover historical compatibility
  and loader refusal. Producer/loader boundaries cover request IDs, claim
  metadata, outcome bytes, entry counts and the aggregate execution block;
  structural boundary seeds are not claimed as thousands of native domains.
  External embedding and the genuine native closure-reader/stop/ack bridge
  cover downstream construction and verified stop. Task completion still
  requires terminal host and six-platform/toolchain results.

- **Legacy admission producer gap addressed in source:** review found the
  legacy-prefix admission test only exercised the pure fold. The new named
  `legacy_admission_requires_matching_prefix_and_replays_after_cold_reopen`
  test exercises the production store API over an actual VERSION 10 fixture:
  migration preserves historical bytes and quarantine, wrong-prefix evidence
  refuses without a clock tick or request slot, matching evidence enables in
  one record, and a snapshot-free reopen replays the original request before
  the now-stale prefix check while refusing a fresh request with that proof.
  Its proof is explicitly preauthenticated test infrastructure, not a native
  quiescence issuer. Formatting and diff checks pass; execution is pending the
  live serialized host gate, so this later test is not attributed to frozen
  `1f21fe4` or its CI results. The older Windows/MSRV leg has now passed;
  Windows stable remains running in `37259980746`.

- **Windows review timeout diagnosed and budget corrected:** older frozen run
  `37259980746` is terminal with eight successes and Windows stable cancelled.
  The check annotation explicitly reports the 45-minute job limit, and its
  retained sanitized diagnostics show release tests passing through `io_bounds`
  and advancing into `journal_record_bounds` immediately before cancellation;
  this is not evidence of a test failure or an indefinitely hung case.
  `ci-37259980746-windows-stable-cancelled.log` retains the completed job log.
  Task 9302 adopts the CI workflow footprint to set a bounded 75-minute budget
  for both Windows toolchains, retaining 45 minutes for other portable hosts,
  every gate command and the 1,000-crash test floor. The currently live newer
  run retains its original timeout; it is not restarted or cancelled, and the
  changed budget awaits the next frozen-source review. This infrastructure
  change does not count a cancelled Windows leg as passing or complete task
  acceptance. Diff and file-size checks pass; no build runs concurrently with
  the still-live isolated host gate.

- **Final audited persistence source frozen:** the isolated `1f21fe4` host
  process terminated successfully with all eight commands passing; its final
  marker and eight exit records were independently checked, with log SHA-256
  `5e6a77e8b4bf9250c869006462f3eb5713e1e42809f54beffb2aa33f3192b5c4`.
  After that process ended, the new legacy-admission producer case passed at
  stable and MSRV and all-target store Clippy passed on exact `cf3f600` source;
  `legacy-admission-cf3f600.log` retains both named runtime passes, three zero
  exits and its completion marker. No native legacy-issuer proof is inferred
  from this preauthenticated fixture case. Source
  `cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb` now freezes the complete audited
  persistence test set, corrected format documentation and Windows timeout.
  Its isolated eight-command host gate is running serially, retaining
  `cf3f600-host-gates.log`; run `37263404252` starts the full nine-job CI matrix
  on the same source with the bounded Windows budget. Both earlier runs are
  preserved, including the diagnosed timeout and pending `1f21fe4` portable
  axes; none substitutes for the latest terminal results. Task 9302 remains
  In progress pending those gates and final review, with downstream contained
  runner and runtime integration still planned.

- **Final-source native proof verified:** both native jobs in `37263404252`
  pass all 70 cases at frozen
  `cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb`; retained artifacts are under
  `ci-37263404252/{stable,msrv}` in the task cache. Independent verification
  checks all eight suite hashes and exact AST-literal inventories, every
  success row, clean source and compiler identity, executable digest fields,
  unrelated-process survival, I/O cancellation log and its digest, and the
  deliberately failing live-domain final-kill control. Stable used 1.99.0 and
  MSRV used 1.89.0. The native evidence bridge remains labelled fixture issuer
  and non-production backend. The final host process and six portable legs
  remain live; no new tests or production changes are added to this frozen
  range while its acceptance gates run. The earlier `1f21fe4` run now has
  both Linux portable legs passing, with macOS and Windows still running.
  Task 9302 remains In progress, and the full plans 20–23 goal is unchanged.

_Task frontmatter remains authoritative; registration does not release the native gate._
