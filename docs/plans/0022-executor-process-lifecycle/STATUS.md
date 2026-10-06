# Plan 0022 — Executor Process Lifecycle — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and the integration coordinator owns
lifecycle updates.

- **Status:** Registered by hand; native prerequisite complete;
  durable execution persistence and contained runner complete; ownership integration started.
- **Goal:** prevent a successor from overlapping a surviving local handler
  tree, with bounded shutdown and evidence-based restart for both process and
  MCP handlers.
- **Root cause:** direct-child handles and short-lived writer locks do not
  contain descendants or persist execution ownership across executor death.
- **Approach:** resolve the native containment prerequisite, journal claims
  before launch, prove tree closure before reuse, and drive every execution
  host through the same shutdown and recovery protocol.
- **Progress:** 3/7 tasks done; 0 blocked; 0 dropped; ownership integration In progress.
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

- Frozen 7d2c089 local stable workspace debug gate passes in full with serial
  tests and compiler/rustdoc warnings denied; its cache log records every
  workspace target and no failures. CI run 37339071702 passed all earlier
  native authority controls, then broker access failed with connection refusal:
  lifetime guard reread the live catalogue, retiring the broker when recovery
  deliberately removed it. Protected configuration/authority/boot/lock checks
  now remain independent of current catalogue availability; provisioning and
  new allocation/binding/execution still validate approval. Native controls add
  malformed-catalogue recovery, no allocation on missing/malformed approval and
  restart without the catalogue. SPEC/API/embedding/release record the private
  guard boundary without changing journal/configuration/receipt/hash bytes;
  workspace all-targets stable clippy with warnings denied and format/size/diff
  checks pass. Source review confirms provision, allocation, binding and entry
  retain separate approval checks, while original recovery uses attested private
  completion and full original proof. Current frozen debug/release/native/portable
  gates and production routing remain pending with task 9303 active.

- Frozen 181cbad local stable workspace release tests pass in full with serial
  tests and compiler/rustdoc warnings denied; CI run 37340474722 independently
  passes both provisioned native toolchains, each verified against its exact
  source/compiler with 78 cases, executable bytes unverified and gate unreleased.
  Ubuntu stable and zero-dependency jobs pass; other portable axes remain pending.
  Review found authenticated broker close lacked the runner's fallback fencing
  on matched-stop refusal; both now share the same original-domain helper while
  retaining independent full closure and the original stop error. Native fault
  controls require revocation and depopulation/actual absence without fabricated
  manager completion, changed damaged handoff or ownership release. This source
  change requires fresh gates; task 9303 and production routing remain pending.

- Frozen 4afca71 local stable workspace debug tests pass in full with serial
  tests and compiler/rustdoc warnings denied. Review strengthens the stop fault
  control to reach authenticated broker close through the installed client after
  dropping to operator UID/GID 65534, rather than invoking only the shared helper;
  a five-minute handler deadline prevents manager runtime expiry from masking
  absent fallback fencing during the two-second refusal/depopulation control;
  removing broker fallback fencing must now leave the original tree populated
  and fail the control. The broker response must refuse closure, preserve damaged
  handoff and omit manager/closure completion while revoking entry and fencing.
  Execute all-target stable clippy, authority unit tests (20 passed; 13
  provisioned controls ignored) and formatting/size/diff checks pass.
  Native execution of this caller control remains pending, as does the full
  portable/frozen acceptance and production routing; task 9303 remains active.

- Review identified genuine installed-gate exec failures being classified as
  process nonzero exit or MCP protocol failure. The runner now creates a private
  pre-grant exec-status channel with a charged bounded Root-only metadata record,
  verifies the original handoff/enrollment and sole cgroup process before and
  after a one-shot PID hello, and retires the listener/socket/directory before
  grant. The gate verifies descriptor close-on-exec and reports only actual exec
  syscall failure; candidate selection withholds process/MCP results until the
  channel resolves and never trusts handler stdout/stderr or reserved exit codes.
  Exact private error plus complete original closure yields existing spawn; EOF
  alone never proves execution or termination. Partial/malformed channels retain
  uncertainty, and identity-matched cleanup runs before every receipt, including
  never-launched crash recovery. Association has a shared two-second deadline
  within the existing five-second gate wait; ownership/capacity semantics and
  journal/receipt/attestation/hash/public response formats remain unchanged.
  Named provisioned private_exec_status inventory covers missing/permission-denied
  process and MCP commands, forged stderr/exit 203, descriptor/path retirement,
  denied operator connect and unknown/torn/symlink/readable/replaced private paths;
  these native controls are compiled but not executed on this non-systemd host.
  SPEC/API/embedding/release document the new unreleased private metadata boundary
  and reserved identity/supplementary membership prerequisite. Review explicitly
  retains the deployment lemma that the pre-grant dynamic identity has no other
  holders, including stale recycled-identity processes; PID hello and same-group
  path permissions alone do not establish that lemma. Production gate remains
  unreleased until the supported profile and primitive authenticate it. Stable
  and MSRV authority units pass 23 with 14 provisioned controls ignored; workspace
  all-target stable clippy, formatting, size/diff and native script syntax pass.
  Fresh full gates,
  native stable/MSRV proof, six portable axes, frozen high-risk review, installed
  executable-byte proof and production routing remain pending; 9303 stays active.

- Authoritative CI run 37340474722 has completed successfully at frozen
  181cbad11d169e3a3c46126a42687b1b5be74d46: all six full portable platform/
  toolchain axes, zero-dependency job and both provisioned native jobs pass.
  Previously independently verified native artifacts each contain 78 cases with
  executable bytes unverified and production gate unreleased. This establishes
  that baseline only, predating shared broker fencing and private exec reporting;
  it does not validate current 0848dfd/c73c8cf source or the newly named native
  private_exec_status control. Warning-free stable workspace documentation passes
  at c73c8cf. The remaining identity-exclusivity authentication review, fresh
  current native/portable/full gates and production host routing remain required.

- Frozen ac331c2 local stable workspace debug gate passes in full with serial
  tests and compiler/rustdoc warnings denied. Sender review replaces the
  provisional exclusive-dynamic-identity assumption for exec-status with a
  32-byte ready-kernel-random nonce delivered solely through inherited manager
  stdin before launch; the 44-byte PID/nonce hello must match before grant.
  Ordinary installed Root helper/client validation now requires 0711, with
  fs.suid_dumpable 0 or 2 checked before launch and protected Root-owned proc fd
  directory/no tracer verified before and after association. Kernel exec closes
  CLOEXEC descriptors before restoring dumpability. Primary Linux v6.8 exec/proc
  source supports these mechanisms; /proc/PID's world-readable directory itself
  retains effective UID ownership, so review dropped an incorrect UID-check
  concern without changing that guard. The nonce is not persisted or exposed to
  handler argv/env; MCP worker startup follows hello to prevent stdin read-ahead
  loss, and process exec restores /dev/null input while retiring its Root writer.
  A native same-UID/GID actor races the frozen original gate, cannot inspect its
  protected descriptors, sends the correct PID with a false nonce and must be
  refused without grant/completion/result; exact original cleanup follows.
  Existing named controls also verify null process input and no inherited status
  socket. Native suite budget is 90 seconds for this multi-run fault control,
  without changing individual association/handler/closure limits. The previous
  sender lemma is superseded for authentication; native execution, fresh full
  stable/MSRV and portable gates, executable-byte/frozen proof, production host
  routing and task 9303 acceptance remain pending. No journal/receipt/attestation/
  hash/public response formats change; the unreleased installed-helper profile
  requires reprovisioning. Stable and MSRV authority unit subsets each pass
  24 tests with 14 provisioned controls ignored; final stable workspace all-target
  clippy passes with warnings denied. These subsets compile the new native
  controls but do not establish their runtime acceptance.

- Frozen 23a0d91 full stable release workspace gate passes with serial tests
  and rustc/rustdoc warnings denied. CI 37353887719 native stable and MSRV
  both refuse in enrolled_gate_authorization before nonce controls: the new
  refused-close fixture incorrectly supplies an allocation object rather than
  the SPEC positive canonical number. Correcting only that fixture permits
  actual broker dispatch; malformed original handoff, live-domain fencing,
  retained refusal and no-closure assertions remain unchanged. Native acceptance
  and fresh full portable gates remain pending; this failed run is not proof
  of nonce authentication, and task 9303 remains in progress.

- Review repair routes never-launched residual cgroup removal through the same
  remove_empty helper as completed submissions, eliminating an unbounded
  read_to_string and permissive populated-line scan. Existing observation
  parser fault tests cover duplicate/missing/unknown/noncanonical samples;
  genuine_claim_binding retains native never-launched refusal, removal,
  incomplete-receipt and cold-retry controls. This changes no published bytes
  or hash domains and does not establish native runtime acceptance; full
  current-source gates and task 9303 acceptance remain pending. Stable authority
  units pass 24 with 14 provisioned controls ignored, and warning-denied stable
  workspace all-target clippy, formatting and size/diff checks pass.

- Frozen 496e55d full stable debug workspace gate passes with serial tests and
  rustc/rustdoc warnings denied. CI 37354885917 native stable/MSRV artifacts are
  independently verified at exact clean 496e55db4eac667db8dd59c464a56ab613268354:
  each contains 79 passing cases, including private_exec_status and genuine
  never-launched closure, with original source/compiler and suite/authority-log
  hashes checked. Production backend and release remain false; portable matrix
  completion, installed executable-byte proof and final high-risk review remain
  pending. Preparation now repeats installed ordinary Root 0711/dumpability and
  bounded kernel entropy readiness checks before lock creation or allocation
  intent; launch obtains its own fresh nonce and repeats profile checks. This
  closes a prerequisite ordering gap without accepting task 9303 or adding a
  fallback; production host routing and remaining lifecycle tasks are required.
  Stable authority unit subset passes 24 with 14 native controls ignored; stable
  workspace all-target clippy with denied warnings and format/size/diff checks
  pass. Fresh native prerequisite execution and full-source gates remain pending.

- Frozen ea5da75 full stable release workspace gate passes with serial tests
  and rustc/rustdoc warnings denied. CI 37356196081 native artifacts pass the
  frozen-source evidence verifier independently at stable/MSRV, each with 79
  cases, expected negative-failure proof and exact report/log hashes; executable
  bytes remain unverified and the gate unreleased. Portable full matrix remains
  pending. Review identified that capture boundary tests exercised the legacy
  direct-child runner, so native_capture_bounds now drives the actual claimed
  runner at 4096/4097, 1 MiB/1 MiB+1 and 8 MiB for process stdout/stderr and MCP
  failure stderr. It verifies exact prefixes, only complete bounded digests,
  bounded response, matching original closure, actual group absence, retained
  journal ownership and no stopped record before application, plus equal owned
  descriptor/thread inventories after each of ten runs. The named native case
  receives 90 seconds without changing handler/cleanup deadlines. Stable
  authority subsets at stable and MSRV each pass 24 with 15 provisioned controls
  ignored, and all-target stable clippy passes; new native capture runtime proof,
  fresh full gates and
  task 9303 acceptance remain pending.

- Frozen 0687876 full stable debug workspace gate passes with serial tests and
  rustc/rustdoc warnings denied. CI 37357493793 native stable and MSRV both fail
  native_capture_bounds at the first MCP failure-class assertion: the fixture
  incorrectly expected null for isError=true, whereas the documented tool-call
  class is mcp_error. Correcting that expectation preserves every prefix/digest,
  bounded result, original claim/closure and resource-retirement assertion;
  process success still expects null. Both failed artifacts are retained and
  do not count as 80-case acceptance; fresh runtime proof remains pending and
  task 9303 stays active.

- Frozen e1d358f full stable release workspace gate and warning-denied workspace
  documentation pass. CI 37358669917 native stable and MSRV each pass all 80
  cases; the independent frozen-source evidence verifier checks both exact
  e1d358f25149f03edf92142d3f7abb19371e8b57 artifacts, source inventories,
  compiler, suite/log digests and expected negative-failure proof. The actual
  native_capture_bounds control passes all ten process/MCP runs at both
  toolchains, including exact/plus-one prefix/hash boundaries and 8 MiB output,
  original-claim closure and repeated descriptor/thread retirement. This closes
  that runner evidence gap; it does not substitute for production host routing,
  prerequisite refusal faults, full portable acceptance or frozen high-risk
  review. Current six portable CI axes remain queued/running, executable bytes
  remain unverified, production backend/release remain false, and task 9303
  remains in progress pending its complete acceptance audit.

- Native prerequisite refusal control now verifies the exclusive provisioner's
  installed fixture device/inode and complete bounded artifact SHA-256 before
  changing only that original open descriptor's mode from 0711 to 0755. It
  requires early prepare refusal with unchanged counter/directory inventory
  and no cgroup, restores 0711 through the same descriptor (including best-effort
  unwind cleanup), then proves the first successful allocation is still 1.
  No existing product installation is reused or modified: the provisioner
  exclusively creates the fixture and passes its exact identity to the test.
  Formatting, native script syntax and tracked diff/size checks pass; local
  compilation was stopped before acquiring the shared artifact lock after
  host swap exceeded 80%, so no compile/lint/runtime pass is claimed for this
  unit. Fresh provisioned stable/MSRV and full portable proof remain required;
  task 9303 remains active and all production integration is still pending.

- CI 37359888477 provisioned native stable and MSRV both pass all 81 cases at
  frozen 2a327807ac99276420844a74e377b2dbbe71ba8d, independently verified against
  source inventories, compiler identity, report/log hashes and negative-failure
  proof. native_profile_refusal passes on both, closing its runtime evidence
  gap without modifying product installations or consuming a rejected allocation.
  This also compiles the new control at both toolchains; local host compilation
  remains deferred under swap pressure, and full portable jobs remain active.
  The verifier still reports executable_bytes_verified=false and gate_released
  false; production routing, full host gates and complete 9303 acceptance remain
  required, with every later task and plans 20/21/23 still unfinished.

