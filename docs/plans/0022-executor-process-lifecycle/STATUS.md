# Plan 0022 — Executor Process Lifecycle — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and the integration coordinator owns
lifecycle updates.

- **Status:** Registered by hand; native prerequisite complete;
  durable execution persistence complete; contained runner implementation started.
- **Goal:** prevent a successor from overlapping a surviving local handler
  tree, with bounded shutdown and evidence-based restart for both process and
  MCP handlers.
- **Root cause:** direct-child handles and short-lived writer locks do not
  contain descendants or persist execution ownership across executor death.
- **Approach:** resolve the native containment prerequisite, journal claims
  before launch, prove tree closure before reuse, and drive every execution
  host through the same shutdown and recovery protocol.
- **Progress:** 2/7 tasks done; 0 blocked; 0 dropped; contained runner In progress.
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

- **Final host review and admission control complete:** the isolated
  `cf3f600` host process terminated with exit zero and all eight commands
  passing, including debug/release workspace tests, all-target Clippy,
  warning-free documentation, zero dependencies and downstream embedding.
  Its eight exit records and completion marker were independently checked;
  `cf3f600-host-gates.log` has SHA-256
  `95d1b0cf0b533d4ed6d4c6a737d2a8001446195565860982e2f512c0c7c15422`.
  An independent cache-only checkout then passes the named legacy admission
  case, fails it with exit 101 when only the producer prefix check is disabled
  (`store/execution_stale` replaces the required `store/execution_evidence`),
  and passes after exact restoration. The source is clean, all three log
  digests were checked, and no compilation failure earns the negative result;
  `admission-prefix-negative-cf3f600.json` retains this 0/101/0 control.
  Final CI `37263404252` now has both Linux and both macOS portable legs,
  both 70-case native jobs and zero dependencies passing. Both Windows legs
  remain live under the corrected bounded budget, so task 9302 remains
  In progress pending their terminal results and final acceptance closure.

- **Task 9302 accepted:** final frozen source
  `cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb` has terminal success in all nine
  jobs of CI `37263404252`, including both Windows toolchains, all six native
  portable legs, both independently verified 70-case Linux containment/store
  evidence matrices and zero dependencies. The eight-command isolated host
  gate also passed. The requirement audit above covers writer races and stale
  observation, exclusion through verified stop and atomic settlement, retry
  boundaries, abrupt/torn recovery and real descriptor/rotation faults,
  historical migrations and byte preservation, repeated actual archives,
  bounded producers/loaders, public embedding and legacy-prefix admission.
  Runtime guard controls fail when the relevant checks are disabled and pass
  after exact restoration. Documentation review findings are resolved.
  Task 9302 is Done and lands at the frozen source; task 9303 is now Ready.
  Native authority publication, the production contained runner, service
  ownership, shutdown, reconciliation and complete runtime acceptance remain
  downstream work. This closes persistence acceptance, not plan 0022 or the
  plans 20–23 goal, and makes no claim that the fixture issuer is shipped.

- **Task 9303 started:** a private shared capture accumulator retains at most
  4 KiB, hashes at most 1 MiB and accepts further drained chunks without
  retaining them; EOF is required for a whole-stream digest. The existing
  file reader uses it without changing its read-limit behavior. Two focused
  runtime tests pass for exact prefix/hash limits, excess draining, bounded
  retained capacity and cancelled-stream truthfulness. An initial compile
  check found a shared MCP hash import removed during extraction; restoring
  it resolved the failure before both tests passed. This is preparation for
  live contained capture, not proof of bounded existing spool files, native
  closure, worker cleanup or a shipped production backend; those requirements
  and the complete runner review remain outstanding.

- **Live capture transport integrated:** the production Linux runner now
  drains process stdout/stderr and MCP stderr through nonblocking sockets in
  both direct polling and scheduler completion observation, with no spool
  files or capture-reader threads. Per-poll work is 64 KiB per stream and
  final drain work is bounded; retained peers cannot block finalization or
  earn a whole-stream digest without EOF. The new `lifecycle_runner` case
  uses real noisy roots at 4 KiB/plus-one and 1 MiB/plus-one, plus 8 MiB on
  both streams, checking exact stderr prefixes/digests and zero spool files
  throughout execution. The retained-peer unit case and accumulator bounds
  pass; runner, MCP, scheduler and these new tests pass serially on pinned
  Rust 1.89. All-target execute Clippy also passes on pinned Rust 1.89. Existing
  non-Linux file capture is retained; the six portable/native integration
  gates remain required for task acceptance. No new public item or error is
  introduced. Production claim authentication, native closure, MCP protocol
  worker cancellation and uncertain ownership remain outstanding; task 9303
  stays In progress and no contained result is claimed by these tests.

- **Owned Linux MCP worker integrated:** a private worker now retains its
  thread handle and independent stdin/stdout socket cancellation controls.
  A conversation result is collected only after the thread is observed
  finished and joined; timeout/cancellation shuts down both directions,
  retaining unfinished workers and refusing another launch until their join.
  Polling performs only nonblocking completion observation; Drop cancels
  best-effort and joins already-finished handles. Thread creation failure
  kills/reaps the root rather than panicking or abandoning it. The retained
  peer case passes for blocked reads and large blocked request writes while
  both peers remain open. A separately labelled unavailable-control injection
  checks actual runner launch refusal through the unfinished worker and
  permits launch only after its observed join; it is not native shutdown
  failure evidence. Both new cases pass on pinned Rust 1.89; existing
  runner/MCP/scheduler/live-capture suites pass on that toolchain, and pinned
  Rust 1.89 all-target execute Clippy passes. No public item or error code is added.
  Native authority authentication, domain closure, explicit contained cleanup
  states, descendant integration and complete portable/native acceptance
  remain outstanding, so task 9303 remains In progress.

- **Protected authority registration/binding implemented:** task 9303 adopts
  `fsm-execute/Cargo.toml` and `src/containment/` for the separately provisioned
  authority binary. Supported Linux callers must be root before request I/O;
  other runtimes refuse. Registration verifies a store read-only, creates a
  generation exclusively and persists its canonical path/device/inode under
  protected root ancestors. Binding checks a protected allocator-produced
  prepared record, boot and native identities, then independently opens that
  registered store to require the original durable claim hash, all claim
  fields and current unstopped ownership of a pending effect on a running
  instance. Its private canonical create-once record fsyncs before the parent;
  symlink, nonregular, oversize, noncanonical and unprotected records refuse.
  The real cold-journal test covers matching/wrong hashes, changed handler
  identity and durable cancellation; pure domain metadata in that test is
  explicitly not native enrollment evidence. File cases cover exact 8 KiB,
  plus-one, symlink refusal and unchanged duplicate publication. A separately
  invoked root registration test passes on stable and MSRV, checking private
  ownership/mode, exclusive duplicate refusal and unchanged journal head,
  with fresh owned namespaces/stores removed afterward. Positive native
  binding awaits the production allocator, rather than seeding its prepared
  record and mislabelling that as allocator evidence. Full broker, protected
  entry, registered-store access policy, closure and runner integration remain
  outstanding; registration/binding alone launches no handler and issues no
  closure receipt.

- **Toolchain attribution corrected:** the preceding capture and worker
  entries originally labelled unqualified Cargo commands as stable even
  though `rust-toolchain.toml` selected 1.89.0. Those historical entries now
  correctly describe pinned MSRV checks. Explicit `cargo +stable` uses
  Rust 1.98.1 and passes every execute target, including the unchanged public
  inventory and cumulative capture/worker tests, the new authority tests and
  refusal integration; its all-target execute Clippy and zero-dependency
  check pass. Explicit Rust 1.89 authority/refusal/live-capture checks pass,
  and actual privileged registration passes using binaries from each
  toolchain. Review also corrected an initial cancellation-test signature
  mismatch and the forbidden stderr logging macro before the final passes.
  Stable formatting, source-size and diff checks pass. These are preliminary
  local checks: full workspace debug/release, workspace Clippy/docs, external
  embedding and the six real portable plus complete native CI gates have not
  been rerun for this task's current source, and remain required before final
  task acceptance. Task 9303 stays In progress; all plans retain full scope.

- **Production allocator implemented, native execution pending:** authority
  registration now initializes a root-protected counter bound to namespace,
  generation, boot and actual authority directory identity. `prepare` checks
  the registered store identity, counter and complete bounded intent/native
  inventory; it durably writes a create-once intent and advances the counter
  before creating a protected empty cgroup and publishing actual native
  identity. Missing/incomplete intents, rollback, copied authority, boot
  changes and unknown domains refuse without recycling. Names include
  generation; explicit initial limits are 4096 lifetime allocations and
  32768 inventory entries. Preparation runs no user code and does not enable
  admission. Stable/MSRV authority/refusal/capture checks compile the five
  opt-in native cases; ignored cases are not claimed as native passes.
  Stable all-target execute Clippy passes. The actual privileged registration
  test also passes locally, including the unprovisioned-parent branch that
  refuses before publishing an intent or advancing its counter. This host's
  cgroup mount is read-only and its system.slice owner is mapped to nobody,
  so positive cgroup allocation requires the existing real provisioned CI
  runtime. The matrix adopts a production authority probe with five cases:
  empty preparation, unknown-domain refusal, counter rollback, persisted
  incomplete intent and genuine journal claim binding. Reports retain exact
  compiler/source/executable hashes, per-case logs and unrelated-process
  survival; scope is production allocator/binding, not complete backend.
  Full native inventory becomes 75 cases only when those five execute and
  pass alongside the existing 70. Frozen-source CI is the next review step;
  protected entry, broker, closure and contained runner remain incomplete,
  and task 9303 stays In progress.

- **Allocator native review passes; portable import defect corrected:** frozen
  source `dffc2086718080067b1a42c597ee8eb49bef5957` runs in CI `37270144908`.
  Both native jobs pass all 75 cases, including the five actual production
  allocator/binding cases; artifacts retained under `ci-37270144908/{stable,msrv}`
  were independently checked against exact frozen AST inventories, nine suite
  hashes, per-case successes, compiler/source identity, executable digests,
  unrelated-process survival, I/O cancellation and the expected failing live
  final-kill control. Stable uses Rust 1.99.0 and MSRV uses 1.89.0. Each new
  authority case log hash and named runtime pass is checked; its scope remains
  production allocator/binding with `production_backend: false`. Zero
  dependencies also passes. Four macOS/Windows portable legs fail compilation:
  sanitized retained macOS stable and Windows MSRV logs identify the shared
  capture module's unconditional `spawn_error` import, unused outside Linux
  under CI's warning refusal. The import is now grouped under its Linux cfg;
  no capture behavior or native authority implementation changes. The two
  Linux portable legs remain live on the frozen source and are not restarted
  or cancelled. A new source review is required for the portable fix; these
  failures cannot count as six-leg acceptance, and task 9303 remains In progress.

- **Protected entry verifier implemented:** the private `gate` path accepts
  only a canonical namespace/generation/allocation route under an unprivileged
  identity. A bounded immutable root-owned grant supplies the claim, original
  hash and absolute argv; authority/cgroup device/inode, boot and exact own
  cgroup membership must match before exec, and the grant is re-read before
  replacement. Caller-provided commands are rejected by the actual binary.
  Shape tests cover valid absolute commands, missing/unknown fields, malformed
  hashes, empty/excess argv, bare commands, nonstring arguments and NULs;
  their pure metadata authenticates no real publisher or enrollment. Stable
  and MSRV authority/refusal/capture tests pass and stable all-target execute
  Clippy passes. Production grant publication, claim revalidation immediately
  before launch, systemd integration, bounded native I/O and full closure
  remain outstanding; fencing/killing the already enrolled gate is required
  to cover the last validation-to-exec race. This entry unit is later than
  frozen `bfcfd79` and is not attributed to its live CI review. Task 9303
  remains In progress with full native entry and runner acceptance pending.

- **Entry-grant publisher implemented; validation pending:** root-only
  `authorize` matches the immutable protected claim binding and revalidates
  runnable journal ownership under the authority lock before exclusively
  publishing a synced root-owned 0440 grant for a nonzero group. Any
  closing/closed marker refuses, including malformed markers. The genuine
  native binding case now checks these refusals, publication ownership/mode,
  duplicate refusal and refusal after durable cancellation; these new
  assertions have not yet executed on a provisioned native host. Formatting
  and diff checks pass. The initial compile attempt found an incorrect
  `fs::chown` reference, corrected to the Unix API; the subsequent serialized
  validation build was rejected by automatic approval review because swap
  is full and the workspace memory rule prohibits it. A specific user
  exception is pending; compilation, Clippy and native acceptance for this
  unit remain unverified. Earlier frozen `bfcfd79` CI `37271018161` reports
  native stable, native MSRV and zero-dependency success, while all six
  portable jobs remain in progress; those results do not cover this unit.
  Trusted broker group derivation, real gate launch, I/O and closure remain
  outstanding, and task 9303 stays In progress.

- **Grant publisher native review passes:** source
  `6ede0027cfbe885ef62a0a7da4f3897939f3f1a8` is frozen in CI `37272197029`.
  Both native jobs and zero dependencies pass. Retained artifacts under
  `ci-37272197029/{stable,msrv}` independently verify all 75 named cases
  against frozen AST inventories, nine report hashes, exact source/compiler
  identity, executable digests, authority named-pass log hashes and unrelated
  process survival. Stable uses Rust 1.99.0; MSRV uses 1.89.0. The expanded
  genuine binding case executes the actual publisher and checks malformed
  closing/closed marker refusal, group-zero refusal, root/group/mode ownership,
  duplicate refusal and current-claim refusal after durable cancellation.
  I/O cancellation and the expected failing final-kill control are verified
  separately. This supersedes pending native compilation/execution evidence
  above through independent CI hosts; local builds remain disallowed by the
  full-swap rule without a specific user exception. All six portable jobs
  remain live, and their full acceptance is not inferred. Group 1 is a test
  fixture, not proof of trusted broker group selection or actual gate launch;
  `production_backend: false` remains accurate. Task 9303 stays In progress.

