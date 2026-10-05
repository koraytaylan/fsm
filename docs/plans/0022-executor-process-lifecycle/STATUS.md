# Plan 0022 — Executor Process Lifecycle — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and the integration coordinator owns
lifecycle updates.

- **Status:** Registered by hand; native prerequisite complete;
  production lifecycle implementation has not started.
- **Goal:** prevent a successor from overlapping a surviving local handler
  tree, with bounded shutdown and evidence-based restart for both process and
  MCP handlers.
- **Root cause:** direct-child handles and short-lived writer locks do not
  contain descendants or persist execution ownership across executor death.
- **Approach:** resolve the native containment prerequisite, journal claims
  before launch, prove tree closure before reuse, and drive every execution
  host through the same shutdown and recovery protocol.
- **Progress:** 1/7 tasks done; 0 blocked; 0 dropped; durable claims Ready.
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

_Task frontmatter remains authoritative; registration does not release the native gate._