- Acceptance review repairs allocation's remaining unbounded cgroup-events
  read and permissive populated-line scan through the existing exact bounded
  observation reader, requiring empty/unfrozen initial state and repeated
  original directory identity/protection checks before prepared publication.
  Burned intent/counter survive refusal; no prepared evidence or reuse follows.
  Existing exact event parser faults and provisioned allocation cases remain
  the required checks; stable executor library tests pass (18), and authority
  tests pass (24, with 16 provisioned-native cases ignored locally), together
  with formatting, source-size and diff checks; fresh native and full
  host/portable gates remain required and 9303 stays
  active rather than being accepted from the previous 81-case source.

- Frozen allocation-observation source `893dd8b3c6d6db0e6ea87482690c19d9a271ceda`
  passes both provisioned native jobs in CI `37361472892`; independently
  verified artifacts retain 81 cases at stable Rust 1.99.0 and MSRV 1.89.0,
  exact clean source, frozen suite inventories and report/diagnostic hashes,
  including the expected final-kill refusal control.
  Evidence is retained under the dedicated task cache in
  `ci-37361472892/stable` and `ci-37361472892/msrv`.
  Executable-byte comparison remains unproved and the native gate remains
  unreleased; the local stable workspace debug gate is still running, and
  the six portable axes have no terminal aggregate verdict yet.
  Review confirms production native process/MCP descendant and corrupted-proof
  controls; standalone, embedded and public service tick ownership routing
  remains task 9401 work, and task 9303 remains in progress.

- The full local stable debug workspace gate for source `893dd8b` completes
  with exit zero, including the embedding, zero-dependency, lifecycle and
  existing journal/crash suites; its task-cache log is
  `local-893dd8b-stable-debug.log`.
  Formatting, source-size and complete committed-range diff checks also pass.
  The stable release workspace gate is running serially, and all six portable
  CI jobs for `37361472892` are live without a terminal aggregate verdict.
  Review of the core/store delta against accepted persistence base `cf3f600`
  checks the original-policy settlement selector, exact settlement replay,
  current-claim hash access and physical-store proof validation against their
  SPEC contracts; this does not replace the remaining frozen aggregate review
  or accept downstream service ownership integration.

- Named-target coverage repair `0cef6f6c5be8881429dc97fa7ebd5e1ac9abfaa7`
  passes native stable and MSRV in CI `37363278015`; independent verification
  confirms 81 matrix cases per compiler, exact frozen source and the
  `lifecycle_runner` fixture target, with evidence retained in task-cache
  `ci-37363278015/stable` and `ci-37363278015/msrv`.
  The native authority cases now execute through the plan's named integration
  target; portable invocations still intentionally ignore provisioned cases.
  This repairs the command-level coverage gap without accepting task 9303.
  Frozen host validation will use `runner-review-0cef6f6` in the task cache.
  The release gate launched earlier in the active checkout remains preliminary
  because the test target changed while it was compiling; its completion
  cannot stand in for that frozen gate, and full portable verdicts remain
  pending while host swap pressure prevents additional local builds.

- Admission review repairs prevalidation cloning/hashing of caller-constructed
  handler material: borrowed strings and nested values are charged first, then
  the complete normalized contract is checked against the exact JSON ceiling.
  A limit/limit-plus-one unit control and production claim refusal controls
  check unchanged records/state/request keys for excessive bytes/depth.
  Formatting, source-size and diff checks pass; compilation and fresh native,
  portable and frozen host gates remain pending under host swap pressure.
  Task 9303 remains active and no downstream integration is accepted.

- CI `37358669917` at historical source `e1d358f` is now terminal success:
  all six portable stable/MSRV Linux/macOS/Windows gates, zero-dependency and
  both native jobs pass; this supplements its independently verified 80-case
  native reports but does not validate the later profile/observation/target/
  caller-input changes.
  Cancellation was requested for superseded runs `37363278015`, `37361472892`
  and `37359888477` after verifying their exact obsolete review-branch commits;
  retained native evidence is preserved, and unfinished portable axes from
  cancelled runs must never be reported as passing.
  Current source `602b3dc` remains under live CI `37364129624`, with its clean
  frozen checkout at task-cache `runner-review-602b3dc`; freeing obsolete CI
  work does not accept task 9303 or release any production native gate.

- Admission guard review extends the production claim refusal control to all
  three nested-value branches: MCP arguments and both outcome payloads, beside
  the excessive argv byte case; each requires the exact early bound refusal
  and unchanged records/state/request keys before the successful original claim.
  These controls require fresh native execution; they are not accepted from
  the previous source's smaller branch coverage.

- Stable native CI `37365040760` at `7bd363a` fails in
  `private_exec_status` before the later genuine-claim admission controls:
  production execution returns `runner cleanup uncertain` after a bounded
  manager query deadline; no positive closure/result is accepted.
  Failed reports and exact job diagnostics are retained in task-cache
  `ci-37365040760/stable` and `ci-37365040760-native-stable-job.log`.
  The failure does not establish its timing cause or prove the admission fix;
  the fixture now labels its fixed command/kind when execution refuses, so
  further native diagnosis can distinguish exec-status scenarios without
  relaxing production deadlines or converting uncertainty into success.
  Current MSRV/portable validation remains unexecuted or queued at this source.

- The MSRV native job for failed-stable run `37365040760` completes with
  success at exact source `7bd363a`; independent verification confirms all
  81 matrix cases, frozen named-target identity and report/log hashes, including
  the expanded production handler admission refusal branches.
  Evidence is retained in task-cache `ci-37365040760/msrv`; stable evidence
  remains a separate failure and neither verdict explains its timing cause.
  Diagnostic source `31e0c63` is under CI `37365652966`, with its frozen
  task-cache checkout at `runner-review-31e0c63`; native MSRV is live and
  stable is queued, while local builds remain deferred under swap pressure.
  Production gate release and task 9303 acceptance remain pending.

- Diagnostic-source native MSRV CI `37365652966` at exact `31e0c63`
  completes successfully; independent verification confirms the full 81-case
  matrix, exact compiler/source/target identities and report/log hashes.
  Its task-cache evidence is `ci-37365652966/msrv`; stable at this source is
  still queued, so this second MSRV pass does not explain or erase the earlier
  stable manager-query timeout.
  Superseded run `37365040760` was requested cancelled only after retaining
  that split native evidence; cancelled portable work is not acceptance.
  Current native stable and portable verdicts remain required, alongside the
  unfinished aggregate review and production host ownership integration.

- Diagnostic-source native stable CI `37365652966` at exact `31e0c63`
  completes successfully; independent verification confirms all 81 cases,
  compiler `rustc 1.99.0 (b940084d7 2026-09-28)`, frozen named target,
  exact source identity and report/log hashes, with retained evidence in
  task-cache `ci-37365652966/stable`.
  Both native toolchains now pass at this product source, including
  `private_exec_status` and all production handler admission branches; this
  pass does not establish the cause of the prior stable manager-query timeout
  or discard its failed evidence.
  Executable bytes remain independently unverified and the production gate
  remains unreleased; six portable gates and aggregate frozen review are
  still required before accepting task 9303.
  Task 9401 now contains a source-backed integration handoff review, while
  its production host routing and acceptance tests remain planned.

- Current CI `37365652966` retains both verified native successes, and Ubuntu
  stable finishes debug workspace tests before entering release tests;
  Windows MSRV remains in debug tests.
  Five queued jobs (zero-dependency, Ubuntu MSRV, macOS stable/MSRV and Windows
  stable) terminate cancelled at 20:02 UTC with check annotations reporting
  that a hosted runner was not acquired after multiple attempts.
  These are unexecuted gates, not test passes or product failures; no newer
  review-branch run exists and the remote source remains exact `31e0c63`.
  Preserve the two active portable jobs, then retry the cancelled gates at
  that same source when the run becomes terminal rather than duplicating the
  running work or substituting historical platform evidence.

- Ubuntu stable job `111950022600` in CI `37365652966` completes successfully
  at exact source `31e0c63`; each step is terminal success: formatting, size
  limits, workspace debug/release tests, all-target Clippy with denied warnings,
  documentation, fuzz compilation and byte-identical decimal regeneration.
  The exact job log is retained at task-cache
  `ci-37365652966-ubuntu-stable.log`; this is current-source Linux CI evidence,
  not a substitute for the five unexecuted cancelled jobs or the still-running
  Windows MSRV gate, nor proof of the separate local frozen host invocation.

- Windows MSRV job `111950022633` in CI `37365652966` completes successfully
  at exact `31e0c63`; formatting, size, debug/release workspace tests, all-target
  Clippy, documentation and decimal regeneration all pass; fuzz compilation is
  skipped on Windows by the existing workflow.
  Its retained exact log is task-cache `ci-37365652966-windows-msrv.log`.
  After the first attempt became terminal, `gh run rerun --failed` started
  attempt 2 at the same exact source, retrying only the five jobs cancelled
  for hosted runner acquisition; all four successful gates remain successful.
  Windows stable retry job `111966694566` is live; zero-dependency
  `111966694085`, macOS stable `111966694437`, macOS MSRV `111966694744` and
  Ubuntu MSRV `111966694772` remain queued and unaccepted.

- Attempt 2 macOS stable job `111966694437` completes successfully at exact
  `31e0c63`, with formatting, size, debug/release workspace tests, all-target
  Clippy, documentation and decimal regeneration passing; fuzz compilation is
  skipped by the existing platform condition.
  Its exact retained log is task-cache `ci-37365652966-macos-stable.log`.
  Windows stable and macOS MSRV remain live in release tests; zero-dependency
  and Ubuntu MSRV retries terminate cancelled again with check annotations
  explicitly reporting hosted runner acquisition failure after multiple attempts.
  Those two gates remain unexecuted and require another exact-source retry
  after the active work finishes; neither cancellation accepts task 9303.

- Attempt 2 Windows stable job `111966694566` completes successfully at exact
  `31e0c63`; every applicable gate step passes, including debug/release workspace
  tests, all-target Clippy, documentation and decimal regeneration; the existing
  platform condition skips fuzz compilation.
  Its retained exact log is task-cache `ci-37365652966-windows-stable.log`.
  All three stable OS gates and Windows MSRV now pass at this product source,
  beside both independently verified native matrices; macOS MSRV remains live
  in release tests and the two cancelled Linux jobs still require retry.

- Attempt 2 macOS MSRV job `111966694744` completes successfully at exact
  `31e0c63`, with every applicable gate step passing and the existing non-Linux
  fuzz condition skipped; its exact log is retained at task-cache
  `ci-37365652966-macos-msrv.log`.
  After attempt 2 became terminal, targeted failed-job rerun starts attempt 3
  at the same source for only zero-dependency `111978333384` and Ubuntu MSRV
  `111978333841`; both are queued and remain unexecuted.
  Five portable OS/toolchain gates and both native matrices now pass at this
  source, but the final portable axis, zero-dependency job and aggregate review
  remain required; task 9303 and the production native gate stay unaccepted.

- Attempt 3 zero-dependency job `111978333384` terminates cancelled without
  executing its gate; its check annotation again states that a hosted runner
  did not acquire the job after multiple attempts.
  Ubuntu MSRV job `111978333841` is live in workspace release tests, after
  completing workspace debug tests; no replacement run is started while it
  remains active, and the zero-dependency gate remains unaccepted.

- Attempt 3 Ubuntu MSRV job `111978333841` completes successfully at exact
  `31e0c63`; every gate step passes, including formatting, size, workspace
  debug/release tests, all-target Clippy, documentation, fuzz compilation and
  byte-identical decimal regeneration.
  Its exact log is retained at task-cache `ci-37365652966-ubuntu-msrv.log`.
  All six portable OS/toolchain gates now pass beside both verified native
  matrices; after the attempt became terminal, failed-job rerun starts
  attempt 4 for only zero-dependency job `111985628762`, now queued.
  Zero-dependency execution, the separate frozen local host invocation and
  aggregate review remain outstanding; task 9303 is not accepted.

- Attempt 4 zero-dependency job `111985628762` completes successfully at
  exact `31e0c63`, passing both the CLI zero-dependency test and the
  embed-acceptance dependency-tree check; its exact log is retained at
  task-cache `ci-37365652966-zero-deps.log`.
  CI `37365652966` is now terminal success with all six portable gates,
  both independently verified 81-case native matrices and zero-dependency
  checks passing at the same product source.
  Local swap usage still exceeds the workspace's 80% threshold, so no new
  intensive local gate is started; the separate frozen local host invocation
  and aggregate high-risk review remain outstanding, and task 9303 remains
  in progress without releasing the production native gate.

- Aggregate review finds interrupted exec-status accept/hello-read retries
  bypassing the shared deadline at `31e0c63`; correction `9f1f175` checks
  before every retry and adds provisioned production-listener fault controls
  for both branches, asserting no post-expiry I/O, grant or stopped record.
  SPEC/API/embedding/release contracts move with the correction; formatting,
  source-size and full-range diff checks pass, but intensive local gates remain
  deferred by the workspace swap limit.
  Only the authorized review branch is pushed; new CI `37376949993` is live
  at exact `9f1f175ad91609359699e3a2d670119e8cbb506a`.
  Historical green `31e0c63` does not accept this correction; corrected-source
  native/portable/dependency gates and remaining aggregate review are required.