- **Enrolled entry authorization wait implemented:** exact routed cgroup
  membership now precedes a fixed five-second wait for exclusive grant
  publication, allowing a future launcher to derive the DynamicUser group
  before publishing. Only absence permits waiting; errors refuse, and any
  closing/closed marker refuses during waiting and immediately before exec.
  The binding validator shares this fail-closed marker check. The native
  genuine-binding case adds zero-bound missing-grant, malformed-marker and
  published-grant checks for the actual gate helper; these do not prove
  positive waiting or a real enrolled unprivileged gate launch. Local
  compilation remains prohibited by the full-swap rule; frozen-source CI
  validation is the next step. Full manager launch, trusted group selection,
  native I/O and closure fencing remain outstanding; task 9303 stays In progress.

- **Durable admission revocation implemented:** root-only `begin-close`
  checks the protected prepared domain, canonical route, actual authority
  and cgroup identities and boot under the shared authority lock. It fsyncs
  an exact domain closing marker before removing entry and pending grants
  and syncing the directory. Exact replay is permitted; malformed existing
  material and unexpected grant types/ownership refuse. Cancellation does
  not prevent revocation, and no claim is cleared. The genuine native binding
  case adds revocation/replay assertions and confirms that journal ownership
  remains and no closed marker is issued; execution remains pending CI.
  This is not manager admission fencing, native termination or a closure
  receipt, all of which remain required by task 9303. Local builds remain
  prohibited by the full-swap rule without a specific exception.

- **Closing replay durability review corrected:** an existing exact marker
  is now synced with its parent before any grant deletion, covering replay
  after a previous visibility-without-durability failure. The genuine native
  binding case replaces only its owned pending grant with a symlink: closing
  durably fences admission and removes the entry, then refuses the unexpected
  pending type without touching its binding target; authorization remains
  refused. Explicit fixture repair permits replay and ownership remains
  durable. This is a real filesystem refusal/replay case, not a physical
  power-loss or full native termination proof. Formatting and source checks
  precede a new frozen-source CI review; task 9303 remains In progress.

- **Kernel termination submission implemented:** root-only `request-kill`
  completes durable revocation and retains the authority lock while opening
  no-follow freeze/kill controls, verifying root ownership/native device and
  actual cgroup identity before and after each open, then submitting `1`.
  Failures preserve closing admission and journal ownership; success is
  submission only, without closure receipts or capacity release. The genuine
  binding native case moves two administrative sleep fixtures into its actual
  prepared group, verifies membership/population, submits the production
  operation and observes both fail/reap and the group become empty within
  three seconds. This deliberately does not prove atomic handler enrollment,
  DynamicUser isolation, descendant inheritance or complete manager fencing.
  Local builds remain prohibited by the full-swap rule; frozen-source native
  CI must execute these new assertions before acceptance. Full runner and
  closure integration remain outstanding; task 9303 stays In progress.

- **Production manager capability check implemented:** preparation verifies
  the fixed root-protected systemctl and clears inherited environment before
  querying loaded/active `system.slice` at `/system.slice`. It bounds each
  output stream to 4 KiB, observation to two seconds and query-child cleanup
  to one second; failure refuses before allocation intent/counter mutation.
  Pure response cases refuse missing/inactive/wrong/duplicate/unknown fields
  and invalid encoding. Socket cases independently cover exact 4096/4097
  bytes, retained-writer nonblocking progress and truthful EOF. Existing five
  production allocator native cases now require the actual manager query;
  new source execution remains pending frozen CI because local builds remain
  prohibited by the full-swap rule. This is capability detection, not launch
  authorization or closure proof; task 9303 remains In progress.

- **Closing replay native evidence independently accepted:** frozen source
  `2d487a35ae49da47c07eb0b397e9dffa4389362a` passes both native jobs in CI
  `37272965618`. Retained `ci-37272965618/{stable,msrv}` artifacts verify
  all 75 cases against exact frozen AST inventories, nine report hashes,
  source/compiler identity, executable digests, named authority passes/counts
  and log hashes, and unrelated-process survival. Stable is Rust 1.99.0;
  MSRV is 1.89.0. The actual genuine-binding test executes grant publication,
  entry wait-helper absence/marker/read checks, cancellation refusal, closing
  revocation, pending-symlink refusal, explicit repair and exact replay while
  retaining journal ownership and issuing no closed evidence. Native I/O
  cancellation and the expected failing final-kill control are checked
  separately. This evidence covers closing replay, not later `c9f877e`
  kernel termination or `ae738f9` manager detection, whose CI runs remain
  queued. Older import-fix source `bfcfd79` in CI `37271018161` now passes
  all four Linux/macOS portable legs; both Windows legs are still running,
  so six-leg acceptance remains incomplete. Task 9303 stays In progress.

- **Production kill case extended to an inherited descendant:** the genuine
  binding case now enrolls a re-executed administrative fixture before opening
  its stdin barrier. Only then does it fork a sleep descendant; both fixture
  and test parent verify actual inherited cgroup membership, and all two roots
  plus the descendant must appear in the owned group before production kill
  submission. Roots must fail/reap and native population reach zero within
  three seconds. Readiness capture is bounded to 4 KiB/nonblocking/three
  seconds, and failure cleanup verifies the owned cgroup identity before any
  group kill and bounds direct-root reaping. The helper's ignored libtest entry
  is a re-execution fixture, not an additional accepted inventory case. This
  extends the same 75-case matrix without claiming actual handler enrollment,
  DynamicUser isolation, manager admission fencing or permanent closure.
  Formatting/diff/file-size checks pass; compiled/runtime evidence remains
  pending CI and task 9303 stays In progress.

- **Read-only native cleanup observation implemented:** root-only `observe`
  validates the actual prepared domain, reads a bounded no-follow protected
  event sample, rejects malformed/duplicate/unknown event fields, and
  revalidates domain and exact closing phase afterward. Its closed progress
  shape reports domain/closing/population/freeze without creating locks or
  records, issuing receipts or releasing ownership/capacity. Closing and
  observation share the unchanged prepared-domain validation. The genuine
  native case requires observation under an already held authority lock and
  unchanged directory listing; the inherited-descendant case samples actual
  populated-before-kill and empty-but-closing-after-kill states. Pure fixtures
  cover canonical boolean samples and malformed event shapes. Formatting and
  source checks precede frozen CI; complete manager fencing, closure issuance
  and shared runner/service integration remain outstanding, and task 9303
  stays In progress.

- **Independent evidence checks made reproducible:** adopted
  `verify_native_evidence.py` reads frozen AST inventories without executing
  them and verifies exact source/compiler, suite/case counts, report hashes,
  authority named runtime passes/counts/log hashes, I/O cancellation and the
  expected surviving-domain kill negative control. Replayed stable/MSRV
  artifacts for `2d487a3` each verify all 75 cases. Eight actual artifact-copy
  controls reject altered source, aggregate count, case success, log bytes,
  I/O test identity, negative-control population, production-backend scope
  and expected compiler; coherent updated report hashes do not conceal
  semantic failures. Copies are removed after each check. Executable digest
  syntax is validated, but artifacts omit executable bytes; the verifier
  explicitly reports `executable_bytes_verified: false` and never claims an
  independent binary-byte comparison. Newer runtime CI remains queued and
  full backend/runner acceptance remains incomplete; task 9303 stays In progress.

- **Production kernel submission native review passes:** frozen source
  `c9f877ea03b6eeae80a209f373af7d4b50d01a76` passes stable and MSRV native
  jobs in CI `37273243518`. Retained `ci-37273243518/{stable,msrv}` artifacts
  independently pass the committed verifier: exact frozen inventories and
  all 75 cases, source/compiler identity, report/log hashes, named authority
  passes/counts, unrelated-process survival, I/O cancellation and the expected
  live-domain negative control. Stable uses Rust 1.99.0; MSRV uses 1.89.0.
  The actual production request revokes admission and submits matched-domain
  freeze/kill; its genuine binding case enrolls two administrative sleep
  processes, verifies membership/population, observes both fail/reap and the
  group become empty while durable claim ownership remains and no closed
  evidence is published. This source predates the inherited-descendant test,
  manager probe and observation command; their pending runs cannot inherit
  this acceptance. Executable digest syntax is checked, with independent
  byte comparison explicitly false because artifacts omit binaries. Full
  manager fencing, closure issuance and shared runner/service integration
  remain outstanding; task 9303 stays In progress.

- **Portable capture import repair accepted on all six axes:** CI
  `37271018161` for frozen source
  `bfcfd79126df579f131f283238dfe781c2438458` is terminal success with all
  nine jobs passing: Linux/macOS/Windows at stable and MSRV, both native
  toolchains and zero dependencies. This resolves the earlier `dffc208`
  macOS/Windows unused-import compilation failures without dropping an axis
  or weakening the warning gate. The full portable verdict applies only to
  this source, which predates protected entry/publication, closing,
  termination, manager detection and native observation. Their later frozen
  source reviews remain separate; this regression acceptance does not
  complete task 9303 or enable contained execution.

- **Full handler contract fingerprint defined and implemented:** adopted
  config identity code/tests under task 9303 and added provisional
  `HandlerSpec::fingerprint` plus its public inventory entry. SPEC defines
  exact versioned full material, explicit parsed defaults, ordered argv/stamps,
  MCP tool/templates, timeout, both advances and sorted unique retry classes,
  hashed under `fsm:handler-contract:1` plus LF. Concurrency/manual hosting
  policy and substituted runtime arguments are excluded; hashing validates
  no manually constructed handler and authorizes no launch. An independent
  Python/SPEC process-default digest anchors the fixture, with explicit-default
  equivalence and field/template/payload/stamp-order/host-policy cases. Local
  Rust execution remains prohibited by the full-swap rule; compiled/public
  surface/native and portable evidence require a new frozen-source review.
  Shared runner and privileged catalogue wiring remain outstanding; this is
  not the sanitized public compatibility identity and task 9303 stays In progress.

- **Privileged contract approval wired into binding and grants:** root-only
  catalogue publication validates a root-protected source, wraps the table
  within cold-readable byte/depth bounds, and exclusively fsyncs it under the
  authority lock before any allocation. Current counter provenance and an
  otherwise fresh registration/counter/lock directory are required; missing
  catalogues refuse preparation and late/duplicate publication refuses.
  Binding replays the registered pending effect, selects its approved handler
  and matches full fingerprint/retry; grant argv must exactly equal approved
  substitution from journal-derived arguments. Native fixtures now provision
  the catalogue first, verify missing-catalogue refusal leaves counter/intent
  unchanged, and claim using the actual handler fingerprint. The genuine case
  refuses a changed command before grant publication and a separately durable
  real-domain claim bearing an unapproved fingerprint before binding; no
  binding/grant is published for that claim. Formatting/source checks pass;
  compiled/native/public-surface/full portable acceptance require a new frozen
  review. Broker access policy, real enrollment/group derivation, native I/O
  and full closure remain outstanding; task 9303 stays In progress.

- **Manager test compilation repair:** frozen source `ae738f9` failed both
  native jobs in CI run `37273504946` while compiling the authority test
  binary, before producing allocator evidence. Review found its retained-writer
  test passed a byte to `Write::write_all` rather than a byte slice; corrected
  that call and made the native producer include rendered JSON compiler
  diagnostics on build failure. Formatting, Python syntax and diff checks
  pass; the repaired source still requires independent compiled/native and
  portable CI evidence, and no runtime manager acceptance is claimed from
  the failed run. Task 9303 remains In progress.

- **Trusted enrolled-gate group derivation implemented:** the root-only
  `authorize-enrolled` operation accepts exactly a grant and no group override,
  retains fresh binding/approved-argv verification under the authority lock,
  and derives access from stable bounded manager/proc observations of the
  routed live DynamicUser gate. Exact installed root-protected executable,
  gate argv/cgroup, invocation/PID, UID/GID range, supplementary groups,
  privilege and capability restrictions are required. The manager query now
  shares its existing bounded capture/deadline implementation for exact
  property inventories. Pure negative controls cover identity/policy shapes;
  the real allocator case refuses a prepared claimed domain without an
  enrolled manager gate and confirms no grant/pending file appeared.
  Formatting/source checks precede frozen CI; positive installed-gate
  enrollment, launch, native I/O, broker policy and complete closure remain
  outstanding, and task 9303 remains In progress.

- **Repair CI reached runtime; descendant failure retained:** stable native
  job in run `37276994386` compiles frozen `086df77` and passes the four
  allocation/refusal cases, but the genuine binding case fails when its
  administrative descendant fixture exits before readiness. Its retained
  named log confirms that failure; no full/native acceptance is inferred.
  Fixture stderr now shares the already bounded nonblocking readiness stream,
  and an early EOF reports those retained bytes rather than discarding the
  child diagnostic. Runtime behavior is unchanged pending the next exact
  source review; task 9303 remains In progress.