- Corrected-source CI `37376949993` zero-dependency job `111988370112`
  passes at exact `9f1f175`, including CLI zero-dependency execution and the
  embed-acceptance dependency-tree check; its exact log is retained at
  task-cache `ci-37376949993-zero-deps.log`.
  Both native compiler jobs and all six portable gates remain live; the new
  interrupted-association controls are not yet accepted from their job state.

- Corrected-source stable native job `111988369737` passes in CI
  `37376949993`; its retained task-cache `ci-37376949993/stable` artifact
  independently verifies all 81 frozen inventory cases at exact `9f1f175`
  with `rustc 1.99.0 (b940084d7 2026-09-28)`, including the production
  private exec-status case now containing both interrupted-retry controls.
  Executable-byte verification and production gate release remain false;
  native MSRV and all six portable gates remain live and unaccepted.

- Corrected-source MSRV native job `111988370064` passes in CI
  `37376949993`; retained task-cache `ci-37376949993/msrv` independently
  verifies all 81 cases at exact `9f1f175` with
  `rustc 1.89.0 (29483883e 2025-08-04)`.
  Both native compilers now execute the interrupted accept/read controls in
  the passing private exec-status case; all six portable gates remain live.
  The deadline finding has a tested correction at the native boundary, but
  corrected-source portable/local gates and remaining review still prevent
  task acceptance or production gate release.

- Corrected-source Ubuntu stable gate `111988370337` passes in CI
  `37376949993` at exact `9f1f175ad91609359699e3a2d670119e8cbb506a`;
  job metadata confirms every step succeeds, including debug/release workspace
  tests, formatting, file size, all-target Clippy, documentation, fuzz target
  compilation and byte-identical decimal regeneration.
  The exact log is retained at task-cache `ci-37376949993-ubuntu-stable.log`.
  Five portable jobs remain live; this pass does not substitute for the
  separate local frozen host invocation or finish aggregate review.

- Corrected-source Ubuntu MSRV gate `111988370280` passes in CI
  `37376949993` at exact `9f1f175ad91609359699e3a2d670119e8cbb506a`;
  every step succeeds, including debug/release workspace tests, formatting,
  size, all-target Clippy, documentation, fuzz compilation and decimal vectors.
  Its exact log is retained at task-cache `ci-37376949993-ubuntu-msrv.log`.
  Both Ubuntu compiler gates now pass; four macOS/Windows jobs remain live,
  and full-range review plus the separate local frozen host gate remain pending.

- Corrected-source macOS MSRV gate `111988370164` passes in CI
  `37376949993` at exact `9f1f175ad91609359699e3a2d670119e8cbb506a`;
  its exact log is retained at task-cache `ci-37376949993-macos-msrv.log`.
  This is actual macOS execution, not cross-compilation or Linux substitution;
  all required job steps succeed. Three portable jobs remain live, and the
  separate frozen local host gate remains unexecuted because swap exceeds
  the workspace limit; task 9303 and the production native gate remain open.

- Corrected-source macOS stable gate `111988370324` passes in CI
  `37376949993` at exact `9f1f175ad91609359699e3a2d670119e8cbb506a`;
  exact log retained at task-cache `ci-37376949993-macos-stable.log`.
  Required debug/release tests, formatting/size, all-target Clippy,
  documentation and decimal regeneration pass; Linux-only fuzz compilation
  is skipped on macOS by the workflow. Both macOS and both Ubuntu axes now
  pass; the two Windows jobs remain live, including stable now in release tests.
  The local frozen host gate and task acceptance remain pending.

- Corrected-source Windows MSRV gate `111988370258` passes in CI
  `37376949993` at exact `9f1f175ad91609359699e3a2d670119e8cbb506a`;
  its exact log is retained at task-cache `ci-37376949993-windows-msrv.log`.
  This is actual Windows debug/release workspace test execution, with required
  formatting, size, all-target Clippy, documentation and decimal checks passing;
  the Linux-only fuzz step is skipped as configured.
  Only Windows stable remains live in release tests; the separate local frozen
  host gate remains unexecuted under the workspace swap rule.

- Corrected-source CI `37376949993` completes successfully with all nine jobs
  green at `9f1f175ad91609359699e3a2d670119e8cbb506a`.
  Final Windows stable job `111988370545` passes every required step; its exact
  log is retained at task-cache `ci-37376949993-windows-stable.log`.
  All six actual portable axes, both independently verified 81-case native
  compiler matrices and zero dependencies now pass on the deadline correction.
  Reviewed evidence is consolidated in task 9303's corrected-source acceptance
  table; the separate local frozen stable host invocation remains unexecuted
  under the workspace swap limit. This CI verdict does not release the native
  production gate or finish task 9401 host integration, shutdown/reconciliation,
  concurrent host/crash acceptance, or plans 20, 21 and 23.

- Prepared an isolated detached checkout of exact corrected `9f1f175` at
  task-cache `runner-review-9f1f175` for the remaining frozen local gate;
  its tracked/untracked status is clean and stable formatting plus source-size
  checks pass there. No build or intensive test was started: current swap
  remains above the workspace threshold. The isolated checkout prevents later
  documentation commits or the user's untracked workflow from changing the
  code verified by the eventual local gate.

_Task frontmatter remains authoritative; registration does not release the native gate._

- **Contained runner accepted:** task 9303 lands at frozen product `9f1f175`,
  reviewed from `cf3f600`; all required local stable host gates and exact-source
  CI `37376949993` pass, including both verified 81-case native matrices.
  Recorded findings are resolved; task 9401 is now In progress, while
  production host routing, bounded shutdown and reconciliation remain incomplete.

### Physical writer authority review (2026-10-06)

Review found that a healthy writer on an identical copied journal could pass logical claim checks without holding the original physical-store lease; fresh Runner installation now discovers the protected store registration and matches the original claim namespace/generation before retaining ownership or requesting binding, and shared native application checks its physical-store pin before changing entry permission, cursor, settlement or local capacity. The provisioned fresh process and actual MCP control now independently refuse an unpinned copied-store start with no Root binding/launch/entry/handoff, then refuse copied-store borrowed entry while an independent process holds the original writer, preserving both prefixes and the local reservation before successfully retrying with the original writer. These native assertions compile but have not yet executed at this source.

Rust 1.89.0 executor all-target Clippy passed in verified 1 GiB RAM/zero-swap scopes; all 27 library tests passed in session `89347`, whose later inventory invocation failed because the nonexistent target `public_api_inventory` was supplied. Corrected session `65043` passed Clippy, all 16 `public_surface` tests and all five tick tests, with logs `local-native-physical-writer-msrv.log` and `local-native-physical-writer-msrv-corrected.log` retained in the dedicated task cache. Formatting, file-size and diff checks pass. Run `37429029062` remains live with two Windows jobs, so no superseding push was made; provisioned process/MCP acceptance, automatic fresh admission, shutdown, reconciliation and cold post-ack recovery remain incomplete, task 9401 remains in progress, and production acceptance flags remain false.

### Selected native fresh admission (2026-10-06)

`Runner::new_native` now selects automatic preparation, writer-held claiming and one-shot binding/entry through both shared public tick entry points. The native host retains the original resolved effect and checked handler contract before any allocator transport, serializes allocation requests, and carries queued, preparing, delivered, cleanup, unknown-allocation and uncertain-claim states across writer contention. Only a never-requested queued cancellation or matched original-domain cleanup after actual helper retirement/EOF releases a matching unclaimed local slot; opaque allocation/startup, cleanup and claim-write failures remain charged and never produce legacy synthetic acknowledgements or replacement allocation. Claim publication transfers to an installed original owner before binding, including post-publication startup refusal; genuinely observed claims after an uncertain append transfer to existing recovery ownership. Original ready owners precede additional claims, and readonly snapshot storage is dropped before opening the standalone writer to avoid retaining two full copies unnecessarily.

New genuine provisioned process and actual MCP axes start with no prepared domain or published claim: readonly borrowed refusal leaves allocation/binding/entry absent, an independent writer permits allocator progress but blocks publication, healthy original writer publishes exactly one claim, further independent contention/read-only refusal blocks Bound entry, original writer dispatches once, completion progresses without the writer, and durable ack precedes the original accepted event while releasing local capacity. These controls are in the existing `provisioned_broker_access` call chain and compile at this source; they have not yet run. The independent lease-only helper is additional coverage for the initially unclaimed phase and does not replace the existing genuine competing-domain claim checks in the recovered/manual-handoff controls. Cancellation of delivered preparation, losing admission to another writer, post-claim startup failure and subsequent capacity reuse still need genuine runtime controls; production CLI/MCP/service defaults still use the earlier Runner selection, and shutdown/reconciliation/cold post-ack recovery remain incomplete. Task 9401 stays in progress with empty `merged_as`, and all production/gate/executable flags stay false.

Final local Rust 1.89.0 verification in session `39734` passed executor all-target Clippy, all 28 library tests, all 16 public inventory tests and all six tick tests, including unavailable-authority/read-only refusal through both public paths with no legacy marker or journal mutation; the verified scope had MemoryMax=1 GiB and MemorySwapMax=0, and the retained log is `local-native-selected-admission-final-msrv.log`. Earlier session `91438` correctly failed compilation of the new ignored admission probe because it referenced a nonexistent Record.request_id field; the probe now reads the genuine body request_id, with corrected and final logs retained. Formatting, file-size and diff checks pass. This is local portable proof and compilation of the provisioned controls, not runtime native acceptance.

Run `37429029062` at exact source `f5fb6ae2b4a32b6c14b884b3a16037fbe453bfe9` is terminal failure: both Windows portable gates and zero-deps succeeded, all four Ubuntu/macOS gates failed the 1003-line broker-native-tests size gate, and both native jobs failed fresh manual handoff before the independent writer barrier because the wrapper omitted the genuine competing-domain environment value. Structural splits and the original-domain environment correction are already committed, while corrected actual native execution remains pending. Terminal metadata and all six nonempty portable job logs are retained under `ci-37429029062`; the two failed native artifact sets were retained earlier, and no all-81 verification success is claimed for them. This terminal observation permits a subsequent corrected-source review push without cancelling a live run.

### Admission review follow-ups at f0570b1 (2026-10-06)

Authorized review push `f0570b1a21144d7101781742f88f53fce5d97aa1` started CI run `37437221677`; it was confirmed live rather than restarted, with zero-deps successful, both native jobs failed, stable Windows failed and the other five portable jobs still running at the last observation. Completed-job direct logs and both native artifact sets are retained under `ci-37437221677`; the normal gh run log command refuses while the parent is live, so completed job logs were obtained from their direct API endpoints with terminal escapes sanitized for display. No superseding push has been made.

MSRV native execution now passes the former missing-competing-domain barrier and the physical-store/manual-handoff assertions through original ack/event advancement, but fails the final duplicate check at 11 records versus 10. Source review shows resume re-enters in_review and emits a distinct notify, which the still-enabled synthetic current handler may legitimately settle during that repeat tick. The control now independently requires that distinct pending identity and replaces its current table with an empty scheduler only after the original local slot is verified released, preserving the exact unchanged-prefix replay assertion; original writer barriers, competing-domain rejection, entry checks, root handler contracts and deadlines are unchanged. Corrected actual runtime remains pending, and the new selected-admission process/MCP axes were not reached by this failed manual-handoff axis.

Stable native fails before authority cases during protected installation with authority artifact exceeds bound or differs from frozen build; no installed authority report or all-81 verdict exists. A serial frozen-source local stable build of f0570b1 reproduces the size condition: unstripped artifact 67,753,048 bytes exceeds the unchanged 67,108,864-byte limit, while Cargo DEV_STRIP=debuginfo produces 13,218,200 bytes with opt_level=0, debug_assertions=true and overflow_checks=true. The authority producer now packages this exact Cargo artifact with only debug sections stripped, computes its digest afterwards, and records authority_strip=debuginfo; installer size, digest, identity, permissions and cleanup checks stay unchanged. Frozen size/profile verification session `49706` passed, and corrected actual producer plus executor all-target MSRV Clippy passed session `11771`, both in verified 1 GiB RAM/zero-swap scopes; logs are local-f0570b1-authority-artifact-bound-stable.log and local-admission-fixture-packaging-corrected.log. Python syntax, formatting, file-size and diff checks pass. This reproduces the size cause locally and verifies packaging, not native runtime acceptance.

Stable Windows reports OS error 433, A device which does not exist was specified, while writing the http_post dependency file and again for the runner step-summary path; these logs identify storage failure and do not establish a product compile/test defect or justify changing checks. Its job is not accepted. Remaining portable gates, corrected native runtime, default production host selection and all remaining lifecycle work are still outstanding; task 9401 and plan 0022 remain in progress, merged_as remains empty, and production flags remain false.

Latest exact-source observation of run `37437221677` confirms all four Ubuntu/macOS stable/MSRV full portable gates and zero-deps succeeded at f0570b1; their nonempty logs and live-run.json are retained in the run cache. Windows MSRV job `112181888684` remains authoritatively in progress, stable Windows failed storage error 433, and both native jobs remain failed as described above. Corrections e5aa669 remain local pending that terminal run; no live CI was cancelled and no corrected native runtime acceptance is claimed.

### Prepared-domain cancellation control (2026-10-06)

The provisioned broker call chain now includes genuine preclaim cancellation: a native Runner collects its original prepared domain while an independent unprivileged process holds the writer, that writer journals actual instance cancellation and continues holding the lease, and public reporting ticks must retire the exact unclaimed domain and release only its local reservation without requesting a writer or publishing binding/launch/entry/handoff, claim, synthetic ack or outcome event. The writer verifies only its cancellation changed the prefix, and Root independently verifies the original closed-domain record, one allocation and removal of its original group. This is additional process-handler preparation coverage, not actual MCP execution, startup-after-claim failure, losing admission to a competing claim or subsequent execution/capacity reuse.

All-target Rust 1.89.0 executor Clippy passed in verified 1 GiB RAM/zero-swap session `96335`, compiling the ignored genuine control; formatting, file-size and diff checks pass, and log local-native-preclaim-cancellation-msrv.log is retained in the task cache. Actual provisioned execution is pending. The source f0570b1 review run still had Windows MSRV job 112181888684 live at this turn observation, so committed corrections were not pushed over it; task 9401 stays in progress and production acceptance remains false.

Prepared-domain cancellation now runs against both original process and actual MCP handler-table configurations in the provisioned broker call chain, retaining the same independent writer mutation, cleanup, no-binding/no-entry/no-ack and capacity-release assertions. These cancellation axes intentionally enter no handler and therefore do not replace the separate actual process/MCP execution controls. The unchanged UID/frame client script was extracted byte-for-byte to broker_client_native_probe.rs to keep the growing broker source below the Rust file-size gate. All-target MSRV executor Clippy passed in verified 1 GiB RAM/zero-swap session 47883 with log local-native-mcp-preclaim-cancellation-msrv.log; extraction equality, formatting, file-size and diff checks pass. Provisioned execution remains pending, default production selection remains incomplete and no production flag or task-completion status changes.

Run 37437221677 at exact f0570b1 is now terminal failure: Windows MSRV job 112181888684 completed successfully, so five full portable gates and zero-deps passed; stable Windows failed the retained storage error 433, stable native failed authority artifact installation, and native MSRV failed the corrected manual-handoff replay-fixture check. Final-run.json, all six nonempty portable logs, completed native job logs and both native artifact sets are retained under ci-37437221677. No all-81 or selected-native-runtime success is claimed. Terminal confirmation permits the reviewed replay/packaging and process/MCP cancellation corrections to be pushed without cancelling a live run; lifecycle integration and the full plans remain incomplete.


### Exact-source selected native runtime evidence (2026-10-06)

Review run `37443864437` executes exact commit `23ea633c8be029bf947bab92c98a6325f11f14de`; both native jobs completed successfully, stable `112203845384` and MSRV `112203845715`. Their downloaded artifacts are retained under `ci-37443864437/{stable,1.89.0}` in the dedicated task cache. The independent frozen-source verifier passed all 81 cases separately for rustc 1.99.0 and 1.89.0, with results retained as independent-verification.json; executable_bytes_verified and gate_released remain false. Source review confirms provisioned_broker_access invokes both manual fresh process/MCP handoffs, automatic selected process/MCP admission, and process/MCP prepared-domain cancellation, with child success and exact completion markers asserted by the Root wrapper. Its actual completed logs show the provisioned case passed on both compilers, so these controls now have runtime evidence rather than compilation alone.

This validates the reviewed manual replay fixture, bounded authority packaging and selected native admission/cancellation boundaries at this source; cancellation deliberately enters no handler and does not substitute for the separate actual MCP execution axes. All six portable gates remain live at this observation, zero-deps passed, and no superseding push is permitted yet. Default CLI/MCP/service production selection still uses the legacy constructor; bounded shutdown, reconciliation, cold post-ack recovery including sealed history, remaining concurrency/crash boundaries and plans 20–23 remain incomplete. Task 9401 remains in progress, merged_as stays empty and production acceptance flags remain false.


### Same-host capacity reuse control (2026-10-06)

The selected automatic process/MCP provisioned axes now use an explicit one-slot handler table and, after the first durable acknowledgement/event and unchanged replay, create a second genuine instance/effect through the same Runner, Watcher and Scheduler. Public reporting ticks must publish and execute its distinct second native allocation, accept its original outcome event after its actual acknowledgement and release the second local slot; the Root wrapper independently verifies two distinct bound effects, both consumed claims and acknowledgement/event dedup entries, and tracks both original groups for exact cleanup. Existing cancellation axes still require only one allocation and no entry. This closes a fixture coverage gap for subsequent capacity reuse; runtime execution at this new source remains pending and the earlier 23ea633 all-81 evidence does not prove these new assertions.

All-target executor Clippy with warnings denied passed on Rust 1.89.0 in verified 1 GiB RAM/zero-swap session 31185; log local-native-capacity-reuse-msrv.log is retained in the task cache, and formatting, source-size and diff checks passed. Review run 37443864437 still has all six portable jobs live, so no push supersedes it. Task 9401 remains in progress and production acceptance remains false.


### Losing fresh admission to a genuine competing claim (2026-10-06)

Additional provisioned process/MCP axes give an independent writer an actual Root-prepared first domain, let the selected native host prepare its distinct second domain while that writer holds the lease, and then publish a genuine checked-contract claim for the original effect through Pipeline.claim_native_handler in the writer process. Reporting ticks must clean only the host's now-ineligible unclaimed domain, release its exact local preparation slot without acquiring the writer, and preserve the competing claim and pending effect across repeated observations without binding, entry, replacement allocation, acknowledgement or outcome event. Root independently verifies the second domain's original closed record/group removal, both domains' absence of binding/launch/entry/handoff, exactly two allocations and the intact original competing claim. Final prepared-domain discard is explicitly fixture teardown and does not consume journal ownership.

All-target executor MSRV Clippy passed with warnings denied in verified 1 GiB RAM/zero-swap sessions 52191 and 46693; final log local-native-admission-competition-final-msrv.log is retained, and formatting, source-size and diff checks pass. These ignored controls compile but have not executed at this source; earlier 23ea633 native evidence does not cover them. Run 37443864437 remains live with all six portable gates, so no superseding push was made; ownership integration and the complete plans 20–23 objective remain unfinished.


### Native-selected direct primitive refusal (2026-10-06)

Review identified that Runner::new_native selected native admission for public ticks but its public spawn method could still launch an unclaimed direct process/MCP worker. Native-selected Runner::spawn now refuses with exec/mode before worker reaping, capture creation or any child/worker startup; explicit Runner::new primitives remain available. SPEC, API-POLICY, EMBEDDING and RELEASE document this selected-host boundary in the same change, without persistent format or public inventory changes. A portable process/MCP control verifies both refusals, unchanged scratch entries and no running/finished direct work.

Verified 1 GiB RAM/zero-swap session 24643 passed executor all-target MSRV Clippy, the new native_mode control, all 16 public_surface tests and all six tick tests; local-native-direct-spawn-refusal-msrv.log is retained. Formatting, source-size and diff checks pass. Earlier 23ea633 native runtime evidence predates this guard and the committed capacity-reuse/competing-claim controls, which still need exact-source provisioned execution. All six portable jobs in review run 37443864437 remain live at this observation; no push cancelled them, task 9401 remains in progress and the full plans 20–23 objective remains incomplete.


### Cold post-ack persistence review (2026-10-06)

[POST-ACK-RECOVERY.md](POST-ACK-RECOVERY.md) records the implementation and proof review for the remaining cold Ack-to-event gap: current pure settlement removes ownership, the host's original contract/completion handoff is volatile, and the sealed claim-hash index deliberately carries unresolved claims only. The required change is an atomic acknowledged-result/original-contract handoff, retired only by actual matching accepted event replay and authenticated through new versioned roots and sealed bases. The review explicitly rejects fingerprint-only outcome reconstruction, historical root/4 byte changes, claim-hash self-reference and inferred handoffs for legacy sealed acknowledgements, and defines crash, migration, boundedness, physical-store and process/MCP acceptance obligations. This is a reviewed implementation prerequisite, not a shipped format or completed cold recovery; task 9401 remains in progress.

A serial full stable debug workspace gate now runs against frozen fc22678 in verified 1 GiB RAM/zero-swap session 37186, with retained log local-fc22678-capped-stable-debug.log; no success is claimed before its terminal result. Review run 37443864437 still has all six portable gates live, so the newer commits remain local without cancelling review CI.


### Production routing candidate and restored defaults (2026-10-06)

[production-native-selection.patch](production-native-selection.patch) retains a concrete review candidate changing service::run/fsm execute and embedded MCP ExecutorLoop to Runner::new_native, with synchronized capability docs and genuine standalone CLI/embedded session process/MCP refusal controls. The candidate's all-target CLI/executor MSRV Clippy and both-host refusal control passed in verified 1 GiB RAM/zero-swap session 97466, but its subsequent existing serve_modes embedded happy-path regression failed: that unprovisioned fixture received exec/mode and recorded zero acknowledgements instead of the expected one. The retained log is local-native-production-both-hosts-corrected-msrv.log; session 1461 preserves the earlier observer error, corrected by reading past the observed-pending line to the actual bounded refusal. Session 87445 passed the embedded-only control, native_mode and all six tick tests before the standalone extension.

Automatic approval review rejected committing the production default switch because installed happy-path acceptance is unproven and the existing embedded regression fails. The production sources and their proposed documentation changes were restored byte-for-byte to HEAD, and the candidate test was removed from active tests; only the review patch remains. This is not an applied switch or a workaround for the rejection. Restored-default session 2202 passed the exact existing embedded regression in a verified 1 GiB RAM/zero-swap scope, with local-native-production-routing-restored-msrv.log retained. Genuine installed standalone/embedded process/MCP happy-path evidence and fixture adaptation preserving the original outcome semantics remain necessary before this candidate can be reconsidered; no test assertion was weakened or removed.

Frozen fc22678 full stable debug workspace session 37186 completed successfully, retained as local-fc22678-capped-stable-debug.log; it does not validate the unapplied candidate. Both native jobs, zero-deps and all four Unix portable gates of 23ea633 run 37443864437 passed, while its two Windows jobs remain live at this observation, so no superseding push was made. Current default production constructors remain legacy, task 9401 remains in progress and production flags remain false; cold post-ack recovery, shutdown, reconciliation and full plans 20–23 acceptance remain incomplete.


### Bounded acknowledged handoff value (2026-10-06)

The additive pure core AcknowledgedHandoff value implements the prospective original-claim/hash, contract, actual outcome, acknowledgement sequence and derived request-key envelope required by the cold recovery review. It bounds complete candidates to 128 KiB before cloning, bounds contract/outcome separately to 64 KiB, checks the original handler fingerprint and retry identity, selects the original success/failure event, preserves omitted versus null result, and refuses interruption, malformed event envelopes, foreign keys, zero sequence and unknown fields. Its independent literal handler fingerprint was calculated with Python stdlib SHA-256 from the specified domain and canonical contract. SPEC, API-POLICY, EMBEDDING and RELEASE explicitly distinguish this candidate value from authenticated publication or event permission; the full checked handler parser remains an execution-layer obligation.

Verified 1 GiB RAM/zero-swap session 93151 passed core all-target MSRV Clippy and all handoff, ownership, replay and value tests, with local-acknowledged-handoff-values-msrv.log retained; formatting, source-size and diff checks pass. No ExecutionState collection, atomic acknowledgement integration, VERSION/root/base/snapshot change or cold host discovery has landed, so this does not complete cold recovery or task 9401. Production defaults remain restored legacy constructors and the rejected switch remains only an unapplied review artifact; genuine installed host happy-path proof remains necessary. Review run 37443864437 still has two Windows jobs live, so no push supersedes it and no full terminal CI verdict is claimed.


### Original acknowledgement binding and terminal review (2026-10-06)

AcknowledgedHandoff now centralizes the pure comparison needed by atomic store integration: the complete actual original Claim, authenticated stopped outcome, stopped closure's run/domain identity, verified original claim-record hash and actual acknowledgement key/sequence must all match. Independently decoded candidates with a valid original key but changed hash, sequence, result or run fail, as does a foreign stopped closure. The store/fold must supply verified inputs; this helper does not manufacture publication proof. SPEC, API-POLICY, EMBEDDING and RELEASE move with the additive method, while persistent integration and cold delivery remain unfinished.

Verified 1 GiB RAM/zero-swap session 68138 passed core all-target MSRV Clippy, all seven handoff tests, 19 ownership tests and five replay tests; local-handoff-acknowledgement-binding-msrv.log is retained, with formatting, source-size and diff checks passing. No journal, root/base format or production-default change is included.

Run 37443864437 at exact 23ea633c8be029bf947bab92c98a6325f11f14de is now terminal success: both native jobs, all six full portable stable/MSRV gates and zero-deps passed. Final-run.json and eight nonempty job logs are retained under ci-37443864437 alongside both native artifacts and their separate independent all-81 verification results. This closes that source's review verdict, not acceptance of later same-host capacity reuse, competing admission, native-selected spawn refusal or handoff-value changes. It also does not validate the rejected/unapplied production routing candidate. Terminal confirmation permits pushing the newer reviewed source without cancelling a live run; task 9401 remains in progress, merged_as remains empty and production acceptance remains false.


### Published checkpoint anchor review and current native evidence (2026-10-06)

The new store regression builds a valid 10,000-record prefix through actual annotation and claim APIs, independently removes the checkpoint root fields to derive the provisional claim hash, and proves that current_execution_claim_hash returns the distinct final published hash. Complete prefix replay matches actual store state, and handoff matching against the actual stopped fixture accepts the published anchor while rejecting the valid-shaped provisional anchor. POST-ACK-RECOVERY.md records the projection hazard: append_execution currently replaces only projected last_hash after publication, so a future anchor cache must reconcile the actual final claim hash at checkpoint boundaries. This uses preauthenticated store fixture evidence and proves journal identity, not native closure or implemented handoff persistence.

Verified 1 GiB RAM/zero-swap session 50208 passed store all-target MSRV Clippy and the checkpoint regression, with local-handoff-checkpoint-anchor-msrv.log retained; formatting, source-size and diff checks pass. Both native jobs of live run 37449264100 completed successfully at exact 057763cdee301a0cbb918fa6bead57a67cc94ae5, and downloaded stable/MSRV artifacts independently verify all 81 cases against known rustc 1.99.0/1.89.0 identities. Artifacts and independent-verification.json are retained under ci-37449264100. Frozen call-chain review confirms the provisioned broker invokes genuine selected process/MCP same-host one-slot capacity reuse and prepared admission losing to an independent writer's genuine competing claim, in addition to existing manual/automatic handoffs and cancellation; these new assertions now have actual runtime evidence at that source. The checkpoint regression is later source and is not covered by those artifacts.