- **Positive installed-gate native integration case authored:** authority
  inventory adds `enrolled_gate_authorization` (six authority cases; complete
  matrix becomes 76). The producer serially builds the real binary and test
  artifact, exclusively installs the frozen real binary at the fixed root
  path, records its digest and removes only the matching owned inode/digest;
  existing installations refuse. The case administratively starts the actual
  gate under DynamicUser, observes exact gate identity before any grant,
  publishes through the derived-group operation, requires approved `/bin/true`
  execution and confirms the journal claim survives with no closed receipt.
  Shared real-store claim setup retains the original binding/termination
  controls. Formatting, file-length, Python syntax and source review precede
  new frozen CI; no positive native pass, production launch transport or full
  runner acceptance is claimed, and task 9303 remains In progress.

- **Production gate startup implemented:** private root `launch` freshly
  validates binding/catalogue, exclusively fsyncs a bounded cold-readable
  single-submission intent, and starts only the protected installed routed
  gate through root-protected systemd-run with fixed DynamicUser isolation,
  control-group lifetime/kill policy, no restart/capabilities/escalation,
  cleared environment and directly owned pipe streams. Approved timeout plus
  entry wait bounds manager runtime; lock release after spawn permits grant
  and close operations. Command monitoring is finite and cleanup remains
  best-effort without closure evidence or ownership release. The positive
  native case now uses this production startup, refuses injected partial and
  existing complete intents without replacement, and checks the approved
  5100 ms ceiling. Formatting/file-length/source review precede frozen CI;
  native execution, broker transport, capture/result integration, manager
  fencing and permanent closure remain outstanding, and 9303 stays In progress.

- **Launch handoff race closed for accepted startup:** review found that
  returning immediately after utility spawn allowed closing to race pending
  manager submission. Startup now retains the authority lock through one
  shared two-second deadline for manager/proc handoff and protected exclusive
  handoff-record fsync; largest envelope size is reserved before intent.
  Failure best-effort revokes entry under the same lock and boundedly retires
  its owned utility, retaining intent/claims. Derived-group authorization
  requires the exact binding and freshly corroborated gate identity, syncing
  validated handoff/parent on replay before granting. Native controls refuse
  missing/changed handoff with no grant; the pure expired-deadline case refuses
  before manager spawn. Formatting/file-length/source checks precede frozen
  compiled/native CI; permanent manager fencing, uncertain-start recovery and
  full runner/closure integration remain outstanding, and 9303 stays In progress.

- **CI-driven retained-writer repair:** both native jobs in run `37277460229`
  fail frozen `bffeba4` at the first allocator case with the bounded manager
  query deadline; both named logs and reports are retained. Source review found
  the shared query refactor retained the `Command` object and therefore its
  original socket writers after spawn, preventing truthful EOF even when the
  child exited. Production capture now explicitly drops that object before
  polling, and a real `/usr/bin/true` case exercises this exact capture path
  without depending on manager availability. Startup also drops its command
  descriptor copies immediately. The deadline remains unchanged; formatting,
  source and file-length checks precede new frozen compiled/native CI. The
  earlier descendant readiness failure remains unresolved until execution
  reaches its retained diagnostic, and task 9303 stays In progress.

- **Prearmed startup admission refused:** review identified that a grant
  published before launch could bypass gate handoff verification. Startup now
  refuses either entry/pending path of any type under the authority lock before
  intent or manager submission. Native controls cover regular and dangling
  symlink paths, absent intent and unchanged empty population. Formatting and
  file-length checks precede frozen compiled/native CI; 9303 stays In progress.

- **Production stream ownership control authored:** the installed-gate case
  now passes real owned stdout/stderr socket descriptors through production
  startup, requires the approved quiet handler to exit successfully and both
  streams to reach truthful EOF within one shared two-second observation bound.
  It still requires unresolved journal ownership and no closed receipt. This
  exercises descriptor handoff/retirement rather than null output endpoints;
  formatting/file-length checks precede compiled native CI, with retained-pipe,
  noisy process/MCP capture and full runner closure still outstanding.

- **Persistence summary reconciled with accepted implementation:** SPEC's
  opening format table still named historical root/3, snapshot/5 and base/1
  as current. It now names the implemented root/4, snapshot/6 and base/2,
  preserving historical rows and their execution-quarantine boundary; the
  separate base-execution-claims/1 commitment is now listed alongside the
  unchanged base-index/1. This is
  documentation repair, not a format or hash change. Separately, retained
  MSRV evidence for `3ea6d5a` confirms real/test binaries compile and fixture
  installation reaches the first case, which fails with the known manager
  EOF timeout repaired later in `9e50756`; no positive gate or full native
  acceptance is inferred. Task 9303 remains In progress.

- **Matched manager stop implemented:** root-only `request-stop` durably
  revokes entry and retains the lock while matching protected binding/handoff,
  actual prepared domain and current manager invocation/isolation/lifetime/
  kill/no-restart policy. It revalidates native identity before fixed bounded
  replacement stop through the shared protected manager capture path. Failures
  preserve closing/claims; success issues no closure evidence or capacity
  release. The production gate case approves a bounded sleep handler, verifies
  actual exec, then stops it and requires failed transport exit plus unresolved
  ownership and no closed marker. Ungranted gates can exit on revocation before
  manager observation, so this control deliberately proves the live-handler
  stop path rather than treating that absence as completion. Formatting,
  file-length and source review precede frozen compiled/native CI; full manager
  fencing, permanent receipts and runner integration remain outstanding, and
  task 9303 stays In progress.

- **Retained native failure diagnosed:** stable evidence from `9e50756`
  passes preparation, three refusal controls and installed-gate authorization;
  the descendant helper then rejects its expected membership because Rust's
  `strip_prefix` yields a relative path and the fixture omitted the leading
  slash required by `/proc/PID/cgroup`. The expectation now restores that
  slash while preserving exact membership checks before descendant creation.
  This is a fixture repair, not relaxed containment; compiled/native reruns
  remain required and task 9303 stays In progress.

- **Manager completion made durable:** successful matched synchronous stop
  now exclusively publishes and fsyncs a protected bounded domain/binding/gate
  completion record; preexisting paths refuse replacement and publication
  failure remains uncertain. Complete record size/depth is checked before
  manager submission, and the running-handler native control requires exact
  completion material, preserved preexisting regular/dangling-symlink paths
  and a still-running handler after refusal, while requiring unresolved ownership and no
  closed marker. This records a closure cutpoint, not a closure receipt;
  permanent fencing/closure and runner integration remain outstanding.
  Both stable and MSRV retained `9e50756` evidence pass installed-gate
  authorization and fail the same descendant membership expectation corrected
  in `e95b4a2`; the repair workflow `37281659587` remains queued.

- **Matched closure publication implemented for review:** private root
  `complete-close` requires the protected prepared/closing/intent/handoff/
  binding/completed-stop chain, absence of grants, unit, queued job and native
  cgroup, unchanged boot/authority and retained admission fence. It publishes
  the domain tombstone followed by an immutable existing-format claim receipt
  through fsynced exclusive pending/hard-link ordering; exact replay preserves
  the receipt inode while different/partial records refuse replacement. The
  native running-handler control now requires missing/corrupt stop refusal,
  preserved partial receipt and claim, opaque store proof reading, replay,
  closed-launch refusal, real store publication of an interrupted stopped
  result from that opaque proof, cold replay retaining unresolved ownership,
  and monotonic successor allocation without settling the original claim.
  Source review corrected premature receipt visibility
  by fsyncing its pending inode before linking the final path. Formatting,
  file-size and diff checks precede compiled/native review; uncertain startup,
  natural completion, full Runner/MCP I/O and service integration remain
  outstanding, so 9303 remains In progress and production_backend remains false.

- **Natural exit retirement implemented for review:** source review found
  that live-domain `request-stop` cannot handle a handler whose unit/cgroup
  already disappeared. `complete-close` now verifies accepted one-shot handoff,
  durably revokes admission even after native exit, checks any present cgroup
  identity, and still requires absent unit/jobs/cgroup before distinct protected
  manager-retired evidence and closure publication. A present stop record must
  still match; no natural exit is relabeled a successful manager stop. Handoff
  admission now checks control-group lifetime/kill and no-restart policy.
  The production quiet-handler control requires receipt reading and replay
  with no stop record, unchanged unresolved/un-stopped journal ownership, and
  exact retired material; explicit-stop controls remain. Source checks precede
  stable/MSRV native review, with Runner/MCP integration and uncertain-start
  reconciliation still outstanding; task 9303 remains In progress.

- **Claimed authority execution path authored:** private root `execute`
  derives the complete approved invocation from a freshly verified claim and
  journal effect, launches both process/MCP through installed enrollment and
  grant, drains bounded live capture, and requires matching closure plus owned
  transport/MCP-worker retirement before an identity-bound candidate response.
  Linux native I/O adapters reuse the existing capture and owned protocol
  worker; provisional API inventory and guides move with that surface. Source
  review replaced source-file inclusion (whose worker tests depended on the
  library Runner) with an explicit shared adapter, avoiding duplicated workers
  or bypassed tests. Native controls cover exact-prefix, prefix-plus-one with
  independently calculated digest, hash-limit excess/no digest, timeout,
  closure proof, unresolved journal state and duplicate-launch refusal.
  Formatting/file-size/diff checks pass; compiled native process/MCP/tree,
  broker authentication, public service integration and uncertain-start
  reconciliation remain required, and task 9303 stays In progress.

- **Native MCP tree controls authored:** the installed-gate authority case
  now calls production `runner::execute` for both a tool-error answer and a
  withheld-answer timeout. Approved fixture Python servers leave child and
  grandchild processes holding inherited protocol streams; an independent
  observer requires their actual cgroup membership/dynamic UIDs and the
  recorded root gate PID before releasing the fixture barrier. Results must
  bind the original claim/hash, verify closure, retain unresolved/un-stopped
  journal ownership and refuse duplicate execution; error-answer stderr must
  preserve the exact prefix and independently calculated digest. Marker
  cleanup checks its exclusively created namespace identity and removes only
  known files, preserving unknown entries. Source review corrected umask
  filtering of its writable fixture directory and moved barriers to `/dev/shm`
  to respect unchanged DynamicUser strict protection, checked against upstream
  systemd documentation; both embedded fixture programs parse successfully.
  Formatting/file-size/diff checks pass; stable/MSRV native review remains
  queued, process early-root-exit observation, broker/service integration and
  uncertain-start reconciliation remain outstanding, and 9303 stays In progress.

- **Frozen native success retained; portable failure repaired:** run
  `37281659587` for `e95b4a2` passes both native jobs and zero dependencies;
  retained stable 1.99.0/MSRV 1.89.0 artifacts independently verify all 76
  cases against the frozen source/compiler, without claiming executable-byte
  comparison or production backend acceptance. Its Linux stable portable gate
  fails the retained-writer unit's assumption that a single nonblocking read
  must see EOF immediately after dropping the peer. The test now requires
  actual EOF within the same bounded two-second observation policy, preserving
  its pre-drop non-EOF and byte checks; exact-limit capture uses that bounded
  observation too. Other portable jobs remain live, so this is not full gate
  acceptance; later closure/execution/MCP changes still need their own compiled
  native review, and task 9303 stays In progress.

- **Early process-root exit observed for review:** the claimed runner now
  periodically reads manager main-process exit fields within its remaining
  handler deadline, matching invocation and the recorded original gate PID
  before selecting a normal/signal exit candidate. Unknown/mismatched data
  remains uncertain unless the owned transport has already supplied an actual
  reaped status; closure and handle retirement still gate every result. Pure
  controls cover live/normal/signal observations, canonical boundaries and
  mismatched/missing/extra identity fields. The independent native tree fixture
  now adds successful root exit with inherited child/grandchild streams and a
  child in another session, requiring zero status/capture/proof before its
  ten-second timeout. Source review also caught Python dictionary braces being
  reserved argv-template syntax; fixture code now uses `dict()` without changing
  the template contract, and both programs parse with no reserved braces.
  Separately, retained `ab68110` run `37283037348` passes native stable/MSRV;
  both artifacts independently verify 76 cases against source/compiler, covering
  matched closure publication, opaque proof/store stopped replay and monotonic
  successor allocation. This predates the claimed runner and does not establish
  full portable/runner acceptance or independent executable-byte comparison.
  Formatting/file-size/diff checks precede compiled/native review; broker,
  service integration and uncertain-start reconciliation remain outstanding,
  and task 9303 stays In progress.

- **Explicit runner cancellation authored; first claimed-runner proof retained:**
  private execution now accepts a shared cancellation flag, refusing observed
  pre-launch cancellation without intent publication, withholding an unissued
  grant when cancellation is observed after handoff, and selecting existing
  `exec/cancelled` semantics before collecting another candidate. In-flight
  cancellation uses the same admission revocation, descendant stop, immutable
  closure proof and owned transport/worker retirement; journal ownership is
  unchanged. Independent native process/MCP cases request cancellation only
  after checking actual root/child/grandchild enrollment and require matching
  proof, no retry classification, retained claims and duplicate-launch refusal;
  pre-cancelled controls require absent launch intent. Formatting, size, diff
  checks and embedded Python parsing pass; compiled cancellation acceptance
  remains pending. Frozen `a5f5366` run `37285594470` passes both native jobs;
  retained stable/MSRV artifacts independently verify all 76 cases against exact
  source/compiler, including process capture boundaries and timeout closure.
  These predate later MCP/tree/root-exit/cancellation changes and do not prove
  those changes, full portable acceptance or executable-byte equivalence.
  Broker/service integration, explicit host stop/drain and uncertain-start
  recovery remain outstanding, and task 9303 stays In progress.