Zero-deps passed and all six portable jobs remain live at this observation, so no terminal/full-host verdict or superseding push is claimed. Default production constructors remain restored legacy selectors, their candidate is unapplied, installed production host happy paths still need proof, and atomic handoff storage, sealed cold recovery, shutdown and reconciliation remain incomplete; task 9401 stays in progress and all production/gate/executable-byte acceptance flags remain false.


The checkpoint fixture now assigns its memory store an owned directory under the explicit home .cache/fsm-handoff-checkpoint-tests root, whose child is removed by the fixture guard, containing the snapshot automatically emitted at sequence 10,000. The initial test left only crates/fsm-store/<memory>/snapshots/snap-10000.json, which was identified and removed. Automatic approval review rejected a first correction using the system temporary-directory API; the applied correction has no system-temp or /tmp fallback. Verified 1 GiB RAM/zero-swap session 40434 passed all-target store MSRV Clippy and the corrected regression, with local-handoff-checkpoint-anchor-owned-cache-msrv.log retained; source-size/diff checks pass and no placeholder snapshot artifact remains. This changes fixture cache ownership, not production persistence or native acceptance.


### Memory checkpoint filesystem correction (2026-10-06)

Automatic snapshotting now skips memory journals, matching shutdown and Drop behavior; the actual 10,000-record claim regression asserts its owned home-cache directory remains empty while preserving checkpoint roots, final claim hashes and full replay equality. SPEC, API-POLICY, EMBEDDING and RELEASE record the correction without changing VERSION or persistent bytes. Store all-target MSRV Clippy passed in a verified 1 GiB/zero-swap scope; the first exact-name test filter selected no tests, so a corrected filter was run and the actual boundary test passed, with local-memory-snapshot-refusal-msrv.log and local-memory-snapshot-boundary-msrv.log retained. Formatting, source-size and diff checks passed; broader stable and portable gates for this correction remain pending. Current run 37449264100 remains live with both native, both Ubuntu and zero-deps jobs successful and Windows/macOS jobs still running, so no superseding push or final verdict is claimed. Atomic acknowledgement persistence, cold recovery and production host acceptance remain incomplete, and task 9401 stays in progress.


### Verified replay anchor integration (2026-10-06)

Execution replay now carries each exact unresolved owner’s original claim-record hash outside logical execution serialization; canonical decoding alone carries no anchor, and settlement drops the context with ownership. The store replaces provisional context with the final published hash at checkpoint boundaries, base decode attaches its separately authenticated claim index only after all roots verify, and both checkpoint-bound and prefix-reproduced snapshot paths reconstruct anchors from verified records or exact base owners. Snapshot logical equality excludes the reconstructed context. Independent root material and every persisted format remain unchanged; attachment is a caller-trust API, not authentication or native authority, and foreign claims or malformed hashes refuse. SPEC, API-POLICY, EMBEDDING and RELEASE move with the implementation and POST-ACK-RECOVERY.md now distinguishes this shipped prerequisite from the unimplemented atomic handoff transition.

The initial check failed compilation on a missing string borrow in the base error mapper; the corrected serial 1 GiB/zero-swap scope passed core/store/embed all-target MSRV Clippy, all five execution replay cases, all 14 durable claim cases including append boundaries and two sealed reopens, the checkpoint/cache regression and the downstream ownership API test. Final session 12852 passed the extended bound/unbound checkpoint regression, three independent format-v2 goldens, base/snapshot goldens, historical admission/VERSION-10 migrations, all 21 execution unit/crash cases and the expanded downstream API test; corrected and final logs are retained as local-replay-claim-hash-context-corrected-msrv.log and local-replay-claim-hash-final-msrv.log. Formatting, source-size and full diff checks pass; the stable host gate and newer portable/native review remain pending. Current CI 37449264100 at exact 057763c remains live with both native, all four Linux/macOS portable jobs and zero-deps successful, while both Windows jobs continue, so no superseding push is made. Task 9401 remains in progress: handoff persistence, cold outcome delivery, actual production constructor acceptance, shutdown and reconciliation remain incomplete, and all production/gate/executable-byte acceptance flags stay false.


### Terminal selected-admission review and next format implementation (2026-10-06)

Run 37449264100 at exact 057763cdee301a0cbb918fa6bead57a67cc94ae5 is terminal success: both installed native jobs, all six full portable stable/MSRV jobs and zero-deps passed. Final-run.json and all eight nonempty portable/native job logs are retained under ci-37449264100 with the previously downloaded stable/MSRV artifacts and independent all-81 verifications. This closes review for that exact source, including genuine process/MCP same-host one-slot reuse, competing admission and native-selected direct-spawn refusal; it does not cover later checkpoint, memory snapshot or replay-anchor changes, the unapplied production-default candidate, or the new handoff format draft.

The stable host gate against frozen 9d27222ae8387c702a8415953685d88bfc094448 passed the complete debug workspace tests and is now building/running the release workspace leg under the same verified 1 GiB/zero-swap scope, with local-9d27222-stable-host-gate.log retained; Clippy, rustdoc, zero-deps and embedding follow serially and no complete gate verdict is claimed yet. Independent implementation work in the active worktree now drafts atomic handoff installation and exact accepted-event retirement, native completion contract forwarding, VERSION 12/root/5/snapshot/7/base/3, explicit old root/base paths, independently derived new goldens, migration through VERSION 11, durable/cache/sealed and torn-tail regressions. That unit remains uncommitted and has not yet run type checking or tests because the frozen host gate is still live; formatting/source-size checks pass, and it must complete format, append-fault and API verification before review. Cold native host delivery and copied-store binding remain separate required integration work, production constructors remain unchanged, task 9401 stays in progress and all production/gate/executable-byte acceptance flags remain false.


The frozen-source stable host gate for 9d27222ae8387c702a8415953685d88bfc094448 is now terminal success: formatting, source-size check, complete debug and release workspace tests, workspace all-target Clippy, warning-denied rustdoc, zero-deps and embedding acceptance all passed in verified 1 GiB RAM/zero-swap session 7268, with local-9d27222-stable-host-gate.log retained. The subsequent handoff format draft includes original-owner write/rotation/fsync failure vectors and explicit downstream API coverage; its first all-target MSRV type-check/test scope is now live with local-atomic-handoff-format-msrv.log retained, so no draft acceptance is claimed. This documentation checkpoint preserves the committed replay-anchor source for a newer exact-source portable/native review while keeping the format unit uncommitted until its checks pass.


### Atomic acknowledgement handoff persistence (2026-10-06)

The store/fold now installs a bounded original handoff in the same acknowledgement transaction and retires it only on an actual accepted event matching original instance, derived key, event, send fingerprint and stamped payload. Live publication and replay apply identical retirement, native completion forwards its checked original contract, and ownership/capacity remains separate. VERSION 12, root/5, snapshot/7 and base/3 authenticate the collection; explicit old root/4 and base/2 paths preserve historical bytes and refuse invented handoff blocks. Independent Python goldens first verify historical root material before deriving the new domains, while migration covers every prior VERSION 1–11 and retains future-format refusal at 13. SPEC, API-POLICY, EMBEDDING and RELEASE move with the unit, and POST-ACK-RECOVERY.md records the implementation review and remaining host integration.

Targeted verified 1 GiB/zero-swap MSRV scopes passed workspace all-target Clippy, seven handoff value cases, three bounded ledger/accepted-event cases, five execution replay cases, three historical format goldens, all 26 store execution unit/crash cases, 18 base cases including explicit base/2 preservation, all 14 durable claim cases, the external store refusal test, snapshot/6 skip and independently derived snapshot/7 golden, eight composition migration cases, all 16 VERSION migration cases and two external core ownership/handoff API cases. Initial failures were the misplaced store reference in the core-only downstream target, stale current-format hostile snapshot domains, an independently derived snapshot self-hash missing its empty slot, and the now-current VERSION 12 future-refusal vector; corrections preserve the original semantic refusals and use no system-temp fallback. All logs from local-atomic-handoff-format-msrv.log through local-atomic-handoff-migration-completion-msrv.log are retained, with formatting/source-size/diff checks passing; full stable and new portable/native format acceptance remain pending.

The already committed replay-anchor checkpoint was pushed only after its complete stable gate and the prior nine-job review were terminal; new live CI 37455127828 is pinned to 413d6a480e650299efddda03942264f7c5f88c86 and excludes this later format unit. No superseding push is authorized while it remains live. Cold event-only recovery still needs original physical-store/namespace binding and actual process/MCP acceptance before production constructor routing can be reconsidered; the earlier rejected candidate remains unapplied. Task 9401 remains in progress, merged_as remains empty, and production/gate/executable-byte acceptance flags stay false.

### VERSION 12 CLI fixture review — 2026-10-06

The frozen d7cfa63 stable workspace debug gate failed five assertions across
three CLI targets: old VERSION 11 transcript expectations and snapshot root
material missing execution_handoffs; release, lint, documentation and later
gates did not run, and this is not a full stable acceptance verdict.
The scope recorded memory.swap.current=0 and no OOM kills.
The audit root was independently derived with Python standard-library SHA-256,
first reproducing historical root/4, then adding the empty handoff collection
and using domain fsm:state-root:5; the resulting root is
sha256:517f6e5ee124ea219f98ce797a6e9ab745844ac869696b68ccd38aab90aec585.
The corrected adversarial snapshot fixtures preserve all original divergence
assertions. Serial stable checks under verified MemoryMax=1G/MemorySwapMax=0
passed all 3 cli_golden, 77 review_regressions and 7 audit_golden tests.
The first audit retry reused a binary whose manifest directory named the frozen
checkout; rebuilding the source target made it read the current fixtures.
Logs are retained under the task cache as local-cli-format12-correction-final.log
and local-cli-format12-audit-rebuilt.log, alongside the original failed gate.
Task 9401 remains in progress; full workspace, portable and installed native
acceptance of VERSION 12 and cold handoff delivery remain incomplete.

### Matched-stop retirement race review — 2026-10-06

Review run 37455127828 at exact source 413d6a4 remains live with both Windows
jobs running; the four Ubuntu/macOS gates, zero-deps and native MSRV jobs
are successful, while native stable failed private_exec_status with ENOENT
during live domain revocation, as retained in its failed authority artifact.
The correction permits continuation after a failed early live revocation only
when a fresh metadata lookup confirms the original cgroup is absent; all
original binding, handoff, manager identity/policy and durable revocation
checks still run, and independent closure remains necessary.
Surviving, replaced or unreadable groups preserve the original refusal.
Stable fsm-execute all-target Clippy and library tests, formatting and source
size checks passed serially under verified 1 GiB/no-swap limits; the retained
log is local-matched-stop-retirement-review.log in the task cache.
This local result does not prove the installed race is corrected, nor
authorize production defaults: genuine installed native acceptance remains
required, and the earlier review run will not be superseded while live.

### Terminal replay-anchor review — 2026-10-06

Review run 37455127828 is terminal at exact source
413d6a480e650299efddda03942264f7c5f88c86: all six portable
Ubuntu/macOS/Windows stable/MSRV jobs, zero-deps and native MSRV passed;
native stable failed private_exec_status during early live revocation with
ENOENT, and the parent verdict is failure. The failed authority artifact
and terminal job logs are retained in task-cache ci-37455127828.
Independent verification of the completed native MSRV artifact proves
all 81 frozen cases at rustc 1.89.0 (29483883e 2025-08-04), with
gate_released=false and executable_bytes_verified=false.
This run predates VERSION 12 persistence and the matched-stop correction.
The local cf607de stable host gate passed its full debug workspace phase
and remains live in release compilation; subsequent phases are unproven.
Cold host implementation and controls exist only as unapplied task-cache
drafts, and production constructors remain unchanged.
A successor review may now test the committed VERSION 12 and race fixes
without cancelling any job from this terminal review.

### VERSION 12 and retirement correction review — 2026-10-06

Successor run 37460020929 at exact source
665a71b39b403ec9ce03846619a194e7f85804c0 remains live in portable
gates; both completed native stable/MSRV jobs passed. Both retained
artifacts independently verify all 81 frozen cases with source identity
and compiler identity checked, gate_released=false and
executable_bytes_verified=false. Stable is rustc 1.99.0
(b940084d7 2026-09-28); MSRV is rustc 1.89.0
(29483883e 2025-08-04). This does not erase the prior native failure
or prove the newly drafted cold controls, which are unapplied.
The local cf607de gate passed full debug and release workspace suites,
then failed all-target Clippy when the shared target exposed older core
metadata lacking the source-present handoff APIs. The failure is retained
in local-cf607de-stable-host-gate.log; no complete host verdict is claimed.
A serial fresh-target continuation for all-target Clippy, warning-denied
rustdoc, zero-deps and full embed acceptance is live under verified
MemoryMax=1G/MemorySwapMax=0, with log
local-cf607de-fresh-check-gate.log in the task cache.

### Cold post-ack host implementation — 2026-10-06