- **Runner cleanup uncertainty controls authored:** independent process and
  MCP tree cases now replace only their fixture-owned protected handoff after
  actual root/child/grandchild enrollment and request cancellation. They require
  a cleanup-uncertain error, durable admission closing, removed grant, absent
  closed-domain record and refused file-verified receipt while the journal claim
  remains unresolved and unstopped; the corrupted record must remain unchanged,
  and duplicate execution is refused. Post-launch handoff read/missing-gate
  failures explicitly report cleanup uncertainty, including a fault racing the
  runner's first handoff read. Exact fault restoration followed by
  independent stop/complete-close is test cleanup only, never a passing runner
  result, and journal ownership must remain retained after it. Formatting, size,
  diff and fixture parsing checks pass; native stable/MSRV acceptance is pending.
  Run `37289090885` for root-exit commit `7b104fe` remains authoritatively queued,
  with all nine jobs present; it is not restarted or treated as completed.
  Broker/service integration, host stop/drain and uncertain-start recovery remain
  required, and task 9303 stays In progress.

- **Frozen MCP fixture failure diagnosed:** run `37286722581` for `95edb3`
  completes both native jobs with failure; retained stable/MSRV artifacts show
  catalogue validation rejected argv element 2 because Python dictionary braces
  were parsed as a noncanonical placeholder name, before MCP runner execution.
  This confirms the source-reviewed fixture repair already committed in
  `7b104fe`, which uses brace-free `dict()` while preserving the strict template
  contract. The failed evidence remains retained and is not relabelled passing;
  zero dependencies passes, remaining portable jobs are live, and the repaired
  tree/root-exit/cancellation/uncertainty source still requires its own native
  acceptance. Task 9303 stays In progress.

- **Production broker transport implemented for review:** private authority
  commands now explicitly provision one nonzero operator UID outside the
  dynamic-handler interval and serve a protected local endpoint. Startup holds
  lifetime leadership, verifies immutable configuration/current authority,
  checks bounded complete epoch history, burns the next epoch durably before
  socket creation, preserves old sockets and publishes a read-only atomic route
  only after operator-only socket permissions. Missing/pending/rollback state
  refuses startup. Canonical length-prefixed requests have bounded acquisition
  and response deadlines, fixed closed shapes and only prepare/bind/execute/
  close/observe actions, with no caller path, argv, grant or manager command.
  At most eight owned sessions dispatch existing production authority logic;
  executing sessions own their runner thread, detect client EOF/trailing data
  as cancellation, and observe join before returning/releasing capacity.
  Authority loss closes admission and requests cancellation while retaining
  leadership through owned session joins. Responses never settle the journal;
  oversized responses refuse with an ownership-preserving error. Pure protocol
  controls cover bounds, canonical framing, allocation aliases and policy
  refusal. Formatting/file-size/diff checks pass; compiled/native authentication,
  restart, disconnect and service/client integration remain required, with host
  stop/drain/recovery still incomplete; task 9303 stays In progress.

- **Broker permissions reviewed before native acceptance:** source review
  caught mode 0755 being filtered by the required restrictive server mask.
  Provisioning now explicitly applies traversal permissions only to its own
  new broker directory and refuses inaccessible existing authority ancestors;
  registration applies its intended 0755 to its own new namespace/authority
  directories while preserving preexisting namespace permissions. The chown
  sentinel UID is rejected, and socket type/operator owner/exact 0600 mode are
  checked before route publication. Formatting, size and diff checks pass;
  compiled native operator/authentication/restart/disconnect controls and
  service/client wiring remain required, and task 9303 stays In progress.

- **Native production broker access/restart controls authored:** the native
  authority inventory now adds `provisioned_broker_access`, running the actual
  installed production authority binary under UMask=0077 and separate clients
  that drop all supplementary groups and both root UIDs before route access and
  socket connection. The configured operator must prepare/bind/execute a real
  claimed process with matching receipt and retained unstopped journal ownership;
  duplicate execution and unknown policy actions must refuse. Another UID and
  a dynamic-handler-range UID must get actual EACCES. Controls also require
  exclusive server leadership with unchanged counter, restart with monotonically
  increasing epochs and preserved stale socket inode, and refusal of missing or
  rolled-back counter and missing epoch history without resetting authority.
  Fixture daemon ownership has bounded kill/reap cleanup; exact fixture faults
  are restored without turning their refused startups into passes. Formatting,
  file-size, diff and Python syntax checks pass; stable/MSRV compiled native
  acceptance remains pending. The earlier transport run `37291561643` is
  authoritatively queued with nine jobs present. Broker client-disconnect/tree
  proof, production client/service wiring and host stop/drain/recovery remain
  required, and task 9303 stays In progress.

- **Frozen process/MCP tree runner proof retained:** run `37289090885` for
  `7b104fe` completes both native jobs successfully, with zero dependencies
  passing and all six portable jobs still live/queued. Retained stable 1.99.0
  and MSRV 1.89.0 artifacts independently verify all 76 cases against exact
  frozen source/compiler, without executable-byte equivalence or gate release.
  This includes production MCP tool-error/withheld-answer tree cleanup and
  process early-root-exit with inherited child/grandchild streams in another
  session, original claim/hash/result checks, matching closure proof, retained
  journal ownership and duplicate-launch refusal, plus the repaired strict
  template fixture. It predates explicit cancellation, protected-handoff fault
  cases and broker implementation/controls; those still require separate native
  acceptance. This is not full portable/service/backend acceptance, and task
  9303 stays In progress.

- **Native client-death tree/worker controls authored:** the production native
  authority inventory adds `provisioned_broker_disconnect` for process and MCP.
  Independent clients drop root UID/group privilege, send one framed execution
  request and retain the socket until killed by the observer. Before that kill,
  the observer requires actual root/child/grandchild cgroup membership and
  dynamic identities, the recorded gate PID, populated domain and no receipt;
  the child has entered another session and descendants retain inherited pipes.
  Kernel EOF must then drive the installed broker's existing cancellation path:
  it stays alive, returns to one actual kernel thread, closes the native group
  and publishes matching immutable receipt/manager-stop binding/gate evidence
  within eight seconds, before the ten-second timeout. Grant removal, retained
  unstopped journal ownership and duplicate-execution refusal are required.
  Shared test barriers/server remain diagnostics, never authority evidence;
  owned client/daemon fixture cleanup is bounded. Formatting, size, diff and
  Python parsing checks pass; compiled stable/MSRV native acceptance is pending.
  First broker implementation run `37291349077` remains queued with nine jobs
  present; no restart is inferred from waiting. Public client/service wiring,
  host stop/drain and uncertain-start recovery remain incomplete, and task 9303
  stays In progress.

- **Supervised unprivileged transport helper implemented for review:** private
  `client NAMESPACE GENERATION` runs before the binary's privileged-operation
  check and authenticates its actual kernel UID against the configured operator.
  It validates protected immutable route shape, current boot/authority identity,
  epoch and exact socket type/owner/mode/inode before connecting, revalidates
  before dispatch and after response, and forwards only one bounded canonical
  closed-policy request/response through owned standard streams. Pure frame
  controls refuse zero/excess lengths, partial bodies and noncanonical JSON.
  Potentially blocking Unix connect/read stays in a host-owned killable process;
  this avoids assuming a Unix connect timeout or detaching unbounded connection
  threads, and helper death drives existing broker EOF cancellation. The host
  supervisor must supply startup/request/shutdown bounds; the helper alone is
  not a completed public client or service. Formatting, size and diff checks
  pass; actual native Rust-helper access and parent supervisor integration remain
  required. Cancellation run `37289550896` still has a live MSRV native job and
  remaining queued/running jobs, so no verdict is inferred from waiting; task
  9303 stays In progress.

- **Native Rust-helper controls authored; frozen cancellation proof retained:**
  valid broker access requests now drop root UID/groups then exec the installed
  production Rust `client` helper with a bounded framed request. Test responses
  require exact framed length; prohibited policy input must fail the helper
  without counter mutation, and direct unrelated/handler-UID socket controls
  still require kernel EACCES. Client-death process/MCP cases now kill that Rust
  helper, independently checking its actual operator UID and installed executable
  inode after tree enrollment. Formatting, size, diff and wrapper Python parsing
  pass; compiled native helper/broker acceptance remains pending. Separately,
  frozen `afe77d2` run `37289550896` passes both native jobs and zero dependencies;
  retained stable/MSRV artifacts independently verify 76 cases against exact
  source/compiler, including explicit process/MCP in-flight cancellation and
  pre-cancelled launch refusal. All six portable jobs remain live/queued, and
  executable-byte equivalence/gate release is not claimed. This predates later
  uncertainty/broker/helper changes; public supervisor/service wiring and host
  stop/drain/recovery remain required, and task 9303 stays In progress.

- **Public supervised native request adapter implemented for review:** Linux
  `run::native_client::NativeRequest` owns the fixed root-protected ordinary
  helper executable/process and nonblocking standard-stream sockets, with no
  connection thread or capture file. It validates canonical route/deadline and
  closed request policy before spawn, bounds pending/retained bytes and per-poll
  work, clears inherited overrides and drops Command descriptor copies after
  spawn so EOF remains truthful. Deadline checks precede request writes and
  response acceptance; failure/cancellation requests helper death while retaining
  its process handle. Canonical response collection requires successful actual
  reap and stdout/stderr EOF, with explicit cancel/reap methods; Drop remains
  bounded best-effort and no transport observation clears a journal claim or
  proves native closure. The provisional surface inventory records the module,
  type and four methods. Pure controls cover exact framing, partial/extra and
  noncanonical bodies, response shape, allocation aliases, policy refusal and
  retained-writer non-EOF/bounded diagnostic capture. Formatting, size and diff
  checks pass; compiled public-surface/native supervisor acceptance and production
  service claim/closure matching remain required, and task 9303 stays In progress.

- **Frozen uncertainty run diagnosed; handler-deadline race repaired:** run
  `37290017991` for `fa7ed2a` passes MSRV native and zero dependencies, but stable
  fails the real sleep handler's 100 ms timeout control because a root-status
  query exhausts the approved handler deadline and was classified as inspection
  uncertainty. Retained MSRV artifacts independently verify all 76 cases against
  exact source/compiler, including process/MCP protected-handoff fault refusal
  and independent restored-fault cleanup; failed stable evidence stays retained.
  Manager query deadline diagnostics are now centrally named, and the runner
  selects the existing timeout candidate only for that error with an actually
  elapsed handler deadline, still requiring full matched closure/reap/join.
  Earlier query deadlines, identity mismatch and other inspection errors remain
  uncertain, with deterministic pure classification controls. Formatting, size
  and diff checks pass; corrected stable/MSRV native acceptance and all six
  portable gates remain pending/live, no executable-byte equivalence or gate
  release is claimed, and task 9303 stays In progress.

- **Read-only opaque closure matching implemented for review:** task 9303
  explicitly adopts the store evidence/test files for the additive
  `VerifiedClosure::matches_claim` predicate. It compares receipt run ID, full
  native domain and original claim hash without journal mutation or asserting
  current ownership; the stopped mutator's writer-protected current-claim/hash/
  domain checks remain mandatory. Pure preauthenticated controls reject wrong
  hash/run/allocation and replaced authority/cgroup identities, preserving the
  unresolved unstopped claim; they are not native authentication evidence.
  Formatting, size and diff checks pass; compiled store/native result acceptance,
  executor candidate wiring and service integration remain required, with no
  journal format, hash domain or stable error change, and task 9303 stays
  In progress.

- **Claim-matched completion validation implemented for review:** provisional
  Linux `NativeCompletion` requires successful closed broker/result envelopes,
  the original immutable full claim/hash, its canonical derived receipt path,
  object acknowledgement candidate and known nullable failure class. It reads
  protected opaque closure evidence and requires exact run/domain/hash matching
  before exposing unchanged candidate, class and proof; it performs no stopped
  write or settlement, and current ownership must still be rechecked by the
  store under the writer lease. Pure controls reject stale claim, wrong hash,
  alternate receipt route, unknown class, non-object candidate and failed broker
  response before receipt I/O. The real provisioned broker access case now
  verifies its receipt through this production validator and checks proof and
  zero-status/no-failure preservation. The provisional inventory records the
  reexported type and four methods. Formatting, size and diff checks pass;
  compiled/native completion acceptance and public supervisor/service claim/
  stop/settle integration remain required, and task 9303 stays In progress.

- **Broker test compile failure repaired:** frozen `5915552` run `37291349077`
  fails both native jobs and both Linux portable jobs before runtime execution
  because the protocol policy test moves its BTreeMap into a JSON object and
  then attempts to mutate it again (E0382). Independent native and Linux stable
  job diagnostics identify the same source line. The second refusal control now
  starts from a fresh original request map, preserving both closed-shape/path
  refusal and prohibited-action assertions without touching production behavior.
  Zero dependencies passes; other portable jobs remain live/queued, and no
  broker/runtime acceptance is inferred from this failed build. Formatting, size
  and diff checks pass; corrected compiled native/portable gates and remaining
  production integration remain required, and task 9303 stays In progress.

- **Supervisor lifetime loss repaired and controls authored:** review found that
  the parent closed helper stdin after dispatch, allowing an orphaned helper to
  retain its broker connection after host death. Commit `47eafa4` retains that
  endpoint and uses private `client-watch`, which requires nonblocking stdin,
  bounds request receipt and checks lifetime EOF/trailing input while waiting
  for the broker response. Pure controls reject lost/trailing input before
  response consumption and permit response progress with a live owner. Native
  disconnect coverage now separately closes the lifetime endpoint without
  killing the helper for both process and MCP trees, requiring failed helper
  reap, complete matched closure and worker retirement before handler timeout,
  while preserving the unresolved journal claim. The four-tree case has a
  bounded 60-second outer watchdog; individual enrollment/closure deadlines
  remain unchanged. Formatting, size and diff checks pass; run `37298152815`
  is authoritatively queued for `47eafa4`, compiled native controls and actual
  public supervisor death/service integration remain required, and task 9303
  stays In progress.