Implemented bounded event-only adoption and original-authority-checked delivery
in the shared public tick paths, independent of handler tables and execution
capacity, with exact durable membership, fair selection and parked refusals.
Warm original completions retain their established retry path. Genuine cold
process/MCP controls are wired into the provisioned broker matrix and cover
two seals, removed handler tables, disabled events, copied physical stores,
read-only/contended writers and unchanged allocation count; they have compiled
but have not executed in an installed environment at this source.
Stable/MSRV fsm-execute all-target Clippy, formatting and source-size checks
passed; stable library 28/28, native-mode 1/1 and existing serve_modes 12/12
passed serially in a dedicated target under verified 1 GiB/no-swap limits.
Logs are local-cold-handoff-corrected-review.log,
local-cold-handoff-msrv-and-embedded-review.log and
local-cold-handoff-embedded-review.log in the task cache; the mistaken singular
serve_mode invocation is retained, and the correct serve_modes target passed.
The preceding committed cf607de implementation completed its stable debug and
release workspace tests plus fresh-target all-target Clippy, rustdoc,
zero-deps and embed acceptance; its shared-target Clippy failure remains
retained separately and is not overwritten by the fresh-target pass.
Current cold implementation full stable/portable/native acceptance is pending;
review 37460020929 still has live portable jobs and will not be superseded.
Task 9401 remains in progress and production constructors remain legacy.

### Complete cold host stable gate — 2026-10-06

The serial local gate at exact code source
2463384c27ca72b3ba705050817e4ce3d983648f completed with exit 0:
formatting, source-size checks, full debug and release workspace tests,
workspace all-target Clippy, warning-denied rustdoc, zero-dependency and
full embed acceptance all passed in the dedicated cold-handoff target.
The 1 GiB memory and zero-swap limits were verified before Cargo started;
the retained log is local-2463384-stable-host-gate.log in the task cache.
This is local stable acceptance, not installed cold or portable acceptance.
The preceding review 37460020929 remains live only in its two Windows
jobs; all seven completed jobs passed, and its native artifacts independently
verify both 81-case results at exact source 665a71b.
Additional genuine conflicting-key and rejected-original-key controls are
prepared only in the task cache and are not included in this passed gate.
Production constructors remain legacy and task 9401 remains in progress.

### Rejected and conflicting cold handoff controls — 2026-10-06

Added genuine process and MCP controls to the provisioned broker matrix:
an accepted foreign event burns the original derived event key, and an
actual EventRejected record burns the original key before cold restart.
Both controls retain the exact outstanding handoff through two seals,
physical-copy/read-only/contention refusals, healthy-writer delivery refusal,
repeated parked ticks and a final cold read-only reopen; original execution
ownership stays resolved and the allocation counter remains unchanged.
The successful warm and cold handoff assertions remain in place.
Formatting, source-size checks and stable/MSRV fsm-execute all-target Clippy
passed serially under verified 1 GiB/no-swap limits, with terminal exit 0
in local-cold-handoff-retention-controls-review.log in the task cache.
These new controls have compiled but have not executed in the installed
native environment; they are absent from the earlier 665a71b artifacts.
Review 37460020929 is still live in both Windows jobs, so its review branch
has not been superseded; all seven completed jobs passed.
Task 9401 remains in progress and production constructors remain legacy.

### Complete retention-control gate and terminal prior review — 2026-10-06

Local stable gate session 49845 completed with exit 0 at exact source
f5ba23e25bb47d9e79ce2c99432b1db76b5b4577: formatting, source-size,
full debug/release workspace tests, all-target workspace Clippy,
warning-denied rustdoc, zero-dependencies and full embed acceptance passed.
The scope verified MemoryMax=1G/MemorySwapMax=0 before Cargo; observed scope
counters showed zero swap use and no OOM events during execution.
The retained log is local-f5ba23e-retention-controls-stable-gate.log in the
dedicated task cache. Installed execution of the new cold controls remains
pending; ignored provisioned controls compiling is not runtime acceptance.

Prior review 37460020929 is terminal success at exact source
665a71b39b403ec9ce03846619a194e7f85804c0: all nine native/portable/zero-deps
jobs passed, with final-run.json and nine nonempty completed job logs
retained under ci-37460020929 in the task cache. Both native artifacts have
separate independent 81-case verification results; executable-byte and
production gate acceptance remain false. This proves the earlier source,
not the later cold-host implementation or retention controls.

Automatic approval review rejected the successor review-branch push because
it did not recognize explicit authorization for the private repository
payload and destination; exact approval was requested and remains pending.
No push workaround was attempted. Local work continues independently.
Task 9401 remains in progress; production defaults remain legacy, and bounded
shutdown, reconciliation and full lifecycle crash/concurrency proof remain
incomplete.

### Bounded durable ownership discovery — 2026-10-06

Added execution_ownership to the production fsm://executor resource, with
claim-era admission enabled and counts of unresolved runs, stopped runs and
outstanding acknowledged handoffs from the observed verified store prefix.
Unavailable stores return null, while read-only stores can report counts
without a writer; external_executor remains unknown. Counts expose no run
identifiers, commands, native paths or results and do not claim live process
health, native capability, closure or operator reconciliation.
SPEC, API policy, embedding and release documentation move with this field.

Serial verified 1 GiB/no-swap session 31212 completed with exit 0:
format/source-size checks, stable/MSRV CLI all-target Clippy and all three
mcp_executor tests passed. The new test creates an actual journal claim,
reads discovery under an independent live writer, checks the exact nonzero
count and unchanged complete file bytes, and checks unavailable null and
unchanged unknown external-executor status. The retained log is
local-ownership-discovery-check.log in the task cache. This test does not
prove native closure, actual stopped/handoff recovery or production routing.
Full changed-source host and portable gates remain pending; earlier gates
are not attributed to this additive resource change. Task 9401 remains in
progress, downstream lifecycle tasks remain incomplete, and the rejected
remote push still awaits exact authorization.

### Complete durable discovery stable gate — 2026-10-06

Serial session 78310 completed with exit 0 at exact committed source
f8c38f920e92da1b19fcff5c5581b41f29fb9011. Formatting, source-size checks,
full debug and release workspace suites, all-target workspace Clippy,
warning-denied rustdoc, zero-dependency checks and full embed acceptance
passed. Before Cargo the scope verified 1 GiB RAM and zero swap; observed
memory counters showed zero swap and no OOM events. The retained log is
local-f8c38f9-ownership-discovery-stable-gate.log in the task cache.
This includes the named handoff test refactor and new discovery metadata,
but ignored provisioned native controls remain unexecuted locally.
No portable/installed acceptance is inferred from this local stable gate.
The successor remote push remains pending exact approval following its
automatic rejection; production selectors and all production acceptance
flags remain unchanged, task 9401 stays in progress, and the full plans
20–23 objective is not complete.

### Complete retirement-guard stable gate — 2026-10-06

Serial session 71611 completed with exit 0 at exact code source
1d79e255c7508417249d2c59e4c517c9f5ef7bcd. Formatting, source-size checks,
full debug/release workspace suites, workspace all-target Clippy,
warning-denied rustdoc, zero-dependencies and full embed acceptance passed.
The task scope verified MemoryMax=1G/MemorySwapMax=0 before Cargo; observed
scope counters showed zero swap use and no OOM events. The retained log is
local-1d79e25-retirement-stable-gate.log in the task cache.
This validates local stable behavior including retained original-ack refusal,
not provisioned cold host controls, native closure or actual production
routing. Installed/portable acceptance remains pending exact authorization
for the successor push following its automatic approval rejection.
Task 9401 stays in progress and production defaults remain legacy; shutdown,
reconciliation and the complete plans 20–23 objective remain incomplete.

### Closure-request foundation and current verification — 2026-10-06

Entry/completion separation 4663bd3 passed the complete stable host gate in
session 10029, terminal exit 0; evidence is recorded in d8a9483 and retained
as local-4663bd3-completion-stable-gate.log. Runtime source stayed unchanged
through that gate, with only cancellation review documentation committed.

Provisional Linux NativeShutdown landed at
37584c2a1514ee580559c47e54af09b6597d32a0 with its SPEC/API-policy/embedding/
release notes, public-surface inventory and downstream API check. It requests
native closure from an original verified durable snapshot without acquiring
the writer, keeps execution transport separately owned and returns opaque
proof only after full original receipt authentication. Session 55144 passed
stable/MSRV executor all-target Clippy, 32 library tests and the downstream
API/public-surface tests with verified 1 GiB/no-swap limits, terminal exit 0;
local-native-shutdown-check.log is retained. Full source 37584c2 stable gate
session 82449 is still running; no terminal verdict is claimed here.

No installed runtime acceptance or production wiring is proved by those
negative and API tests. Before-binding claims still require separate proof,
and interrupted application, public lifecycle states/controls, bounded report
deadlines and quiet/blocked stdio progress remain unfinished. Task 9402 stays
planned, task 9401 stays in progress, the count stays 3/7 and all production
acceptance flags stay false. Plans 20, 21 and 23 remain planned. The private
remote push remains subject to the earlier automatic rejection and pending
exact destination/payload authorization; no push is attempted here.

### Closure-client gate and binding correction — 2026-10-06

Session 82449 completed the full stable host gate for unchanged runtime source
37584c2 with exit 0; local-37584c2-shutdown-stable-gate.log is retained under
verified 1 GiB/no-swap limits. Read-only inspection confirms this host has no
protected helper or authority directory, so native runtime proof is unexecuted.
Review then required full protected binding validation before closure side
effects, replacing the client's allocation-only request with close-claimed and
retaining independent receipt authentication afterward. Session 80380 passed
focused stable/MSRV all-target checks, 32 library tests, API inventory and the
authority binding guard test; the retained log is local-claimed-closure-check.log.
Installed broker dispatch and full corrected-source gates remain pending.
The count remains 3/7, task 9401 in progress and task 9402 planned; production
defaults, remote-push authorization and all acceptance flags remain unchanged.

### Complete bound-closure-control stable gate — 2026-10-06

Session 87044 completed exit 0 at exact source
18ddf9b9ac9c7026f82f8dbb3748f59796456f94 with runtime source frozen throughout.
Formatting/source-size, full debug/release workspace suites, workspace all-target
Clippy, warning-denied docs, zero-dependencies and full embedding acceptance
passed under verified 1 GiB/no-swap limits. The retained log is
local-18ddf9b-bound-closure-controls-stable-gate.log in the task cache.
Provisioned process/MCP bound-closure axes compile but remain unexecuted;
older CI native reports do not prove these axes or the new close-claimed action.
No protected helper is installed on this host, and the private successor push
still needs exact destination/payload authorization after automatic rejection.
Plan progress remains 3/7: ownership integration is in progress, shutdown and
reconciliation are planned, production selection remains legacy and all
production acceptance flags stay false. The full plans 20–23 goal is incomplete.

### Receipt-only interrupted settlement foundation — 2026-10-06

NativeShutdown::settle_interrupted applies retained authenticated original
closure proof through the existing stop and Interrupted transactions under the
healthy original writer; it preserves pending work and retry counters and emits
no acknowledgement or outcome event. Review checked physical-store proof,
original claim/hash, non-interrupted stopped refusal, exact replay after claim
consumption and conservative refusal when original request keys are pruned.
Neither owned transport nor local capacity is released by this method.

Focused session 96331 finished exit 0 with verified MemoryMax=1G and
MemorySwapMax=0: formatting/size, stable and Rust 1.89 executor all-target Clippy,
32 library tests, downstream API/public-surface checks and original-binding
refusal passed; local-native-interruption-check-v3.log is retained in the task
cache. Initial compilation and item-ordering findings were corrected before
this successful run. Provisioned process/MCP tests now assert read-only refusal,
exact stopped/settled records, preserved pending state and duplicate replay after
cold reopen, but compile-only evidence does not prove those native behaviors.

Full stable host verification remains pending for this source. Actual stopped
success/failure, write failure between stop and settlement, and interrupted
shutdown in the production driver still need runtime coverage. Ownership scope
must distinguish locally admitted work from observed claims of other live
executors before applying shutdown. Progress remains 3/7, task 9401 in progress,
task 9402 planned and production/native acceptance flags false; plans 20, 21 and
23 remain planned and private remote publication remains authorization-dependent.

### Shutdown ownership transfer review — 2026-10-06

SHUTDOWN-OWNERSHIP-REVIEW.md records the observed/local boundary and the
ClaimUncertain refresh transfer that must preserve local provenance before
removing an admission. The full stable gate is live as session 4737 with runtime
source unchanged; it began before commit 7571fd7 while that exact runtime diff
was already present, so its initial printed HEAD ef3e4a8 is not the tested runtime
revision. Documentation-only review additions do not change that runtime.
No terminal gate verdict or installed shutdown acceptance is claimed here.

### Full interrupted-settlement stable gate — 2026-10-06

Session 4737 completed exit 0: formatting/source size, debug and release
workspace suites, workspace all-target Clippy, warning-denied documentation,
zero dependencies and full embedding acceptance passed under verified 1 GiB
memory and zero-swap limits. Runtime source stayed identical to commit 7571fd7;
the gate began before that commit with its complete runtime diff present and
printed the earlier HEAD ef3e4a8, as recorded above. Later 4a09824 changed only
review documentation. The retained log is local-native-interruption-stable-gate.log.
Provisioned interruption axes remain compiled but unexecuted; production
selection remains legacy, task 9402 planned and all native acceptance flags false.

### Local admission provenance implementation — 2026-10-06

Owners now preserve local publication provenance independently of helper phase;
observed claims cannot request local helper cancellation. Verified snapshot
publications transfer from ClaimUncertain before reservation removal, matching
instance/effect, original domain, handler fingerprint and retry policy. Failed
matching retains the reservation; repeated observation preserves local origin.
Successful publication also marks an already-retained matching owner local.