- **Blocking helper transport lifetime gap repaired:** subsequent review found
  that response-read checks alone leave an orphan blocked in Unix connect or
  stdout writing. The helper now owns a nonblocking stdin watcher after bounded
  request receipt; loss/trailing input terminates the transport-only process,
  closing every connection descriptor without writing journal state or claiming
  closure. Ordinary return stops and joins that watcher, with no detached
  connection worker. The superseded read-loop lifetime controls are replaced
  by an incomplete-request deadline control; real process/MCP lifetime-EOF
  controls remain load-bearing and await compiled native execution. Formatting,
  size and diff checks pass; public host death and full service integration
  remain pending, and task 9303 stays In progress.

- **Actual public supervisor-death controls authored:** the native disconnect
  case now includes process and MCP hosts running the production `NativeRequest`
  adapter as UID 65534, using an exclusively copied frozen test executable in
  the test-owned protected authority directory. Before killing the host, an
  independent observer verifies host/helper executable identities, operator UID,
  enrolled root/grandchild membership and live population. Host SIGKILL must
  lead to matched domain closure and broker worker retirement before handler
  timeout; the helper must be absent or kernel-zombie dead, without claiming
  adopted-zombie reap or releasing the unresolved journal claim. The native
  case now covers six trees with a 90-second outer watchdog and a 300-second
  unrelated survival sentinel, preserving per-tree deadlines. Formatting, size,
  Python syntax and diff checks pass; exact-source stable/MSRV native execution,
  public service integration and remaining plan tasks stay pending, and task
  9303 stays In progress.

- **Independent supervisor retirement observations repaired:** review found
  that an stdout drain error prevented stderr draining and child reap polling.
  `NativeRequest::reap` now attempts all three observations before returning any
  error, retains actual child status and closes its lifetime endpoint after
  observed retirement, without turning failed stream reads into EOF or proving
  handler closure. SPEC explicitly requires independent progress. Formatting,
  size and diff checks pass; exact-source compiled/native acceptance remains
  queued, and task 9303 stays In progress.

- **Successful public supervisor completion control authored:** the native
  broker access case now executes its genuine bound claim through the copied
  unprivileged test host using production `NativeRequest`, rather than the
  plain client helper. The host polls bounded transport through actual reap and
  EOF, then validates `NativeCompletion` against its supplied original claim/
  hash and protected closure evidence, requiring unchanged zero-status candidate
  and no failure class. A unique bounded canonical response line lets the
  independent root observer retain its existing receipt/domain/journal checks.
  The death mode explicitly removes inherited completion material and receives
  no request argument, preserving the killed-host controls. Formatting, size
  and diff checks pass; compiled native stable/MSRV and all portable gates,
  public service admission/stop/settle wiring and remaining plans stay pending,
  and task 9303 stays In progress.

- **Claim-bound contained runner adapter implemented for review:** provisional
  Linux `NativeRun` owns the original durable claim/hash and one shared checked
  deadline, requires successful broker binding before execute, derives route/
  allocation from the claim and returns only matched `NativeCompletion`.
  Errors and explicit cancellation retain uncertainty and helper cleanup
  progress; no direct handler fallback or journal mutation is introduced.
  The native access host now exercises this adapter, and the independent root
  observer checks the actual protected binding and re-authenticates the receipt
  using a test-only reconstructed envelope, without claiming raw envelope
  round-trip identity. The public inventory and API/lifecycle contracts record
  the type and four methods. Formatting, size and diff checks pass; exact-source
  compiled/native stable/MSRV and all six portable gates, public service
  claim/admission/stopped/settlement integration and remaining tasks stay
  required, and task 9303 stays In progress.

- **Claim-bound runner refusal control authored:** before the successful native
  run, the unprivileged host now gives `NativeRun` a canonical SHA-256 hash with
  one original-record digest character changed. Broker binding must refuse;
  repeated poll must retain the refusal and explicit cleanup must observe real
  helper reap and EOF within bounded deadlines. The independent root observer
  requires exact NotFound for binding, launch intent, entry grant and handoff
  before the genuine hash is allowed to run, proving failed binding cannot
  dispatch execution or leave launch authorization. Test-mode environment is
  cleared before each host entry. Formatting, size and diff checks pass; run
  `37300307569` is authoritatively queued for `28bf92a`, current-source compiled
  stable/MSRV native and all portable acceptance remain pending, and task 9303
  stays In progress.

- **Protected admission verification tightened:** review found that fresh
  authority verification checked current runnable claim ownership but did not
  explicitly require enabled execution admission. It now refuses quarantined
  state before ownership/hash/catalogue authorization, so retained claims cannot
  independently authorize binding, launch or entry. The genuine-binding native
  case includes a read-only in-memory snapshot control preserving its full
  claim while changing admission to quarantine, requiring the exact refusal;
  its journal remains unchanged and subsequent genuine binding still succeeds.
  This guard control is not migrated-store quiescence evidence. Formatting, size
  and diff checks pass; run `37300579630` is authoritatively queued for
  `df72aed`, compiled current-source/native acceptance and public service
  integration remain pending, and task 9303 stays In progress.

- **Pre-dispatch claim-runner cancellation control authored:** the native
  unprivileged host now starts an authentic `NativeRun`, cancels before its
  first poll, requires a retained cancellation error and observes actual helper
  reap/EOF within two seconds. Before the refusal and successful controls, the
  independent root observer requires exact NotFound for binding, launch intent,
  entry grant and handoff, proving pre-poll cancellation dispatches neither
  binding nor execution. Each host entry clears inherited cancellation mode.
  Formatting, size and diff checks pass; run `37300899117` is authoritatively
  queued for `e79c7be`, current-source compiled/native acceptance and public
  service integration remain required, and task 9303 stays In progress.

- **Never-launched bound allocation closure implemented for review:** complete-
  close now has a separate branch only when launch intent is exactly absent;
  protected binding, durable revocation, no handoff/stop/retirement material,
  empty manager unit/job inventories and original empty/absent cgroup are
  required under the retained authority lock. It removes the original empty
  cgroup, rechecks retirement and protected records, and publishes the ordinary
  immutable matching receipt without fabricating launch or manager history.
  Cold retry after removal requires the durable revocation and repeats absence
  checks. The genuine-binding case now closes its cancelled never-launched
  allocation twice, authenticates matching proof and checks absent native
  history while the journal claim remains unresolved and unstopped. Formatting,
  size and diff checks pass; compiled/native acceptance and pre-binding/uncertain
  launch reconciliation remain required, and task 9303 stays In progress.

- **Never-launched closure partial-record review repaired:** the separate
  closure path now checks both final and pending launch/handoff/manager stop/
  retirement records before revocation and again before publication. The
  genuine-binding case injects a partial launch and pending submission markers;
  each must refuse without removing the original cgroup or publishing proof.
  A test-owned pending receipt obstacle then forces publication refusal after
  actual revocation/removal, with no authenticated receipt. Removing only that
  injected obstacle permits an independently verified cold retry and replay;
  the initial failed operation remains uncertainty and the journal stays
  claimed and unstopped. Formatting, size and diff checks pass; run
  `37301549061` is authoritatively queued for `ef81dbf`, current-source compiled
  native/portable acceptance and remaining integration stay required, and task
  9303 stays In progress.

- **Complete-close normative review reconciled:** the main SPEC operation
  paragraph still unconditionally required launch/handoff after the new
  never-launched exception was specified. It now explicitly requires those
  records for submitted runs, refuses partial/unreadable intents from selecting
  the exception, and states the separate final/pending absence, revocation,
  inventory and original empty-domain removal/cold-retry conditions together.
  Final publication still requires native absence and unchanged ordinary
  tombstone/receipt publication; no implementation or persisted bytes changed.
  Diff and size checks pass; run `37301794834` remains authoritatively queued
  for `7985174`, compiled native/portable acceptance and remaining production
  integration stay required, and task 9303 stays In progress.

- **Receipt publication replay ownership repaired:** replay previously synced
  an existing final receipt without checking a retained pending path. It now
  removes that pending link only after verifying the same protected regular-
  file inode and synchronizing it; another inode refuses and remains untouched.
  Native controls copy identical receipt bytes into a different pending inode
  and require refusal/preservation, then create the actual matching hard link
  and require cleanup while final bytes/inode and matching proof stay unchanged.
  SPEC records the ownership rule without changing receipt format or hash.
  Formatting, size and diff checks pass; run `37301992967` is authoritatively
  queued for `c4e0f93`, compiled native/portable verification and production
  service integration remain required, and task 9303 stays In progress.

- **Terminal nonretryable stopped result gap repaired:** review found that
  nullable retry classification could not encode an MCP protocol failure for
  atomic failed acknowledgement without wrongly allowing retry or treating it
  as interruption. Task 9303 adopts the shared outcome/ownership/test files and
  extends the unreleased VERSION 11 vocabulary with terminal `failed`; it may
  acknowledge while pending but cannot attempt or interrupt a still-pending
  effect. Pure controls round-trip stopped/settled state and refuse incompatible
  dispositions unchanged. Retry classes, historical bytes, hash domains and
  published formats stay unchanged. Formatting, size and diff checks pass;
  native completion mapping, compiled core/store/replay acceptance and remaining
  production integration stay required, and task 9303 stays In progress.

- **Native completion stopped semantics implemented for review:** checked
  completion now exposes `stopped_outcome`, preserving its exact candidate
  result while distinguishing `ok`, existing failure classes, cancellation
  interruption and terminal protocol `failed`. Candidate/class contradictions,
  unknown error/status forms and mismatched error/status combinations refuse
  before receipt verification; null retry class alone never implies success.
  Pure mapping/refusal controls cover process and MCP forms, and the native
  successful host requires `ok` with its unchanged candidate. The provisional
  inventory and API/lifecycle/SPEC contracts record the additive getter.
  Formatting, size and diff checks pass; run `37302421599` is authoritatively
  queued for `0aae3cb`, compiled native/core/store acceptance and writer-
  protected service stopped/settlement integration remain required, and task
  9303 stays In progress.

- **Verified native stopped persistence adapter implemented for review:**
  provisional Linux `Pipeline::stop_native` delegates the checked completion's
  exact outcome and opaque proof to the existing store stopped mutator, which
  retains writer-protected original/current claim/hash/domain checks. It performs
  no acknowledgement, retry, settlement or capacity release. The native access
  case requires stale-claim refusal with unchanged records/ownership, one valid
  stopped append, duplicate replay without another append, retained full claim/
  pending effect and reopened exact stopped outcome. The API inventory and
  contracts record the additive method. Formatting, size and diff checks pass;
  run `37302776458` is authoritatively queued for `19d3725`, compiled native/
  portable acceptance and production service scheduling/settlement remain
  required, and task 9303 stays In progress.

- **Atomic stopped settlement adapter implemented for review:** provisional
  `Pipeline::settle_stopped` delegates an explicit disposition and original
  immutable claim to the writer-protected store mutator, preserving current
  stopped ownership checks and request replay without consulting a changed
  handler table or launching code. The native access case reopens its stopped
  result, requires one atomic acknowledgement append, duplicate replay without
  another record, resolved claim/stopped ownership and removed pending effect,
  then verifies those facts after reopening. The inventory and contracts record
  the additive method. Formatting, size and diff checks pass; run `37303089625`
  is authoritatively queued for `4a712a8`, compiled acceptance, automatic service
  scheduling, retry/exhaustion selection and outcome-event recovery remain
  required, and task 9303 stays In progress.

- **Terminal failed store settlement controls authored:** the store fixture
  suite now persists a terminal MCP protocol candidate with `failed`, requires
  retry-attempt and pending-interruption refusals without journal/ownership/
  pending-state changes, then requires one failed acknowledgement preserving
  exact candidate bytes and no failed-attempt count. Full journal fold matches
  after stop and settlement, and cache-cleared request replay returns the exact
  original settlement without another append. These use preauthenticated fixture
  proof and do not claim native termination authentication. Formatting, size
  and diff checks pass; run `37303405014` is authoritatively queued for
  `51bedad`, compiled core/store/native acceptance and remaining production
  service integration remain required, and task 9303 stays In progress.

- **Native protocol-failure completion controls authored:** the approved MCP
  fixture now emits malformed JSON only after independently observed root/
  descendant enrollment and barrier release. The contained runner must close
  the full tree and return protocol failure with null retry class; production
  `NativeCompletion` must authenticate matching receipt and map it to terminal
  `failed` with the exact candidate. Every determinate existing tree mode now
  checks its completion mapping as well (`ok`, timeout, MCP error, interruption),
  while uncertainty stays excluded and journal claims remain unstopped and
  unresolved. Formatting, size and diff checks pass; run `37303611999` is
  authoritatively queued for `dd6d25f`, compiled native stable/MSRV and portable
  gates plus remaining production integration stay required, and task 9303
  stays In progress.