Review corrected domain-only refresh, omitted-observation transfer and
already-retained publication paths before verification. Session 57311 completed
exit 0 with verified 1 GiB/no-swap limits: formatting/size, stable and MSRV
all-target executor Clippy, 34 library tests, downstream API inventory and the
original binding guard passed; local-native-provenance-check-v2.log is retained.
The first check exposed an unused retained-domain accessor; it was restored and
used rather than discarding uncertainty metadata. Classification fixtures prove
no native closure, and paired live executor and actual append-failure transfer
coverage remain pending. Full stable verification remains pending for this unit.
Plan count stays 3/7, task 9401 in progress and task 9402 planned; production
selection and all native acceptance flags remain unchanged.

### Owned stdio lifecycle route review — 2026-10-06

OWNED-STDIO-REVIEW.md records the owned production entry that can preserve the
borrowed session API, the single clock/writer mutation owner, elicitation reply
demultiplexing and feed/output join constraints. The protocol and lifecycle
paths must remain independent while actual stdio or writer work blocks; bounded
queue acceptance cannot claim actual delivery or native cleanup.
Session 3366 remains live for runtime source 450beb8; this review changes only
documentation and does not invalidate that runtime freeze. Output/retirement
drafts remain cache-only and untested, and no production control or acceptance
flag is promoted. Formatting and diff checks apply to this documentation unit;
its code gates are represented by the still-running frozen runtime gate.

### Complete provenance stable gate — 2026-10-06

Session 3366 completed exit 0 for runtime source
450beb8aae00fe0b22c3e01c1d1197d452da9cc9, unchanged throughout verification.
Formatting/size, debug/release workspace tests, workspace all-target Clippy,
warning-denied documentation, zero dependencies and full embedding acceptance
passed under verified 1 GiB/no-swap limits. Later 520deca changed only review
documentation. The retained log is local-native-provenance-stable-gate.log.
Native paired-owner/public shutdown runtime acceptance is unexecuted, so task
9402 remains planned, progress 3/7 and all production acceptance flags false.

### Bounded queued protocol output foundation — 2026-10-06

Notifier::queued provides opt-in atomic frame admission to an owned output
worker, with explicit OutputControl close/drain/failure observation. Its 256-frame
and 8 MiB retained-allocation budgets include the actual blocked in-flight
frame; memory is dropped before its charge is released. Existing synchronous
construction remains available, and queue acceptance claims no actual delivery.
The production owned stdio route and native lifecycle driver remain unimplemented.

Session 84533 finished exit 0 with verified 1 GiB/no-swap limits: formatting/size,
stable and MSRV CLI all-target Clippy, five queue boundary/I/O tests, seven
existing MCP shutdown tests and two downstream notifier tests passed. The log is
local-queued-output-check-v2.log. The downstream blocked-writer test reaches the
public constructor and exact 256-frame/plus-one boundary; clone ordering asserts
actual canonical bytes. Full stable verification remains pending for this unit.
No native runtime acceptance is inferred from output tests; plan progress stays
3/7, task 9401 in progress, task 9402 planned and production acceptance false.

### Complete queued-output stable gate — 2026-10-06

Session 11985 completed exit 0 at exact runtime source
d20ee063e80d82e22310c06dee21a16c74097c1a, unchanged throughout the gate.
Formatting/size, debug/release workspace suites, all-target workspace Clippy,
warning-denied docs, zero dependencies and full embedding acceptance passed
under verified 1 GiB/no-swap limits; local-queued-output-stable-gate.log is
retained. Production queue/lifecycle integration and native shutdown acceptance
remain unimplemented/unexecuted; no plan task or acceptance flag is promoted.

### Shared bounded protocol input correction — 2026-10-06

Ordinary serve framing and SessionIo reverse replies now share the 16 MiB
wire-byte limit excluding LF. Oversized tails drain through borrowed chunks
without allocating a remainder, preserve the next frame and propagate original
read errors. SessionIo refuses oversized replies with InvalidData through the
existing io/read path; this remains blocking input and grants no silence bound.

Session 14248 completed exit 0 with verified 1 GiB/no-swap limits: formatting/size,
stable and MSRV CLI all-target Clippy, four framing tests, two public-entry
chunked-input tests, nine elicitation tests, six skeleton tests and two queued
output tests passed. The retained log is local-bounded-input-check.log. The
chunked reader traps unbounded read_line/read_until calls, reaching both actual
public paths; exact limit, plus-one and next-frame behavior are asserted.
Full stable verification was subsequently completed as recorded below. Native driver/control
integration is still incomplete, so progress stays 3/7, task 9402 planned and
production acceptance false; no output/input tests substitute for native proof.

### Complete bounded-input stable gate — 2026-10-06

Session 63003 completed with exit 0 at exact runtime source
568638d67d4ad96d8cda0967d94fe82e1f7ed8be; the runtime source remained unchanged
throughout verification. Formatting/size, debug/release workspace suites,
all-target workspace Clippy, warning-denied documentation, zero dependencies
and full embedding acceptance passed under verified 1 GiB/no-swap limits.
The retained evidence is local-bounded-input-stable-gate.log. Installed native
controls and other platform axes remain unexecuted for this source; production
integration and all existing task/acceptance statuses remain unchanged.

### Admission-free native completion pass — 2026-10-06

The public service::observe_admitted_with pass observes retained native
transport, recovers original ownership and fairly applies at most one original
completion/handoff action through the healthy original physical writer, without
pending admission, preparation starts, bound entry, retries or machine deadline
polling. This is a writer-dependent integration seam, not the independent
shutdown/report driver required by task 9402.

Focused session 9284 completed exit 0 under verified 1 GiB/no-swap limits:
formatting/size, stable/MSRV execute all-target Clippy, 34 library tests, native
shutdown API, all 16 public-surface tests, the authority binding guard and all
seven tick tests passed; local-admitted-pass-check-v3.log is retained. Earlier
runs failed because the inventory required both the function and its reexport;
both entries were corrected before the passing run. The scheduling exclusion
test preserves an actual pending effect, due deadline, journal head and state.
The provisioned bound-owner probe now also asserts no protected launch artifact
or journal mutation across these passes, but is only compiled locally, not
executed against installed authority. Full stable verification remains pending;
progress remains 3/7 and production/native acceptance flags remain false.

### Complete admission-free pass stable gate — 2026-10-06

Session 46365 completed exit 0 at runtime source 94d3f5a, unchanged throughout
the gate; the later dbffcaf commit contains only the control-pump review.
Formatting/size, debug/release workspace tests, all-target workspace Clippy,
warning-denied documentation, zero dependencies and full embedding acceptance
passed under verified 1 GiB/no-swap limits. Live scope checks also confirmed
MemorySwapCurrent=0 and no OOM events. Evidence is retained in
local-admitted-pass-stable-gate.log. Installed native controls and other
platform axes remain unexecuted for this source, and no task or production
acceptance flag is promoted.

### Original interrupted native retirement — 2026-10-06

Runner::retire_native_interrupted now releases an exact locally admitted owner
only after authenticated original closure, replay of its exact Interrupted
settlement and reap/stdout EOF/stderr EOF of both execution and closure helpers.
It appends no records and preserves pending work; foreign observation, retained
completion, unhealthy/copied writers and missing/pruned replay refuse retirement.
The provisioned bound-interruption probe retains the original Runner/Scheduler,
checks pre-settlement and read-only refusal, observed foreign-owner exclusion,
successful local capacity release and stale repeated refusal; these actual
authority controls are compiled locally but remain unexecuted here.

Focused session 25243 completed exit 0 under verified 1 GiB/no-swap limits:
formatting/size, stable/MSRV execute all-target Clippy, 35 library tests, both
downstream native shutdown API tests, all 16 surface tests and the binding guard
passed; evidence is local-interrupted-retirement-check-v2.log. The first run
exposed a public-inventory blind spot for methods in private implementation
modules; the public wrapper now lives beside Runner::start_native and the
private implementation stays in native_host. Full stable verification remains
pending; task 9402, production lifecycle integration and installed acceptance
remain incomplete, with progress and acceptance flags unchanged.

### Complete interrupted-retirement stable gate — 2026-10-06

Session 3951 completed exit 0 at exact runtime source
14fbc1791229da5bea29ef508165352bc5216f72, unchanged throughout verification.
Formatting/size, debug/release workspace suites, all-target workspace Clippy,
warning-denied documentation, zero dependencies and full embedding acceptance
passed under verified 1 GiB/no-swap limits; evidence is retained in
local-interrupted-retirement-stable-gate.log. Installed authority controls and
other platform axes remain unexecuted for this source; all task and production
acceptance statuses remain unchanged.

### Shared native admission fence and local targets — 2026-10-06

NativeAdmissionControl now closes one original native runner's shared fence
without waiting for its worker, writer or native I/O. Authorization checks
cover queue reservation, preparation dispatch, publication and bound entry;
already authorized publication retains and binds its original claim while a
closed entry fence forbids launch. Owned observation cancels never-requested
queues and cleans delivered preparations without discarding unknown allocation
or uncertain publication. Completion/handoff application remains eligible.
Runner::local_native_claims borrows only original local execution-retained
claims in run-ID order; it excludes foreign observation and consumed owners,
and does not enumerate unclaimed preparations or assert native liveness.

Focused session 10946 completed exit 0 under verified 1 GiB/no-swap limits:
formatting/size, stable/MSRV execute all-target Clippy, 39 library tests, four
downstream API tests, all 16 surface tests and the binding guard passed;
local-native-admission-fence-check-v2.log is retained. The first run found a
needless return, corrected before the passing run. Tests cover a held worker,
independent successor handles, queue/unknown-allocation retention and metadata
provenance ordering; none constructs native closure evidence. The provisioned
probe now closes the fence on an actual bound owner and checks public ticks
cannot publish a launch artifact or mutate the journal, but remains compiled
and unexecuted locally. Full stable verification is pending. This closes
admission only: independent bounded reports, endpoint control and production
stdio shutdown remain incomplete, and no task or acceptance flag is promoted.

### Complete admission-fence stable gate — 2026-10-06

Session 9885 completed exit 0 at exact runtime source
716666b31fe203928ca1588110fd09d5c0329467, unchanged throughout verification.
Formatting/size, debug/release workspace suites, all-target workspace Clippy,
warning-denied documentation, zero dependencies and full embedding acceptance
passed under verified 1 GiB/no-swap limits; evidence is retained in
local-native-admission-fence-stable-gate.log. The provisioned native controls
and other platform axes remain unexecuted for this source, and production
shutdown integration remains incomplete; no task or acceptance status is promoted.

Owned native lifecycle driver implementation now takes the original durable
writer and native execution components, exposes independently waitable cloned
control, and explicitly polls admission-free original completion plus bounded
fair claim-bound closure; interruption waits for execution helper reap/EOF and
keeps authentic completion policy, and Stopped follows actual writer release.
Focused stable session 97397 passed executor library/downstream writer-control
tests, stable/MSRV all-target Clippy, public API inventory and source-size checks
under asserted 1 GiB RAM/zero-swap limits; see OWNED-DRIVER-REVIEW.md and retained
local-owned-lifecycle-focused-v3.log. Full changed-source stable gate and actual
installed driver/production stdio/endpoint/CLI/signal controls remain pending;
no task is promoted and progress remains 3/7.

Frozen f90871b full stable session 85037 remains live; driver follow-up review
found admission closure alone can prematurely release an empty transferred
writer, with a correction and downstream regression prepared outside the frozen
source, as recorded in OWNED-DRIVER-REVIEW.md; this finding remains open until
the gate terminates and changed-source verification passes.

Owned driver follow-up corrected admission closure versus explicit stop and
proved the downstream writer-retention regression fails with only the old
predicate restored (mutation session 91927, terminal 0 after expected test
exit 101 and successful restored tests). Focused stable/MSRV checks passed
(session 18750), and actual live-bound process/MCP native driver controls now
compile (11667) while remaining unexecuted here. Known-defective f90871b full
gate 85037 was explicitly stopped and confirmed terminal 143 before mutation;
its partial log is not full acceptance. Corrected-source full stable gate and
production/installed acceptance remain pending, with progress unchanged at 3/7.

Corrected owned-driver runtime f50d95868f8c0bcac5800aa24dbea5b5f7cf3d36
passed the full stable host gate in session 47032, confirmed terminal 0: format,
source size, workspace debug and release tests, workspace all-target Clippy,
warning-denying workspace documentation, zero-dependency and embed acceptance
all passed under asserted 1 GiB RAM and zero swap. The complete retained log is
local-owned-driver-request-fix-stable-gate.log. This resolves the local full-gate
obligation for the writer-release correction and compiled live-bound probes;
installed native execution, portable CI and production stdio/endpoint/CLI/signal
integration remain unexecuted or unfinished, and progress stays 3/7.

Opt-in owned native MCP session composition now integrates one writer owner
and clock, bounded single-reader input shared with elicitation, idle admitted
observation, queued output and nonjoining owned-feed stop admission; native
cleanup/writer facts and actual output delivery are separate, with original
deadline reuse and explicit timeout facts. Focused stable/MSRV session 24844
passed library/downstream owned and borrowed session, elicitation, lifecycle
and public API checks; worker-bound mutation session 24100 confirmed an exact
limit refusal fails when only that guard is disabled and restored tests pass.
See OWNED-SESSION-REVIEW.md and retained logs. This does not change current CLI
selection or supply production/installed tree, endpoint, CLI stop, signals or
paired standalone acceptance; full changed-source stable gate remains pending
and progress stays 3/7 with no task promotion.


### Complete owned-session stable host gate — 2026-10-06

Session 30244 completed with exit 0 against exact runtime
3157f1cd12efd1d7e60495fe525705c197a8a6f4, under verified
MemoryMax=1G and MemorySwapMax=0; the retained log is
local-owned-stdio-integration-stable-gate.log in the dedicated task cache.
Formatting, source-size checks, debug and release workspace tests,
all-target workspace Clippy, warning-free workspace documentation,
zero-dependency tests and embed acceptance all passed.
This completes the local stable host gate for the opt-in owned session entry;
current production binary selection, installed native controls, portable CI,
endpoint/CLI stop, signals and paired standalone acceptance remain unproved.
Tasks 9401/9402 retain their existing states and plan progress remains 3/7.