- **Handler-kind completion ambiguity repaired:** an empty candidate with null
  retry class could previously be treated as valid MCP success even for a
  process missing its exit status. The private transient result envelope is now
  `fsm.native-run-result/2` with approved catalogue-derived handler kind;
  completion refuses missing process status, process/MCP-only error mismatch,
  MCP process-exit candidates and unknown kind, preserving shared generic error
  forms. Material controls reject wrong/unknown kind, existing native tree modes
  verify the actual envelope, and the test-host reconstructed positive envelope
  explicitly records its process kind. No journal format, candidate bytes,
  stable error code or hash changes. Formatting, size and diff checks pass;
  run `37303803265` is authoritatively queued for `df17e8f`, compiled native/
  portable verification and remaining integration stay required, and task 9303
  stays In progress.

- **Immutable stopped disposition selection implemented for review:** pure
  `ExecutionState::settlement_for` requires exact current stopped ownership,
  selects interruption for absent effects/interruption, and chooses retry only
  for an admitted class with another attempt in the immutable claim policy;
  success, terminal failure, disallowed classes and final attempts acknowledge.
  It mutates nothing and does not bypass atomic settlement or backoff. Pure
  controls cover all these distinctions, missing stopped result, stale claim
  and unchanged state. Formatting, size and diff checks pass; run `37304098808`
  is authoritatively queued for `b7a8410`, compiled acceptance and service retry/
  exhaustion-result and outcome-event wiring remain required, and task 9303
  stays In progress.

- **Immutable retry exhaustion result preservation implemented for review:**
  checked stopped completion now applies the existing exhaustion error/class/
  attempt metadata only for a final attempt whose class is admitted by the
  original claim policy, preserving capture fields and the raw candidate getter.
  Disallowed classes, nonfinal attempts and unclassified protocol failure retain
  ordinary results; no current handler table supplies policy. Pure controls
  cover those distinctions and exact status/capture preservation. Formatting,
  size and diff checks pass; run `37304348653` is authoritatively queued for
  `bfe1893`, compiled/native exhaustion acceptance and automatic service/event
  recovery remain required, and task 9303 stays In progress.

- **Native immutable exhaustion control authored:** native fixture claims now
  derive retry policy from the actual approved catalogue instead of a hardcoded
  empty-class policy. The timeout MCP tree admits timeout with one total
  attempt; after independently observed descendants and full matching closure,
  checked stopped completion must preserve timeout status while adding existing
  exhaustion error/class/attempt metadata, and the raw candidate must retain
  its original timeout error unchanged. Other tree modes retain their ordinary
  candidate checks. Formatting, size and diff checks pass; run `37304747545`
  is authoritatively queued for `36afd94`, compiled native stable/MSRV and
  portable acceptance plus production service/event recovery remain required,
  and task 9303 stays In progress.

- **Pipeline native claim admission connected for review:** provisional Linux
  `Pipeline::claim_native` refuses unsupported native architectures and
  delegates complete prepared-domain requests to the existing writer-protected
  claim mutator, retaining admission, pending eligibility, unresolved ownership,
  contract, retry/backoff and request replay checks before any binding/launch.
  Native fixture admission now uses this production adapter with actual
  catalogue-derived fingerprint/policy and prepared identity. The inventory
  and contracts record the additive method; claiming launches no handler.
  Formatting, size and diff checks pass; run `37304982080` is authoritatively
  queued for `540e445`, compiled native/portable verification and automatic
  production service preparation/scheduling remain required, and task 9303
  stays In progress.

- **Native stopped-owner successor exclusion control authored:** after actual
  closure and pipeline stopped persistence, the native access case prepares a
  fresh successor allocation and attempts admission through `claim_native`
  before predecessor settlement. It must preserve the exact wrapped
  `store/execution_owned` refusal, unchanged records/full ownership, and absent
  successor binding, launch intent, entry grant and handoff; only the existing
  original settlement proceeds. This is sequential admission exclusion, not
  concurrent-executor/service-race acceptance. Formatting, size and diff checks
  pass; run `37305244263` is authoritatively queued for `0888613`, compiled
  native/portable verification and remaining full integration/races stay
  required, and task 9303 stays In progress.

- Added a second actual MCP timeout tree with two admitted attempts: the verified
  first-attempt result preserves `exec/timeout`, selects `Attempted` from the
  original claim policy, and passes through production stop/settlement adapters;
  duplicate settlement adds no record, ownership clears while the effect remains
  pending, and read-only reopening retains identical execution state.
  Formatting, size and diff checks pass; compiled CI is pending (latest prior
  head `400ddbc`, run `37305436845`, queued), and successor/backoff admission,
  automatic service integration and full gates remain required for task 9303.

- Extended the verified timeout retry case through production successor admission:
  a freshly prepared native allocation is refused one millisecond before the
  original ten-millisecond backoff expires without changing journal/state, then
  accepted at the deadline with attempt two, monotonic run identity and unchanged
  original fingerprint/policy; reopening retains the successor claim.
  This covers sequential retry admission, not concurrent executors or automatic
  service wiring; formatting/size/diff checks pass, compiled verification remains
  pending (`1f26028` run `37306152802` queued), and task 9303 stays In progress.

- Added native retry controls at the eligible deadline for changed handler
  fingerprint and changed attempt budget: the production admission adapter
  rejects each with `store/execution_contract`, preserves the retry ledger and
  journal length, and leaves successor binding/launch/entry/handoff absent;
  the original contract then succeeds with the same request key and clock.
  This proves sequential immutable-contract refusal after actual tree closure,
  not changed-table service recovery; compiled CI for prior head `b24e693` is
  queued as run `37306340523`, local formatting/size/diff checks pass, and
  production service integration plus full acceptance remain incomplete.

- Added provisional native helper/run progress snapshots for the service-facing
  cleanup seam: side-effect-free bounded facts distinguish actual helper reap
  and stream EOFs from verified original-claim closure; cancellation/refusal
  remain `Uncertain` after transport retirement, and only checked completion
  reports `Closed`, without exposing argv, captures or authority paths.
  The unprivileged native supervisor controls assert these distinctions and
  inventory/embedding docs describe them; formatting/size/diff checks pass,
  compiled acceptance remains pending (`fa86377` run `37306593261` queued),
  automatic service wiring remains incomplete, and task 9303 stays In progress.

- Added an actual native process root exiting 17 with enrolled child/grandchild
  and retained pipes: independent observation and existing protected closure
  precede checked `nonzero_exit` completion; the unadmitted class selects Acked,
  writes a failed outcome with the original capture through production adapters,
  duplicate replay adds no record, and reopening retains resolved ownership and
  an absent pending effect. This is sequential terminal settlement, not automatic
  outcome-event/service integration; formatting/size/diff checks pass, compiled
  CI remains pending (`35c0cfe` run `37306871810` queued), and task 9303 remains
  In progress with all remaining native/portable acceptance gates required.

- Added read-only `Store::current_execution_claim_hash` for production host
  binding/recovery: exact current full ownership is mandatory before using the
  existing original-record/authenticated sealed-index lookup; changed/consumed
  claims refuse as stale without writer acquisition or mutation. Native claim
  fixtures now use this API instead of assuming the last record is the claim;
  controls cover identity substitution, stopped/consumed ownership and read-only
  repeated sealed reopening. Task 9303 adopts this recovery seam without changing
  accepted historical formats; local formatting/size/diff checks pass, compiled
  CI (`d1908af` run `37307245166`) is queued, full integration remains incomplete.

- Added provisional `Pipeline::start_native` as the shared writer-held startup
  seam: durable writable storage, exact current full claim/hash, enabled
  admission, running/pending effect and no stopped result precede helper binding;
  the returned owned transport can be observed independently of writer access.
  Native controls refuse read-only and already-stopped ownership without a new
  launch; inventory/embedding docs describe retained uncertainty and remaining
  Root rechecks. Formatting/size/diff checks pass, compiled CI (`3c71852` run
  `37307522637`) is queued, automatic standalone/embedded/public-tick routing
  remains incomplete, and task 9303 remains In progress.

- Routed the positive unprivileged native host through `Pipeline::start_native`
  while holding an actual durable writer, checked no startup journal/state
  mutation, and dropped the writer before polling authenticated completion.
  The fixture provisions a separately reachable operator-owned store inside its
  exact test namespace, preserving registered inode identity and cleaning that
  store before its namespace; positive startup requires this path with no direct
  runner fallback, while existing wrong-hash/cancel transport controls stay direct.
  Explicit native fixture filesystem trait imports support the new provisioning.
  Formatting/size/diff checks pass; prior head `cab915e` run `37307820825` is
  queued, compiled native/full portable gates remain pending, and this is host
  adapter coverage rather than completed standalone/embedded/public-tick routing.

- Extended the positive native host with an independent unprivileged process
  holding the registered store's actual writer lease from before host polling
  through original-claim-matched completion; bounded readiness/release barriers
  check the writer remains alive, then release it before existing stopped and
  settlement stages. The fixture owns helper streams and bounds cleanup without
  treating writer/helper death as native closure. This checks observation under
  genuine writer contention, not timeout/contention or automatic service recovery.
  Formatting/size/diff checks pass; compiled CI (`b651c1f` run `37308300417`) is
  queued, all native/portable acceptance and full host integration remain pending.

- Review of the contention fixture found that spawning its independent writer
  after native helper startup consumed the helper's fixed request-frame budget;
  prewarm the test writer behind an explicit waiting barrier before native
  startup, then release it to acquire the lease after the original writer drops.
  Separate bounded waiting/lease-ready markers retain contention through checked
  completion without changing production deadlines or weakening assertions.
  Formatting/size/diff checks pass; `df39cb5` CI run `37308776012` is queued,
  compiled verification and full integration remain required, and task 9303
  remains In progress.

- Review of the prewarmed contention barrier found remaining pre-poll frame
  starvation and eager execute-helper creation after binding. NativeRun now
  exposes a `Bound` phase retaining the retired binding helper and starts/sends
  execution only on a later poll within the original deadline. The positive
  fixture sends binding before its lease barrier, then verifies no launch/entry/
  handoff exists after the independent writer is ready and before further polls;
  cancellation-before-first-poll remains unchanged. Inventory/embedding docs
  track the new phase; formatting/size/diff checks pass, `fda55c2` run
  `37309000534` is queued, compiled/full integration acceptance stays required.

- Startup review found that `Pipeline::start_native` accepted a writer poisoned
  by failed persistence. It now refuses poisoned journals before helper startup;
  the genuine native claim fixture induces an actual segment-rotation filesystem
  obstruction, verifies the failed acknowledgement leaves original ownership and
  pending work, then checks startup refusal and absence of binding/launch before
  removing only the owned obstruction and reopening unchanged execution state.
  Formatting/size/diff checks pass; `2570021` CI run `37309355616` is queued,
  compiled native/portable verification and full service integration remain
  pending, and task 9303 remains In progress.

- Extended native writer-held startup refusal controls with stale run identity
  and separate actual external acknowledgement/cancellation before binding:
  exact current ownership remains unresolved when pending work disappears,
  admission refuses without appending or creating binding/launch/entry/handoff,
  and read-only reopening retains the original claim. Fixture teardown removes
  only its empty test-owned domain and creates no production closure evidence;
  pre-binding reconciliation remains outstanding. Formatting/size/diff checks
  pass; `e8e412a` run `37309692370` is queued, compiled full native/portable gates
  and automatic service integration remain pending, and task 9303 stays In progress.

- Split native writer-held admission controls into `admission_native_tests.rs`
  along the existing refusal-review boundary, preserving stale/read-only/
  quarantined/poisoned and externally removed-work assertions and the same
  genuine-claim case entrypoint; no production or journal bytes changed.
  The allocator fixture is now 686 lines and the admission module 175 lines,
  both below the project ceiling. Formatting/size/diff checks pass; `4350b4a`
  CI run `37309923079` is queued, compiled gates remain pending, and no task
  acceptance or native gate status changes.

- Strengthened verified-completion contention evidence: the positive native host
  now confirms a real writer-open attempt returns `store/lock` while its
  independent writer remains alive, and read-only inspection retains exact
  unstopped ownership, pending effect and original journal head/hash after tree
  closure. Releasing that process then permits a healthy writer with identical
  ownership, retaining completion for the existing stopped/settlement stage.
  This proves observation and writer acquisition separately, not automatic
  service settlement; formatting/size/diff checks pass, `0fcfbd0` run
  `37310123746` is queued, and compiled/full integration gates remain pending.

- Admission review found `Pipeline::claim_native` allowed memory stores despite
  promising a durable pre-launch barrier. Native claim admission now requires
  the same supported healthy on-disk writer boundary as startup, refusing
  memory/read-only/poisoned journals before mutation; the generic pure/store
  memory APIs remain unchanged. Native controls submit original claim material
  to memory and read-only handles and verify `exec/mode` with identical state
  and record counts. Formatting/size/diff checks pass; `81a18fb` CI run
  `37310306538` is queued, compiled gates and full service integration remain
  pending, and task 9303 stays In progress.

- Source review of the admission-module split found redundant borrows of the
  newly borrowed `&str` effect argument; removed them while preserving all
  assertions and owned-string borrows in the separate external-removal fixture.
  This addresses strict Clippy readiness without changing production behavior,
  formats or acceptance status. Formatting/size/diff checks pass; `c861b4a`
  run `37310521292` remains queued, compiled/native/portable gates and complete
  plans 20–23 integration remain required.

- Re-polled frozen compile-fix run `37297046995` at
  `adb53e169bd5069476243a205277a858010cb6a5`: both native jobs and zero-deps
  succeeded, Ubuntu stable/MSRV and Windows stable/MSRV failed, macOS stable is
  in progress and macOS MSRV queued. Retained both native artifacts in the task
  cache and independently verified 78 cases with exact compilers
  `rustc 1.99.0 (b940084d7 2026-09-28)` and
  `rustc 1.89.0 (29483883e 2025-08-04)` against that frozen source;
  executable bytes remain independently unverified and the gate unreleased.
  This source predates the later lifetime, claimed-host and pipeline integration.