### Opt-in exact-incarnation local control transport — 2026-10-06

Implemented the actual-driver Linux local control publisher and bounded client,
with private discovery, strict identity/schema validation, finite deadlines,
bounded nonblocking connections, abort admission independent of waiting drains,
charged detached client workers and exact-inode cleanup preserving replacements.
LOCAL-CONTROL-REVIEW.md records the rejected draft findings and resolution.
Focused stable/MSRV checks and twelve real-socket/actual-writer tests passed in
session 34926; mutation session 27776 proved five downstream guards load-bearing
and restored tests passed, all under asserted 1 GiB RAM and zero swap.
These opt-in library APIs do not wire execute stop, change current production
selection, prove installed native trees or provide paired standalone/signal
acceptance; the full changed-source stable host gate is still required.
Task 9401 remains in progress, 9402 remains planned, and progress remains 3/7.


Full local control endpoint stable host gate session 47222 completed with exit
zero against exact runtime 9d2439f21e5a3f1d8f51a7f3cbfaddaf8f4bcae6, under
verified MemoryMax=1G and MemorySwapMax=0; retained
local-control-endpoint-stable-gate.log covers formatting, source size, debug
and release workspace tests, all-target Clippy, warning-free documentation,
zero dependencies and embed acceptance. The documentation-only afb7f2b
review did not change that frozen runtime. Portable/installed native and
production selector acceptance remain unproved; task states remain unchanged.


### Operator execute stop implementation — 2026-10-06

Added the production-dispatched stop command using private actual-driver endpoint
control without opening the journal writer; its output preserves actual stopped,
actual uncertain and unknown transport facts separately. CLI-STOP-REVIEW.md
records the design, four real-binary tests, exact timeout boundaries, stable/MSRV
checks, nine argument tests, 77 regressions and two load-bearing guard mutations
followed by restored passing tests (sessions 44567 and 89989, terminal zero).
The command can control explicit library publishers; current production executor
selectors do not publish the native owned endpoint. Full changed-source stable
gate, native tree/production publication, signals, paired standalone and portable
acceptance remain pending, and plan progress/task states remain unchanged at 3/7.


### Complete execute stop stable host gate — 2026-10-06

Session 45893 completed with exit zero against exact CLI runtime
cd0213b3df7aa6a9dac4afa47c1cfd7b98b124fc under verified
MemoryMax=1G and MemorySwapMax=0; execute-stop-stable-gate.log is retained.
Formatting, source size, debug and release workspace tests, all-target
workspace Clippy, warning-free documentation, zero dependencies and embed
acceptance all passed. Subsequent bc6aa3b/f3d2086 ownership reviews changed
only documentation and did not alter the frozen tested runtime.
This proves the local stable host gate for execute stop, not current native
server publication, paired actors, installed trees, signals or portable CI.
Plan progress remains 3/7 with task 9401 in progress and 9402 planned.


### Shared closure pump extraction — 2026-10-06

Extracted the original claim-bound closure map/cursor into the private lifecycle
Closures component, shared by the current owned driver and the upcoming paired
writer strategy. Start still requires a verified original snapshot; transport
polling/reaping is separate from optional healthy-writer settlement. The current
owned driver passes its same original writer, retaining exact behavior, four
helper/attempt bounds, fair cursor, authentic completion priority, actual helper
retirement and original claim matching; writer release still precedes Stopped.
No public API, journal bytes, hashes, error codes or production selectors change.

Focused session 70929 exited zero under asserted 1 GiB RAM and zero swap,
passing stable/MSRV CLI/executor all-target Clippy, 46 executor library tests,
three downstream owned lifecycle tests, sixteen public surface tests, four owned
session tests, twelve transport tests and four real stop binary tests; retained
closure-pump-refactor-check-v2.log records the run. Initial session 92984 failed
at the moved error_line namespace, corrected before the successful pass.
This is the required shared implementation extraction, not a completed paired
driver or installed no-writer native control. Full changed-source stable gate
and remaining paired/production/native/signal acceptance are still required;
plan progress remains 3/7 and task states remain unchanged.

### Paired lifecycle implementation and focused review — 2026-10-06

The shared closure extraction's full stable gate (session 65333) exited zero
against runtime 156a750082a43aa1bef22a74df5faef8a13db55d with asserted
MemoryMax=1G and MemorySwapMax=0; closure-pump-refactor-stable-gate.log
records formatting, size, debug/release workspace tests, all-target Clippy,
documentation, zero dependencies and embedding acceptance.

Added opt-in PairedNativeExecutor with an original physical-store pin and verified
reader snapshot, shared claim-bound closure observation before writer acquisition,
temporary healthy-writer settlement, and actual writer release before publication.
Explicit ticks retain ordinary scheduling; admitted polling does not advance due
machine deadlines or admit pending work. The actual paired endpoint publisher uses
the driver's pinned identity and refuses replaced paths and stopped drivers.

Focused paired-lifecycle-check-v3.log reaches its final public surface suite with
all sixteen tests passing; its process handle is now absent, so no terminal exit
code is inferred from the missing handle. The log also records stable/MSRV Clippy,
executor and owned lifecycle suites, three paired lifecycle tests, owned session
and transport suites, and five real CLI stop tests. Earlier focused session 23955
exited zero before the additional paired CLI test and inventory regeneration.
The held-clock test proves a real temporary writer cannot be reported released;
empty actor stop succeeds while an independent writer remains held, and idle
observation preserves journal records and due instance state.

These tests use empty native inventories: installed nonempty native closure,
two live native executors, production selection/publication, signals and full
changed-source gates remain required. Progress stays 3/7; task states are unchanged.

### Paired physical replacement regression — 2026-10-06

The actual directory replacement test requests abort against an empty actor,
replaces its original store with a different healthy directory, and observes
Uncertain with incomplete inventory; restoring the original inode permits a
later poll to reach Stopped. PAIRED-LIFECYCLE-REVIEW.md records the ownership,
writer-release and physical-prefix review, including remaining race limits.
Focused session 67012 exited zero under asserted 1 GiB RAM and zero swap,
passing formatting/size, stable and MSRV all-target executor/CLI Clippy, executor
library, owned/paired lifecycle, owned session, transport, CLI stop and public
surface checks. The initial narrower invocation stopped on formatting before
tests; formatting was corrected before this successful run. This adds a physical
identity refusal test, not installed native or production acceptance; statuses
remain unchanged.

### Standalone integration boundary review — 2026-10-06

STANDALONE-INTEGRATION-REVIEW.md identifies production obligations before
replacing service::run: retain structured writer-contention outcomes for the
exclusive three-tick contract, isolate blocked diagnostic output from ownership
observation, observe stop independently of ordinary scheduling intervals, and
publish/retire the exact actor endpoint. Current production selection remains
legacy and this review does not claim integration acceptance.

Full stable paired gate session 82897 is confirmed live at scope
run-p4109910-i87994532.scope, invocation 0af60b553a6042068b3feaea4e5c5129;
its log records exact starting HEAD 844a3f1422f5ef0b17a0e8aaf901bd4ffe2ce86e
and asserted MemoryMax=1G, MemorySwapMax=0. Runtime source is frozen during
the gate; this follow-up changes review documentation only. Completion and
changed-source full acceptance remain pending, with task statuses unchanged.

### Production composition drafts and native proof ordering — 2026-10-06

While session 82897 remains live, reviewed implementation drafts are retained
only in the dedicated cache; no tested runtime file has been changed. They add
an outcome-returning paired tick with actual writer-contention/resume tests,
an independently waitable explicit-request notification using the existing
Condvar, and an internal standalone log adapter sharing the current bounded
complete-frame worker. The owner-loop draft preserves exclusive failure reasons,
keeps admitted polling independent of ordinary scheduling intervals, separates
admission closure from explicit stop, validates options before driving, and
reports actual output drainage under the first shutdown deadline separately
from native cleanup. Blocked-output and request-wakeup tests are drafted but
unexecuted; no capability or production-selection claim follows from them.

Native paired writer-contention acceptance needs a new observation stage: the
existing root harness verifies closure only after the unprivileged child exits,
which cannot prove closure while its writer remains held. The cached paired
probe and parent helper coordinate a finite paused stage before writer release;
the parent must verify the protected full original receipt, original physical
store, original domain absence and successor survival while a real independent
writer open still refuses. The child keeps the original claim charged until
writer release and then verifies exact stopped/settled records without an ack
or machine event. Synchronization markers confer no closure authority. These
drafts still need harness wiring, compilation, sensitivity controls and actual
provisioned execution; the successor fixture is not a second live executor.

Plan progress remains 3/7 and all task states are unchanged; native production
integration and plans 20, 21 and 23 retain their full original obligations.

### Complete paired lifecycle stable host gate — 2026-10-06

Session 82897 exited zero against exact starting HEAD
844a3f1422f5ef0b17a0e8aaf901bd4ffe2ce86e, whose runtime implements the paired
driver and physical replacement regression; later commits during the gate
changed review/status documentation only. The retained
paired-lifecycle-stable-gate.log records formatting, size, debug and release
workspace tests, workspace all-target Clippy, warning-free workspace docs,
zero dependencies and embedding acceptance. The scope asserted MemoryMax=1G
and MemorySwapMax=0, and live checks observed zero swap throughout.

This completes local stable host verification of that runtime, not installed
nonempty native closure, two live actors, production selection/publication or
current portable CI. The reviewed cached drafts can now be applied and checked
as a separate runtime unit; task statuses and 3/7 progress remain unchanged.

### Structured paired ticks and request wakeup implemented — 2026-10-06

Applied the reviewed tick_reporting and wait_for_request APIs with normative,
API, embedding and release documentation and regenerated public inventory.
Actual contention tests hold an independent writer across three explicit ticks,
verify structured writer_unavailable, then release it and prove journaled
deadline progress resumes; stopped ticks make no unattempted contention claim.
Control tests cover request notification, unchanged original deadline,
incomplete cleanup facts, idle timeout and invalid bounds without admission
closure. No completion-publication authority is exposed.

Focused session 67502 exited zero under asserted MemoryMax=1G and zero swap:
format/size, stable/MSRV all-target executor/CLI Clippy, 48 executor library,
three owned and six paired lifecycle tests, owned session, transport, five CLI
stop tests and sixteen public surface tests passed; paired-wakeup-check.log is
retained. Production loop/queue wiring, mutation sensitivity, changed-runtime
full gates and provisioned acceptance remain required; progress stays 3/7.

### Nested public inventory coverage correction — 2026-10-06

Review found the source inventory's documented one-hop re-export limit omitted
control and paired-driver members exported through two private modules; previous
regeneration success did not prove those methods were enumerated. The scanner
now propagates named re-export members to a fixed point before public-module
filtering, adding 27 actual lifecycle inventory entries, including paired ticks,
request wakeup, report fields and shutdown-request methods. Renamed and glob
re-exports remain explicitly outside member resolution.

Focused session 92965 exited zero with asserted 1 GiB RAM and zero swap,
passing stable/MSRV Clippy, lifecycle/session/transport/CLI checks and seventeen
surface tests, including direct actual-member coverage; nested-surface-check-v2.log
is retained. Initial session 78840 exited 101 on the stale one-hop documentation
assertion and an expectation for a renamed enum variant outside the scanner's
stated scope; both expectations were corrected before the successful pass.
This repairs verification coverage without changing execution runtime or task
status; progress remains 3/7 and production/installed acceptance remains pending.

### Paired outcome and request-wait sensitivity — 2026-10-06

Mutation session 90239 exited zero under asserted 1 GiB RAM and zero swap:
forcing only paired tick writer_unavailable to false makes the actual held-writer
structured contention test fail with test exit 101; forcing only idle request
wait expiry to true makes the admission-preserving idle test fail with exit 101.
Each source file was restored in a finally block, then all 48 executor library
and six paired lifecycle tests passed. Git comparison confirms lifecycle source
matches the committed runtime after restoration; paired-wakeup-mutations.log
retains the expected failures and successful restored checks. This proves these
two regression assertions are sensitive, not production or installed acceptance;
plan progress and task statuses remain unchanged.

### Opt-in standalone native host pump — 2026-10-06

Implemented fsm_cli::standalone::run_paired around the actual paired driver:
explicit ordinary ticks preserve structured exclusive contention, admitted
polling and request wakeup remain independent of long scheduling intervals,
and initiating errors are retained while bounded abort cleanup is driven.
StandaloneReport separates shutdown, actual output drainage, diagnostic loss
and initiating failure. The internal log adapter reuses the existing complete
frame worker and its in-flight allocation accounting; saturated/oversized or
multiline lines are counted, broken output requests abort, and close/drain uses
the first request deadline without joining a blocked worker. Review additionally
closed queue admission on wrapper error exits before final verification.

Focused sessions 59166 and corrected-source 77915 exited zero with asserted
MemoryMax=1G and zero swap. Final standalone-owner-check-v2.log records stable
and MSRV Clippy, executor/lifecycle/session/transport/CLI/public-surface suites
and 59 CLI library tests. New actual tests cover blocked-output saturation and
bounded close with loss counting, malformed/oversized diagnostic refusal, and
an explicit stop after a second owner observation during a two-second ordinary
interval while an independent writer remains held. Native inventory is empty.

The API is opt-in: production command routing/endpoint publication, installed
nonempty native ownership, full changed-source gates and sensitivity checks
remain required. Progress remains 3/7 with task statuses unchanged.