- Ubuntu stable's authoritative public-surface failure identified the sole
  inventory diff as observed `mod fsm_execute::run::native_client` versus
  declared `module ...`; corrected the current fixture to the scanner's `mod`
  vocabulary without changing the API or scanner. Formatting/size/diff checks
  pass; current pre-fix `054cd4b` run `37310659834` is queued, other portable
  failure classification and all current native/portable gates remain required.
  Local swap remains nearly full, so no local Rust build was started.

- Classified the remaining completed portable failures in frozen
  `37297046995`: Ubuntu MSRV job `111720629638`, Windows stable
  `111720629645`, and Windows MSRV `111720629679` each report only the
  public-surface target failing its inventory/byte-regeneration checks, with
  the same observed `mod` versus declared `module` native-client entry already
  fixed by `c98c4fe`; Ubuntu stable was classified previously. No broader pass
  is inferred from the failing jobs, and later APIs/native-host changes remain
  unverified. Current inventory-fix run `37311042697` at `c98c4fe` is queued;
  full six-axis/native acceptance and production integration remain required.

- Tightened the independent writer probe's release barrier to require actual
  stdin EOF instead of discarding the read byte count; stray release bytes now
  fail the control and the strict I/O-amount lint has an explicit observation.
  This changes only test barrier verification, not native closure authority or
  journal semantics. Formatting/size/diff checks pass; `a68eddd` run
  `37311223444` is queued, compiled native/portable gates and complete host
  integration remain outstanding, and task 9303 remains In progress.

- Re-polled `37298152815` at frozen
  `47eafa460c1ddc585c95d0920782304c865ea305`: native stable/MSRV and zero-deps
  succeeded, Ubuntu/Windows four jobs failed, macOS MSRV is in progress and
  macOS stable queued. Retained both native artifacts in the task cache and
  independently verified 78 cases against that source with exact stable
  `rustc 1.99.0 (b940084d7 2026-09-28)` and MSRV
  `rustc 1.89.0 (29483883e 2025-08-04)`; executable bytes remain unverified,
  native gate unreleased. Ubuntu stable log confirms the known `module`/`mod`
  inventory mismatch fixed by `c98c4fe`; other jobs are not inferred passing.
  This predates later lifetime death controls, owned watcher and host/pipeline
  changes, so it is primitive frozen evidence only; latest `79c5329` run
  `37311368249` is queued, full current native/portable integration remains
  required, and local swap is still nearly full with no Rust build started.

- Re-polled lifetime-EOF control run `37298399506` at frozen
  `1ee5976e63d017a18fb4cdcd064afcd945d4b898`: native stable/MSRV and zero-deps
  succeeded, Ubuntu/Windows four jobs failed, both macOS jobs remain queued.
  Retained both artifacts and independently verified 78 cases against that
  source with exact `rustc 1.99.0 (b940084d7 2026-09-28)` and
  `rustc 1.89.0 (29483883e 2025-08-04)`; executable bytes remain independently
  unverified and the native gate unreleased. This source includes process/MCP
  client-watch lifetime-EOF controls but predates the owned lifetime watcher,
  public host-death controls and later pipeline/service integration; no current
  or portable acceptance is inferred. Latest `7d58210` run `37311594997` is
  queued, full plans 20–23 requirements and task 9303 acceptance remain pending.

- Native CI review found owned-watcher run `37298657515` has both native jobs
  successful (artifacts not yet independently retained), while public host-death
  run `37299058913` has MSRV native success and stable native failure. Retained
  stable failure evidence: the broker-disconnect case stops at the copied test
  executable's 64 MiB size assertion before exercising supervisor death.
  Raised only this debug-test copy bound to 128 MiB, retaining exclusive create,
  source-length-plus-one copy validation, Root fixture ownership/permissions,
  fsync and independent process identity observations; diagnostics now include
  observed size and limit. Production limits and assertions are unchanged.
  Formatting/size/diff checks pass; repaired stable/current native and full
  portable gates remain required, and task 9303 stays In progress.

- Retained and independently verified 78 native cases for owned lifetime watcher
  source `f4ba7482f1b7cb0cfea4cf4d12e036d859001353` from `37298657515`
  on exact stable `rustc 1.99.0 (b940084d7 2026-09-28)` and MSRV
  `rustc 1.89.0 (29483883e 2025-08-04)`, plus 78 MSRV native cases for public
  supervisor-death source `f7b61d3b8a82db7c9056ffc0bc09cc61d4523c09` from
  `37299058913`. Artifacts remain in the task cache; executable-byte verification
  and native gate release remain false. The stable supervisor-death failure is
  retained separately and awaits the fixture-cap repair in `587620e`, whose run
  `37312292423` is queued. These frozen sources predate later NativeRun,
  stopped/settlement and writer-held host changes; current full native/portable
  gates, frozen review and automatic service integration remain required.

- Re-polled positive host run `37299691645` (`e90982135c47303f67fa95aa2cda425321c65688`)
  and initial NativeRun run `37300307569` (`28bf92a7854e75852aec9fe55be0121845cf6aee`):
  each has MSRV native and zero-deps success, stable native and four completed
  portable failures, with both macOS jobs queued. Retained and independently
  verified 78 MSRV native cases for each frozen source using exact
  `rustc 1.89.0 (29483883e 2025-08-04)`; executable verification and gate
  release stay false. Retained the NativeRun stable failure, confirming its
  broker-access case stops at the same 64 MiB copied executable assertion
  repaired by `587620e`, rather than demonstrating a handler/closure failure.
  This is scoped positive-host/bind-execute evidence only, predating current
  result schema, stopped settlement, writer-held startup and contention controls;
  repaired stable/current native, all portable gates and full service integration
  remain required, and no task is marked accepted.

- Corrected process candidate authentication: the owned child is the
  systemd-run launcher, so its exit code no longer selects a handler outcome
  or resolves failed root inspection; only invocation/PID-matched manager root
  status can select a process candidate before full closure and handle retirement.
  Added a real SIGKILL root control with surviving enrolled descendants, requiring
  signal status -1 rather than a launcher-dependent status; SPEC and lifecycle
  guidance now state this boundary explicitly. Latest run 37313133297 remains
  queued; formatting, size and whitespace checks are local-only validation,
  with compilation/native/portable gates still required and task 9303 open.

- Expanded the actual UID-65534 Pipeline/NativeRun host control to both
  successful /bin/true and timed-out /bin/sleep handlers, retaining an independent
  writer lease throughout bind/execute/closure observation in both cases.
  The timeout case requires exec/timeout, signal status -1, Timeout class and
  authenticated original-claim closure while store/lock remains observable,
  the durable claim/effect stay pending and no stopped result is written;
  after lease release it runs the existing stop/replay/successor-exclusion and
  atomic terminal settlement/reopen controls. No retry class is admitted in
  this fixture, so timeout is terminal without fabricated exhaustion metadata.
  Source formatting/size/whitespace checks pass; new native CI remains required,
  latest frozen fix run 37314402122 is queued and task 9303 remains in progress.

- Strengthened both positive-host writer-contention controls with an actual
  second UID-65534 process attempting Pipeline.claim_native for the same effect
  against a distinct, Root-prepared native domain before releasing its ready
  barrier. It requires store/execution_owned through exec/store, unchanged full
  state/record count/journal head/original claim hash and no competing domain
  binding/launch/entry/handoff artifacts; the second process then retains the
  real writer lease through the first host's success or timeout closure.
  The later stopped-owner exclusion check reuses that same prepared domain,
  and fixture cleanup independently tracks both domain identities. This extends
  claim-exclusion evidence without claiming completed standalone/embedded
  service integration or the full live-tree race gate. Formatting, size and
  whitespace checks pass; native execution remains pending, with current run
  37314803628 queued, and no task acceptance changes.

- Added pure original-contract material/recovery APIs without changing the
  existing fingerprint bytes or hash domain: contract_value exposes exactly
  fsm.handler-contract/1 material and from_contract requires closed canonical
  fields, explicit defaults, existing JSON/handler bounds and the caller-held
  original fingerprint. Process/MCP round-trip and substituted-material refusal
  controls cover literal templates, both advances, payloads/stamp order and retry;
  public inventory and SPEC/API/lifecycle contracts are updated, including the
  secret-bearing material boundary. Durable transport/storage and restarted
  outcome-advance recovery remain unimplemented, so this is an integration
  prerequisite rather than task completion. Formatting/size/whitespace checks
  are local validation only; CI/compiler/native/portable acceptance is pending.

- Advanced the unreleased private native result to /3 with the complete
  catalogue-derived handler_contract, retaining unchanged candidate/stopped
  acknowledgement shapes and historical journal/hash/receipt bytes. Completion
  now validates the canonical contract against the original claim fingerprint,
  retry snapshot and kind before reading native receipt evidence, and exposes
  that checked handler through the provisional API. Unit controls reject older
  envelopes and valid-but-substituted contracts; actual process/MCP native
  fixtures require the original contract to survive collection/re-authentication.
  SPEC/API/embedding/lifecycle and inventory are updated. Durable original-contract
  storage and restart outcome-event replay remain required; this is not task
  acceptance. Local source checks pass; frozen run 37315680969 remains queued
  and compiler/native/all portable gates are still pending.

- Added durable original native-completion retention: after verifying the full
  /3 envelope and closure, Root creates completed-ALLOCATION-RUN.json once with
  mode 0600, fsyncs file and parent, and only then returns the result. Completed
  records have an explicit 64 KiB bound; existing control records stay at 8 KiB.
  The read-only allocation-only broker recover action validates protected
  binding/authority identity and the original recorded completion without
  catalogue lookup, writer acquisition, record repair, launch or settlement.
  Actual native controls require identical result recovery with the catalogue
  temporarily absent, preserve/refuse a torn fixture record, restore only that
  fixture-owned file, and recover unchanged after broker restart and journal
  settlement, for both success and timeout; process/MCP root fixtures also
  require recorded recovery. SPEC/API/release/embedding/lifecycle are updated.
  Local formatting/size/whitespace checks pass; native/compiler/platform gates
  and service original-advance recovery remain required, so no task is accepted.

- Wired protected completion recovery into the owned client: NativeRun::recover
  starts in the new Recovering phase, requests only recover and shares original
  identity, deadline, helper reap/EOF and single-delivery checks; refusal never
  transitions to bind/execute. Pipeline::recover_native accepts a supported
  durable unpoisoned read-only snapshot and rechecks exact current claim/hash
  independently of writer access or launch eligibility. Actual unprivileged
  success/timeout host controls require identical original completion recovery
  while the independent writer lease remains held, plus missing-record refusal
  with unchanged ownership and retired-but-Uncertain helper state. Inventory and
  SPEC/API/embedding/lifecycle contracts are updated. Local source checks pass;
  compiler/native/all portable acceptance and automatic service recovery are
  still required, with previous durable-record run 37316870974 queued.

- Added non-writing Store::replay_execution_settlement to recover an original
  terminal settlement under its exact full-claim/disposition request fingerprint
  without claiming missing keys, taking a writer or consuming ownership.
  Missing original fingerprints refuse; existing request conflicts and carried
  sealed-outcome refusals remain explicit. Actual success/timeout native fixtures
  require an unclaimed pre-settlement lookup to leave ownership/records intact,
  read-only reopened replay to match original run/result, changed disposition
  refusal and unused-key/state/head preservation. SPEC/API/embedding document
  the boundary; recovered outcome-event integration remains the next step,
  including stronger sealed-history support. Local source checks pass, with
  full compiler/native/portable and high-risk acceptance still pending.

- Added writer-held Pipeline::advance_native_settled: checked completion now
  retains its exact original claim, and advance requires exact Acked settlement
  replay plus matching committed disposition/instance/effect/run/outcome/result
  before selecting the recovered original on_ok/on_failed. Existing event_rid,
  enablement and acknowledgement-before-event/sequence retry are preserved;
  missing, substituted, conflicting or unavailable evidence cannot send an event.
  Actual success/timeout fixtures declare distinct original events, refuse an
  advance before acknowledgement, reopen after settlement and restart the broker,
  remove only the fixture catalogue, re-verify recovered completion, reject a
  stale claim and require the original docs_ok/note_added event exactly once;
  repeated calls preserve journal count/state and cannot send withdraw.
  Inventory and SPEC/API/embedding are updated. Local source checks pass;
  native/compiler/portable acceptance, automatic service routing/enumeration and
  stronger sealed-history recovery remain required, so no task is accepted.

- Added Pipeline::settle_native_stopped to require a healthy durable writer,
  exactly retained original claim/hash, matching closure and a persisted stopped
  outcome before selecting disposition from the original policy/current pending
  state and atomically consuming it. Acked/Attempted retain existing ack_rid /
  attempt_rid; pending Interrupted uses a run-specific key, consumes no ack and
  sends no event. Actual success/timeout and nonzero-exit native controls now use
  automatic original-policy settlement, with pre-stop refusal and exact derived-key
  replay; retry-timeout controls require Attempted/backoff behavior, while actual
  process/MCP cancellation controls require Interrupted, pending-effect/zero-failed-
  count preservation, no ack/event and reopened replay. Inventory and SPEC/API/
  embedding are updated. Local source checks pass; full compiler/native/portable
  gates, automatic service routing and stronger sealed recovery remain required.

- Source review found and fixed a client admission defect: the provisioned
  broker allowed recover, but NativeRequest's separate closed action allowlist
  omitted it, preventing the new owned recovery constructor from starting.
  Both now admit only canonical positive-allocation recovery; a direct client
  policy control rejects paths, zero/noncanonical allocations and launch aliases.
  The review also found Value serialization before limits in new contract and
  native request/completion validation. A task-adopted private helper now charges
  existing depth and exact canonical bytes before allocating/serializing;
  escape/Unicode/punctuation boundary and deep/oversized controls preserve hash
  bytes and envelope limits, with contract integration refusal coverage.
  SPEC/API are updated. Local formatting/size/whitespace checks pass; current
  run 37319670969 remains queued, so no compiler/native/portable acceptance or
  previously queued recovery runtime success is claimed.

- Added typed owned NativePreparation for the domain-before-claim sequence:
  it requests only prepare, validates returned NativeDomain against the original
  namespace/generation and delivers once after helper success/reap/EOF within
  the original deadline. Identifier-free Preparing/Prepared/Uncertain progress
  grants no claim, closure or handler entry; cancellation/refusal has no fallback.
  Actual UID-65534 positive-host fixtures now prepare through this client before
  Root observes the empty domain and performs the original durable claim; a
  pre-poll cancelled request must retire Uncertain without consuming allocation
  1, and the later success/timeout claim/run controls reuse the same protected
  supervisor copy. Route/refusal/missing-domain unit controls and public inventory
  plus SPEC/API/embedding are updated. Local source checks pass; latest run
  37320800330 remains queued and automatic host routing plus full gates remain
  incomplete, so no task acceptance changes.

- Physical-store receipt review found that a copied unresolved journal could
  otherwise authenticate an original namespace's closure using identical logical
  claim/hash values; registration now publishes separate protected immutable
  physical identity metadata, opaque closure/quiescence proofs retain it, and
  stop/admission writes check the current directory before appending.
  A disk-copy fixture requires unchanged records/head and retained ownership on
  refusal, while the original directory accepts the same stopped proof.
  Historical journal/receipt bytes and hash domains remain unchanged; missing
  metadata refuses rather than being inferred or repaired, and old provisioned
  namespaces need reviewed reprovisioning.
  Source checks pass; compiled/native CI and frozen review remain pending,
  so task 9303 stays in progress and plans 20–23 remain incomplete.

- Added an owned NativeExecution host component retaining original run/checked
  completion independently of the writer, with bounded metadata-only progress,
  cached observation, healthy-writer stop/settlement application and exact
  original settlement replay before slot release; the direct native settlement
  pipeline now also checks the protected proof's physical store identity.
  Actual unprivileged success/timeout fixtures now start through the owned host,
  and recovery fixtures exercise cached observation during an
  independent writer lease and read-only application refusal without releasing
  capacity; Root success/timeout fixtures adopt checked completion, consume an
  already durable stopped result and require repeated exact replay with no new
  records; a previously unstopped process success requires the owned host to append
  stop then settlement, while runner failure/retry/interruption controls exercise owned
  settlement and freshly reconstructed original-ledger replay after consumption. SPEC/API/embedding/inventory describe the provisional component.
  Source checks pass; run 37325435831 has failed completed jobs with macOS
  still queued as recorded below, and production service/scheduler/
  public-tick routing and full native/portable/frozen gates remain incomplete,
  and task 9303 stays in progress.

- CI review of frozen physical-store commit 2aec1f4, run 37325435831,
  found native-fixture compile errors: imports missing from typed preparation,
  StoreState compared with unsupported PartialEq, pending String slices compared
  to &str via contains, and successful recovery asserted in an uncertain branch.
  Fixes use the existing complete store_states_eq helper and typed iterator
  comparisons, import the in-tree parser/limits, and require uncertain recovery
  to refuse. Native stable evidence independently failed the systemd kill probe:
  the manager reported inactive and cgroup populated 0, but the original domain
  still existed; the absence gate is preserved and this remains unaccepted.
  Completed Ubuntu/Windows and native jobs failed, zero-deps passed, and both
  macOS jobs remain queued; no failed/queued job was restarted or canceled.
  Source checks pass, compiled/native verification remains pending.

- CI review of owned-host commit 03b82cf, run 37327572524, confirms stable
  systemd/identity/broker/window/signal/restart/facility primitive suites passed,
  then the independent older store-evidence fixture refused its first receipt
  read because it lacked newly required physical-store registration metadata.
  Its Root binding fixture now captures the physical store identity before
  handler launch and publishes the same separate protected metadata; the driver
  verifies canonical bytes/Root ownership/0444 mode plus missing, torn, symlink
  and writable metadata refusal without repairing anything during proof reads.
  The frozen-source verifier requires the new physical identity/refusal evidence
  for sources declaring it, preserving historical sources without that requirement;
  the completed portable gate exposed one remaining StoreState comparison in
  the store unit fixture, now corrected with its existing complete comparison helper.
  The systemd probe now waits at most three seconds for actual original cgroup
  removal after manager stop, rejecting a replaced identity or still-present
  domain; inactive/populated-0 observations alone still cannot pass closure.
  Task 9303 adopts these fixture/probe changes without changing accepted 9302
  journal formats; source formatting, file-size/diff checks, Python syntax and
  bounded removal/refusal controls pass; the enhanced verifier still validates
  frozen f4ba7482 stable/MSRV reports (78 cases each), without claiming those
  historical reports verify the new source or executable bytes. Full current
  compiled/native/portable verification and production routing remain incomplete,
  with no task acceptance changes.

- Reviewed the current production service launch seam: it still starts legacy
  direct children before writer acquisition, so automatic routing is incomplete.
  Added Pipeline::claim_native_handler to derive instance/effect identity by
  journal replay and validate one original HandlerSpec, fingerprint and retry
  snapshot under a healthy durable writer before returning current ownership;
  it starts no helper and cannot recreate a consumed claim from a duplicate key.
  Native allocator/runner/broker fixture claim preparation now uses this typed
  production admission seam, requiring wrong-effect and invalid original-policy refusal before request-key
  allocation with unchanged records/full state, then an exact original claim
  and duplicate typed admission with no new records or changed original hash.
  SPEC/API/embedding/inventory record prepare → writer claim → launch recheck.
  Source checks pass; CI run 37328653105 passed primitive suites including
  the corrected evidence bridge, then failed authority catalogue setup because
  its fresh-registration allowlist omitted store-identity.json; that confirmed
  follow-up is being corrected separately, and full gates/task acceptance plus
  production host routing remain required.

- Fixed the authority setup failure observed in frozen 853a3a7 CI run
  37328653105: catalogue freshness now recognizes the public store identity
  registration record, first requiring its Root ownership/0444 regular-file
  protection, closed canonical format and exact private registration identity.
  A nonzero counter or any other unexpected file still refuses used authority.
  Native fixture setup requires missing, writable, torn and mismatched public
  metadata to refuse before catalogue/allocation publication, then restores only
  its exact fixture-owned bytes and verifies counter 0 before successful setup.
  No published journal/receipt/hash bytes change; source checks pass, full
  current native/portable/frozen acceptance and production routing remain pending.

- Native result authenticity review found that closure proof alone authenticated
  original run/domain/hash but did not bind the returned candidate bytes, so an
  altered response could pass identity checks after true closure. Root completion
  publication now creates a separate protected immutable digest attestation of
  the entire bounded response before private completed-record publication;
  NativeCompletion requires the original authority/run/claim and exact digest.
  The new private attestation/hash domain preserves historical closure/journal
  bytes and exposes no full contract or captures. Broker fixtures require altered
  output to fail attestation despite original closure, and missing/torn/symlink/writable
  metadata to refuse until exact fixture-owned bytes are restored. SPEC/API/
  embedding describe crash refusal and the unreleased boundary; source checks
  pass, with full native/portable/frozen verification and production routing
  remaining required and no task acceptance changes.

- CI review of dac1a37, run 37330135992, found the new freshness fixture
  compared its full counter Value to integer 0; the assertion now extracts the
  checked last_allocation field with the existing native numeric reader.
  Stable/MSRV native authority builds and Ubuntu gates failed on this single
  type mismatch before runtime authority evidence; zero-deps passed, other
  portable jobs remain active/queued, and no run was canceled or restarted.
  Formatting/file-size/diff checks pass; compiled/native verification remains
  pending and task 9303 is not accepted.

- CI run 37332358645 at 2a5af16 failed stable native authority execution:
  a fast head handler lost its original manager status before Root observation.
  Launch now retains successful/failed invocation status until matched Root stop,
  without deriving candidates from launcher exit. Stop verifies protected handoff
  and original manager policy before revocation even after natural cgroup removal;
  clearing retained failure requires original invocation and ExecMainPID after stop.
  Full independent closure requirements remain unchanged. Native controls add fast
  success/nonzero/signal exits and retained-unit closure refusal; source checks pass and
  current compiled/native/portable/frozen acceptance remain pending, with task
  9303 still in progress and automatic production routing unfinished.

- The Ubuntu portable gate in run 37332358645 also found the contract recovery
  fixture compared declaration-order retry classes against the contract's sorted
  set. The fixture now compares the full recovered handler against an original
  clone with only retry classes normalized, and separately checks unchanged
  fingerprint and exact contract bytes; argv and outcome stamp order remain
  covered by full structural equality. Production bytes and semantics are unchanged;
  formatting/file-size/diff checks pass, compiled gates remain pending.

- Frozen 412cca9 CI run 37334952656 failed both native jobs in the independent
  systemd feasibility probe before authority execution: its exit case observed
  an inactive unloaded manager and original populated-0 cgroup persisting beyond
  three seconds. Submitted closure and the probe now explicitly remove only an
  original protected empty residual domain after durable revocation and manager
  unit/job retirement, repeating identity and retirement checks before removal
  and proving actual absence afterward; recursive cleanup is forbidden and any
  uncertainty still refuses receipt publication. SPEC/API/embedding/release
  record the cleanup boundary. Actual kernel controls exercise no-revocation,
  changed identity, active manager and unknown-child refusal before original
  empty cleanup, with no receipt or unrelated-domain deletion. Local authority
  tests pass 20 with 13 provisioned cases ignored, handler identity tests pass
  all 7, and probe inventory controls plus formatting/file-size/diff checks pass;
  provisioned native/portable/frozen gates and task 9303 acceptance remain pending.

- Local stable clippy review found nested candidate/authority checks, needless
  borrowed effect identifiers and forbidden println macros in native test
  subprocesses. Conditions now use the MSRV-supported let-chain form, test
  barriers use an explicit fallible stdout writer, and borrowed arguments are
  corrected without changing marker bytes or execution semantics; all-targets
  execute clippy now passes with warnings denied, as do format/size/diff checks.
  Native run 37336060083 passed residual-cleanup refusal controls, then found
  missing live-domain revocation on the corrupted-handoff path; that distinct
  regression is under repair and no task acceptance or production gate changed.

- Corrected the corrupted-handoff regression from e34e10d CI run 37336060083:
  stop durably revokes a present verified original cgroup before inspecting
  handoff material, while absent-domain revocation retains exact completed
  handoff and manager-policy requirements. Runner stop refusal now also attempts
  original-identity kernel freeze/kill before transport retirement, without a
  writer, successful closure inference or claim release. Native uncertain cases
  require durable closing, revoked entry and an empty/absent original tree while
  corrupted handoff bytes and unresolved journal ownership remain unchanged;
  repair/recovery still require full matched proof. SPEC/API/embedding/release
  document both revocation paths. Local all-target execute clippy passes with
  warnings denied, authority unit tests pass 20 with 13 provisioned controls
  ignored, and format/size/diff checks pass; native/portable/frozen acceptance
  and production host routing remain pending with task 9303 in progress.

- CI run 37336883860 at 1755013 passed the native enrolled runner suite,
  including fast exits, descendant/MCP cleanup and corrupted-handoff fencing,
  then failed the separate before-binding admission control: cancellation
  retains pending effects with Cancelled status, while acknowledgement removes
  the effect. The fixture now asserts these distinct durable states before and
  after reopen and still requires native startup refusal with no helper records
  and unchanged original ownership. No production semantics or bytes changed;
  workspace all-targets stable clippy passed before this fixture correction,
  with current native/portable/frozen gates and task 9303 acceptance pending.

- Native run 37337395680 at 85b0194 passed the corrected cancellation control
  and then found exercise_binding deleted its fixture before its caller's
  one-shot closure fault cases. Cleanup now occurs once, after all parent
  assertions and after releasing the read-only snapshot, preserving the same
  original kernel domains and protected records through every fault/retry case;
  no production behavior changes. The frozen local stable workspace debug gate
  at 85b0194 completed with two failed CLI targets (session marker parsing under
  serial libtest and Git fixture commits rejected by the local global hook),
  with all other targets completed; those distinct harness repairs follow.
  Native closure acceptance, current full gates and task 9303 remain pending.

- Repaired the two failed targets from the frozen 85b0194 local stable debug
  gate without changing production behavior: the fallback child begins markers
  on a fresh line under serial libtest and its exit must succeed before counting
  all 1,000 IDs; regeneration fixtures use Conventional Commit messages and
  assert every Git setup command succeeds, preserving the user's global hook.
  Task 9303 adopts these isolated gate-harness footprints. Under the same
  serial/warnings-denied settings, http_session passes 9 with its subprocess
  helper ignored in the parent inventory, machine_test_regen passes all 20,
  and CLI all-targets clippy plus format/size/diff checks pass. A current frozen
  full debug/release rerun, native/portable gates, production routing and task
  acceptance remain pending; subset repairs are not full acceptance evidence.

_Task frontmatter remains authoritative; registration does not release the native gate._
