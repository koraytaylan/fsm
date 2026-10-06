---
id: executor-ownership-integration
title: "Executor Ownership Integration"
workstream: "0094"
kind: task
depends_on:
  - contained-handler-runner
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/run/pipeline.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/execution_ownership.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/tests/executor_ownership.rs
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---

### Exact-source shared recovery verification (2026-10-06)

Run `37418107228` at clean source `eb783b273fc7adec698b3f4a8ca04822488b5dd4` has a successful native MSRV job `112121154038`; its downloaded artifact independently passes `verify_native_evidence.py` with `rustc 1.89.0 (29483883e 2025-08-04)` across all 81 cases, including the expanded `provisioned_broker_access` shared-tick recovery control. Evidence and verification output are retained in the dedicated task cache under `ci-37418107228/msrv` and `msrv-verification.log`. This proves recovered completion application through public ticks with independent writer contention, read-only refusal, an absent current handler catalogue, disabled-event parking and subsequent resume at this source; it does not prove new-run admission or restart between acknowledgement and event application.

The stable native job `112121154143` failed before reaching that shared-tick control, in `private_exec_status` for `/bin/sh` with MCP enabled: `closure native retirement deadline (manager_retired=false)`. Its retained `ci-37418107228/stable/authority-private_exec_status.log` identifies the same retirement phase seen in earlier run `37401653979`; a later successful matrix did not establish a fix. Review of `stop.rs`, `manager.rs` and `closure.rs` confirms retirement still requires an unloaded original unit and no queued job, and failed-unit reset still requires original invocation/PID matching. The available failure does not identify the unit state, queued jobs, or a stop refusal, so it does not justify weakening those guards or extending deadlines. This matrix is not accepted, stable shared-control execution is unproven, and all production/gate/executable-verification flags remain false; six portable jobs were still live when this verdict was recorded.

# Executor Ownership Integration

At clean frozen source `e8df2cfdc1a6acb3503f50be2e7c1b4f67074cc3`, capped sensitivity session `17955` completed with exit 0: all three ownership controls pass, ignoring only the `start_requested` guard makes both named startup/cancellation controls fail with test exit 101, source bytes are restored in a finally block, all three pass again, and clean checkout status is verified. `local-native-installed-owner-capped-sensitivity.log` retains actual 1 GiB RAM/zero-swap scope limits and each outcome. This proves those public startup guards are load-bearing without claiming provisioned bound-entry execution or completed fresh admission.

Earlier CI `37418107228` at exact source `eb783b273fc7adec698b3f4a8ca04822488b5dd4` is now terminal failure: all six portable Linux/macOS/Windows stable/MSRV gates, zero-dependencies and native MSRV succeeded, while native stable failed at the retained manager-retirement deadline before its shared recovery control. Final metadata `ci-37418107228/final-run.json`, all six nonempty portable logs and full failed native-stable job log are retained alongside the independently verified MSRV native artifact. This verdict does not validate later diagnostic, installed-owner or bound-entry changes, and does not establish a fix for the recurring retirement failure.

Bound-entry source `265b606` passed Rust 1.89.0 executor all-target Clippy in session `66655` (exit 0), with `local-native-bound-entry-capped-msrv-clippy.log`. The first focused ownership unit run failed two new assertions because `Store::open_memory` contains the required genesis record; the controls now compare the original complete records/state/head rather than assuming an empty journal, and all three ownership tests pass in corrected session `91747` (exit 0), log `local-native-installed-owner-capped-msrv-unit-corrected.log`, retaining the failed log separately. Provisioned bound-entry execution, guard sensitivity and full changed-source host acceptance remain pending.

Validation resumed using temporary user scopes whose actual `memory.max=1073741824` and `memory.swap.max=0` were read and asserted before Cargo started, with one build worker/test thread and no concurrent intensive local job. Bounded `POSIX_FADV_DONTNEED` advice on regular task-cache artifacts reduced shared cgroup usage by about 3.6 GiB without deleting files or changing their sizes/modification times; a second bounded batch produced no meaningful additional reduction. This is clean file-cache eviction, not proof that swap pages were freed or that earlier freezes were caused by this task. Both successful validation logs retain their actual temporary scope and verified limits; existing cgroup limits and swap configuration were unchanged.

### Bound-to-execution writer review

Source review found that the original `NativeRun::advance` transitions from `Bound` to execution inside ordinary observation, which would let an installed fresh owner request execution without a new writer-held eligibility check after asynchronous binding. Installed-owner startup now marks its transport for explicit entry: observation remains at `Bound`, and new `NativeExecution::launch_bound` uses the same extracted healthy-writer/current-claim/hash/admission/running-pending/stopped checks as initial startup before requesting execution of that exact bound original transport. Read-only refusal leaves binding intact, while a requested execution or execution-helper failure cannot be retried; ownership and capacity remain retained. The preexisting constructor keeps its separate primitive behavior, and recovery cannot use this explicit entry path.

The actual non-timeout provisioned owner control now waits for binding under an independent writer lease, observes repeatedly without entry, checks read-only dispatch refusal plus unchanged records/state and exact absence of original launch/entry/handoff files, releases that writer for one healthy dispatch, checks duplicate-request refusal, then reacquires independent writer contention for the existing completion/settlement proof. The timeout axis retains original constructor coverage. Formatting, source-size and diff checks pass; compilation, sensitivity, full changed-source gates and provisioned execution are unexecuted while the shared user cgroup remains near its pressure threshold. This is a required fresh-admission building block, not completed shared-tick preparation, admission, sealed post-ack recovery, shutdown or reconciliation, and no production gate is released.

Review of installed-owner product `18d5fef` found the expanded positive probe had replaced the existing `NativeExecution::start` constructor's only provisioned positive invocation; the timeout axis now retains that constructor while the non-timeout axis exercises `start_retained`, and both require duplicate-start refusal. Changed-source Rust 1.89.0 executor all-target Clippy passed in session `37706` (exit 0), with log `local-native-installed-owner-msrv-clippy.log`; formatting, source-size and diff checks pass. The earlier frozen diagnostic release gate at `e6e9263a81f97d490e743ebc06291b133f41fc36` completed in session `10512` with exit 0 and full retained log `local-e6e9263-stable-release.log`, but does not validate these later startup changes. Unit execution/sensitivity, full changed-source host gates and provisioned startup controls remain pending.

Memory review after the release gate observed 65–66 GiB host RAM available but 5.6 GiB swap occupied; samples showed no current swap I/O. The actual user cgroup has `memory.high=85899345920` (80 GiB), `memory.max=103079215104` (96 GiB), and current usage about 79.4 GiB, with historical high-pressure and OOM-kill counters; this establishes a reclaim limit independent of host-wide available RAM, not attribution of previous freezes to this task. Further intensive gates are deferred until that shared cgroup has adequate headroom, while bounded source review and edits may continue; no cgroup limit, swap configuration, unrelated process or user file was changed.

Fresh-owner startup now has the provisional `NativeExecution::start_retained` method: a host can insert `retain_uncertain(actual_published_claim)` before requesting transport, and the same object retains its identity and capacity on any startup refusal. The startup opportunity is consumed before writer validation, cancellation before startup also consumes it, and recovery/completed or previously requested objects refuse another start. Review caught and closed the cancellation-before-startup gap before committing. Two portable Linux unit controls cover refusal followed by another startup request and cancellation before entry; the actual provisioned positive owner probe now covers read-only refusal followed by healthy-writer retry refusal, installed-owner success and duplicate-start refusal while preserving original records/state/hash. Formatting, file-size and diff checks pass; compilation, unit sensitivity, full changed-source gates and provisioned execution are queued behind frozen diagnostic release session `10512`. No test success or completed shared admission is inferred from these unexecuted controls, and no journal/hash format changes occur.

The diagnostic product `e6e9263a81f97d490e743ebc06291b133f41fc36` passed full stable debug workspace testing in original session `22388` (exit 0); `local-native-stop-diagnostic-stable-debug.log` retains the complete output, and tracked code was checked unchanged against that product. After a healthy memory check (59 GiB available, 2.1 GiB swap occupied), its release gate started serially in clean isolated checkout `native-stop-review-e6e9263`, session `10512`, with one worker/test thread and log `local-e6e9263-stable-release.log`; release and remaining host gates are pending.

An independent refactor moves the existing shared-tick `Runner` recovery/readiness/application methods into `run/native_host.rs`, retaining identical platform branches, method visibility and operations, so the upcoming admission implementation does not overrun the 1000-line Rust file ceiling. This writes no new persistent bytes, changes no public API or native proof condition, and does not implement fresh admission. Formatting and source-size checks pass; changed-source compilation and regression checks are queued behind isolated release session `10512`, whose result cannot validate this later refactor.

### Preparation-to-claim composition review

Source review at `e6e9263` identified an admission boundary that must be resolved before replacing shared-tick launches: `Scheduler::on_observation` inserts `Inflight` and its deadline when emitting `Directive::Start`, before Root preparation or a durable claim exists. A later `Kill` therefore cannot use the legacy `runner.kill`/`PendingSettle` path for a preparation reservation: no original execution claim or authenticated stopped result exists, and helper retirement alone does not establish the prepared domain's absence. Native admission must retain the original effect and checked handler contract separately through preparing, delivered-but-unclaimed, cleanup, and uncertain states; known delivered unclaimed domains require `NativePreparedCleanup`, while an unknown allocation cannot be released merely because its helper reaped. These states must occupy scheduler capacity and continue bounded observation without writer access.

After a domain is delivered, the writer phase must resolve current journal eligibility and publish the original claim before helper entry; it must install both local original ownership and scheduler claim identity before attempting `NativeExecution::start`, retaining that claim on every startup refusal. The existing writer-opening failure loop clears legacy `PendingSettle` reservations and must not clear native preparation or claimed ownership. Successful durable native settlement must explicitly retire the corresponding scheduler reservation, rather than relying on owner-map removal alone: otherwise its stale `Inflight` entry continues charging capacity or issues a later kill. New production routing must cover these boundaries through both public tick entry points, with independent writer contention, cancellation before claim, losing admission to another writer, startup refusal after claim, and subsequent capacity reuse; the current recovered-owner probe supplies none of these fresh-admission proofs.

Follow-up review found `runner::execute_cancellable` discards the result of `stop::fence` before invoking independent closure verification, hiding whether the recurring retirement timeout follows a matched-stop refusal. The runner now includes that original stop refusal (or successful stop classification) only when closure itself fails; it still always attempts closure, and neither a stop success nor a refusal changes receipt eligibility, deadlines, revocation, or ownership retention. Rust 1.89.0 executor all-target Clippy passed in session `16697`, with log `local-native-stop-diagnostic-msrv-clippy.log`; formatting, file-size and diff checks passed. Full changed-source host gates and provisioned native execution remain pending, and this diagnostic is not a fix for the retirement failure.

Every execution host must claim before launch and replay a durable stopped
result before considering another run.

**Steps:**

1. Split `service::prepare` so stopping and observing active runs stay
   independent of writer access, while every new launch follows a durable
   claim and a writer-protected recheck of effect eligibility.
2. Thread run identity and immutable handler fingerprint through scheduler,
   watcher, runner and pipeline; reject stale results and never reinterpret
   recovered results with a changed handler table.
3. Persist verified stopped results before attempt/ack/event settlement and
   recover that settlement without rerunning the handler; keep existing
   acknowledgement-before-event ordering and derived request keys unchanged,
   consuming each stopped result atomically with its retry/ack disposition
   while preserving claim exclusion until that transaction is durable.
4. Route standalone execution, embedded service ticks, public tick helpers and
   startup recovery through the same sequence, reconciling claims before
   allowing any conflicting new run; preserve read-only/degraded non-execution.
5. Keep uncertain runs pending and visible, with bounded health summaries and
   no argv/secret disclosure; update API inventory and execution guarantees.

**Tests:**

- Race standalone/standalone and standalone/embedded executors against one
  pending effect with an independent external marker proving one live tree.
- Hold the writer while an active handler times out: it stops promptly,
  remains claimed, and settles once the writer is available; no unclaimed
  start occurs during contention.
- Crash after claim, after launch, after verified stop, after stopped record,
  after attempt/ack and before outcome event; assert exactly the specified
  sequential retry or settlement recovery and a valid journal each time.
- Pause after a durable stopped result and race a second executor before
  settlement: no new handler starts; retry resumes only after single-consumption
  disposition and the journaled backoff deadline, including after restart.
- Change/remove handler configuration before restart and inject late results
  from an old incarnation; neither starts conflicting work nor applies the
  wrong contract, including public helper and read-only entry points.
- Existing retry, cancellation, dead-letter, embedded workflow and
  interrupted-outcome suites pass with unchanged domain event semantics.

- **Done when:** production standalone, embedded and public tick paths pass concurrent-executor and launch/settle crash tests proving that a matching durable claim precedes every start and that unresolved or stale ownership cannot authorize a second run.

## Integration review before implementation

Reviewed against product source `31e0c6318cb45bcc5f76468d6586e172356e466a`;
this task remains planned while task 9303's current native stable and full
portable gates are pending.

The existing primitives provide the following handoff, but the service does
not yet compose it:

| Host phase | Existing entry | Integration obligation |
| --- | --- | --- |
| Observe ownership | `store.state.execution.unresolved()` | Project claims and stopped results from the same read-only prefix as pending effects, including claims for removed effects; do not filter them through the current handler table. |
| Prepare empty domain | `NativePreparation::start/poll` | Obtain the provisioned original namespace and generation, retain the helper between ticks, and distinguish preparation from permission to execute. |
| Claim and admit | `Pipeline::claim_native_handler`, `NativeExecution::start` | Acquire a healthy writer, revalidate eligibility, durably claim the prepared domain, and start binding before releasing that writer; contention must leave user code unstarted. |
| Observe active run | `NativeExecution::observe/cancel/reap` | Drive owned handles before requesting a writer, retaining original run identity and capacity on uncertainty or unavailable settlement. |
| Apply stopped result | `NativeExecution::settle` | Persist authenticated stopped evidence, then atomically consume it under the original retry policy; preserve completion on refusal. |
| Apply outcome event | `Pipeline::advance_native_settled` | Use the retained original completion only after durable acknowledgement, with the existing derived event key. |
| Recover completion | `NativeExecution::recover` | Read original completion without binding or launching; missing completion retains ownership and requires the reconciliation work in task 9403. |

`Observation` currently carries attempts and executor request keys but no
execution claims, and `Scheduler::on_observation` excludes only local
in-flight runs and those keys before producing starts; both need durable
ownership facts before the service can authorize starts safely.
`service::tick_reporting` currently calls `prepare` before opening the writer,
and its contention path clears scheduler entries for immediate settles;
that path must be replaced with retained claimed execution rather than used
for native completions.

The public tick signatures accept a `Runner`, so changing only `service::run`
would leave embedded and downstream callers on the old sequence; the runner
must own the same preparation/execution state used by all tick entries.
Provisioned route discovery and startup admission still need a production
design: neither a caller-supplied unchecked route nor a direct-child fallback
satisfies this task, and native helper retirement alone never permits claim
release.

This review establishes implementation boundaries, not acceptance evidence;
the race, contention, crash, changed-contract and downstream API cases above
remain unexecuted for the integrated host.

### Ownership projection and capacity design

Reinspection at corrected product source `9f1f175` confirms that `Watcher::scan`
initializes attempts and request keys from one read-only Store, while scheduler
capacity counts only its local `inflight` map. Integrate ownership into that same
scan before any pending-effect filtering: retain original `Claim` and optional
`Stopped` for every `execution.unresolved()` entry, including cancelled instances
and removed effects. Current handler lookup must not decide which owners exist.

The scheduler must exclude owned `(instance_id, effect_id)` pairs from starts,
and compute global/per-instance occupied capacity from the union of observed
owners and retained local handles, deduplicating a local claimed handle by its
original run identity. An uncertain local handle remains occupied even when a
later scan cannot establish its matching claim; missing observation is not
permission to discard the handle. Prepared helpers also reserve local capacity
until delivered or actually retired, but preparation grants no durable ownership.
Stopped owners remain occupied until durable consumption, consistent with SPEC's
owned native host clause; a stopped result is recovery work rather than a new
candidate. This avoids both double-counting a local run and making remote or
removed-effect ownership invisible after restart.

Implement the projection and pure scheduler exclusion first with independent
fixtures for remote ownership, local/remote overlap, removed effects, cancelled
instances, stopped ownership and uncertain retained handles; then compose the
writer-held launch path and original-contract recovery through every public tick.
These are implementation decisions, not executed integration evidence, and the
task remains planned behind task 9303's outstanding acceptance.

Task 9303 has now completed its frozen local and nine-job CI acceptance at
`9f1f175`; ownership integration is opened for implementation using the
projection/capacity design above, with all integrated-host cases still pending.

### Ownership projection implementation review

Scoped source review of `9b32cd9..ea9a17c` confirms that watcher ownership is
cloned from the same read-only Store before instance filtering and that the
scheduler checks the full instance/effect pair before current handler lookup.
The 21 existing watcher tests and public inventory gate pass, the independent
cancelled-owner regression passes with a held writer and cold reopen, and all
21 scheduler cases pass including remote ownership and changed handler lookup.
These results prove projection and start exclusion only; stopped-result scan
fixtures and capacity cases remain pending, and full frozen host/CI acceptance
has not yet been rerun for these integration changes.

Review identifies the remaining capacity defect in `Scheduler::has_room_for`
and `Capped.inflight`: both still count only local entries, and the old comment
assumes orphan children disappear after restart. The next implementation must
count observed owners plus retained local handles, deduplicating only a matched
original run identity; a different incarnation or missing current observation
cannot erase occupied capacity. Production launch still uses the legacy path,
so this slice does not accept writer-protected native routing or settlement.
All-targets executor clippy is running serially with retained log
`~/.cache/fsm-plan-native-matrix-20261005/ownership-projection-clippy.log`.

### Capacity implementation review at `311498f`

Capacity now unions observed `(instance, effect, run_id)` owners with retained
local reservations; only `retain_claim` matching the local effect and immutable
original claim supplies a deduplicating run identity. Unbound reservations
remain distinct, and losing observation retains the local reservation. Global
and per-instance admission and capped diagnostics use that same union.
Scheduler and public inventory tests pass, including a remote owner filling a
one-slot host, matched local/observed overlap allowing the remaining slot, and
missing observation preserving both local reservations. The original projection
all-targets clippy completed successfully; capacity clippy is running serially.
Separate stopped-owner, mismatched-incarnation and per-instance fixtures remain
required before integrated acceptance, along with full frozen host and CI gates.

Current production service inspection confirms the next composition boundary:
`tick_reporting` calls `prepare`, including `runner.spawn`, before `Store::open`,
and clears settled scheduler entries on writer refusal. Native composition must
split stop/observe from starts, retain preparation and execution in Runner,
claim/recheck/start binding under the writer, and preserve original completion
and capacity when settlement cannot obtain the writer. Existing public tick
entries must share that sequence; the current legacy service remains unaccepted.

### Automatic route discovery design

The provisioned authority already publishes immutable Root-owned mode-0444
`store-identity.json` containing physical device/inode, while private
`store.json` retains its path; discovery must use the public identity only.
Add a private Linux client discovery module used by Runner for automatic ticks:
scan only fixed `/var/lib/fsm-containment`, canonical 32-hex namespace names
and canonical positive `authority-<generation>` names, charging at most 4096
total entries across both directory levels before retaining another entry.
Require protected non-symlink Root-owned directory identities before and after
scanning, bounded no-follow regular public identity reads and canonical closed
JSON. Match the actual store directory device/inode, never journal bytes or a
caller-supplied authority path. Zero or multiple matching registrations refuse
new execution; offline or damaged matching registration cannot be silently
ignored to select another generation. For the unique match, validate the current
immutable broker route, boot, operator UID, authority inode and socket identity
using the existing client access contract, and repeat identity checks before
preparation. Discovery grants neither claim ownership nor handler entry.

Keep discovery private: public tick signatures remain shared through Runner,
and the authority continues rechecking physical store and original claim at
binding. Unsupported/unprovisioned hosts refuse automatic native execution;
there is no direct-child fallback. Recovery uses each original claim's route,
not discovery or the current handler table. Required discovery fixtures cover
exact/plus-one inventory and file bounds, missing/duplicate/torn/symlink/writable
registrations, wrong physical store, wrong operator/boot/socket and replacement
between observations; native provisioning cases must exercise the production
entry before accepting automatic routing. This design is not implementation
or execution evidence, and task 9401 remains In progress.

Discovery contract review confirms that current transport validates the helper but has no protected discovery reader; the existing store proof reader provides the no-follow/nonblocking, opened/path identity, bounded canonical-read pattern to follow. SPEC now fixes the shared inventory and document budgets before implementation; discovery remains unimplemented. Capacity all-targets clippy passed with exit code zero.

Discovery `5158d81` implements the protected scan and exposes native preparation by physical store; two inventory/name boundary tests pass and initial executor clippy passes. The filtered boundary invocation ran zero public inventory tests; an explicit unfiltered run found fixture ordering stale, corrected separately with all 16 inventory checks then passing. Provisioned protected-file, ambiguity, access and replacement controls remain unexecuted, so discovery and production host routing remain unaccepted.

### Discovery replacement review

Review of `5158d81..b94e1da` found that discovery revalidated protected directory
properties after selecting a physical-store registration, but did not retain
the selected authority and namespace's original inodes across final route
validation. A replacement Root-published directory could therefore be accepted
as the selected original. The correction retains both original directory
metadata values and requires their identities/modes unchanged before and after
final route/registration checks, also repeating the original base identity at
return. This is a scoped race correction to SPEC's repeated-identity requirement,
not a new format; native replacement fault execution remains required.

Provisioned discovery controls now invoke the production unprivileged `for_store` entry for writable/symlinked public identity, writable route, torn JSON, exact 4096-byte JSON rejection, 4097-byte overflow rejection and duplicate physical-store registration; each requires its exact error and unchanged allocation counter. The unique positive preparation also uses discovery. These controls are compiled, not yet native executed, and cannot accept automatic host routing.

Frozen ownership/discovery range `9b32cd9..7dc91d5ac99257d55a3b9bb6bf946d374a72576d` was pushed only to the authorized review branch; CI `37392851160` is queued at that exact source. This run must execute the updated provisioned preparation/refusal controls on stable/MSRV and the portable matrix; earlier `9f1f175` acceptance cannot validate these changes. Production shared tick routing remains incomplete and is outside this frozen slice.

### Shared Runner composition phases

The shared Runner must retain an entry per original effect with the immutable
observed handler contract and pending-effect identity while preparation is
active. A prepared domain is retained between ticks; it grants no entry. The
writer phase re-resolves that effect, verifies the retained contract against the
current eligible handler, claims the original prepared domain, binds scheduler
capacity through `retain_claim`, and constructs `NativeExecution::start` before
releasing the writer. A claim followed by startup refusal remains an original
uncertain owner, not a spawn-failed acknowledgement or a reusable reservation.

Stop/observe runs before any writer acquisition, including on failed scans;
preparation cancellation retires its owned helper without asserting domain
closure. An already delivered but unclaimed domain must retain its original
route for bounded closure work rather than discover a replacement. Once claimed,
only `NativeExecution` original completion/proof and durable `settle` may release
capacity; timeout or helper retirement cannot call scheduler `complete`.
Outcome advance is retained separately after durable acknowledgement, using the
original completion contract rather than the current handler table. Startup
recovery adopts every observed original owner independently of pending-effect
or handler lookup and never substitutes fresh discovery/binding for recovery.

These phases must be used by `tick_with`, `tick_reporting`, standalone execution
and embedded service ticks through one composition path; legacy `prepare` cannot
remain an alternate automatic execution path. Memory/read-only/quarantined or
unsupported hosts cannot start preparation/user code. Native host crash, held
writer and changed-contract controls remain mandatory before task acceptance.
CI `37392851160` currently has zero dependencies passing and the other eight
jobs running; the isolated frozen local debug gate remains active.

### Frozen native discovery execution

CI `37392851160` native stable job `112041769656` and MSRV job `112041769155`
completed successfully at exact `7dc91d5ac99257d55a3b9bb6bf946d374a72576d`.
Downloaded artifacts in `~/.cache/fsm-plan-native-matrix-20261005/ci-37392851160/`
were independently verified against the frozen inventory, report/log integrity,
clean source and compiler identities: 81 cases each at rustc stable
`1.99.0 (b940084d7 2026-09-28)` and MSRV `1.89.0 (29483883e 2025-08-04)`.
The updated provisioned broker/preparation cases include production store
route discovery and exact refusal/no-allocation controls described above.
This inventory count covers the whole native matrix, not 81 discovery cases.
Verification preserves `executable_bytes_verified:false`, `gate_released:false`
and primitive-only scope. Zero dependencies also passes; the six portable
jobs and isolated local debug workspace gate remain active. Native directory
replacement and full inventory-bound production controls are still outstanding,
as are shared production routing, settlement and host crash acceptance.

### Frozen debug gate and stopped-owner capacity fixture

The isolated stable debug workspace gate at exact `7dc91d5` completed with
exit zero; its retained log is
`~/.cache/fsm-plan-native-matrix-20261005/local-7dc91d5-stable-debug.log`.
The same frozen source's serial release workspace gate is now running;
six portable CI jobs remain active while both native jobs and zero dependencies
have passed. These results do not accept the unfinished shared Runner routing.

A separate scheduler regression now supplies a structurally valid stopped
owner with an immutable successful outcome, requires global capacity to remain
occupied, and permits the deferred unrelated start only after a later observed
prefix removes the owner. It asserts scheduling behavior rather than native
proof authentication or persistence. This new fixture is not part of the
frozen `7dc91d5` gates and remains unexecuted until the serial release gate
finishes; the stopped-owner watcher persistence fixture remains outstanding.

### Discovery binding refusal controls

The provisioned discovery fixture additionally replaces the public physical
store inode, then independently changes the operator UID, boot identity and
published socket inode. Each control invokes the production `for_store`
entry as the unprivileged operator, requires the exact discovery-layer refusal
and checks that the durable allocation counter is unchanged before restoring
the original publication. These controls remain unexecuted and are outside
the previously verified `7dc91d5` native matrix. They complement rather than
substitute for the outstanding native directory-replacement and full
production inventory-limit controls, and cannot accept shared host routing.

The discovery fixture now also counts the actual base-plus-namespace directory
entries and adds exclusively created owned namespace siblings to reach exactly
4096 and then 4097 entries. With a deliberately nonmatching physical-store
registration, production discovery must traverse the exact limit and report
missing registration, then refuse the plus-one inventory independently of
directory iteration order; both calls require an unchanged allocation counter.
It removes each owned filler explicitly and restores the original publication.
This load-bearing entry-limit control is written but not yet compiled/native
executed; the helper-only boundary test and older native artifacts do not prove it.

### Stopped-owner watcher persistence control

The provisioned broker settlement fixture now scans with an empty handler
selection while the stopped original owner's writer is held, requiring the
exact claim and stopped outcome from the same prefix with no writer append.
A fresh scan after reopening must project that same retained owner and outcome.
The arrangement uses the fixture's actual native completion proof and durable
stop rather than manufacturing authentication for a portable watcher fixture.
This control is written but not yet compiled or native executed; prior native
artifacts cannot establish its new watcher assertions. Shared automatic host
routing remains incomplete and task 9401 remains in progress.

### Regression slice review: `6328198..b177c37`

Reviewed the stopped scheduler ownership, discovery bindings/inventory and
native stopped watcher assertions against their production entry points.
The inventory arrangement computes the same shared base-plus-namespace entry
accounting as discovery, creates each filler exclusively, and makes the target
registration nonmatching so the exact-limit call proves complete traversal
without asking the allocator for a domain. The plus-one call requires the
inventory-specific refusal regardless of enumeration order; neutralizing that
guard would instead yield missing registration and fail the assertion.
Binding faults restore original immutable publication bytes between controls
and each verifies unchanged allocation state. The watcher assertions occur
after authenticated durable stop, before settlement, under the live writer
and then a fresh read-only open; they do not construct a fake native proof.
The portable scheduler fixture only asserts that stopped ownership occupies
capacity and does not claim authentication or writer persistence.

No additional defect was identified in this scoped review. Stable formatting,
the repository file-size check and the complete committed diff check each
returned exit zero independently at `b177c37`. Compilation, scheduler test
execution and provisioned native execution of this regression slice remain
pending while frozen `7dc91d5` release session `74445` is confirmed live;
that earlier source's six portable CI jobs also remain live. The frozen
debug success and earlier native artifacts cannot validate the newer tests.
This review does not accept shared Runner routing, startup recovery, shutdown,
reconciliation or any unfinished requirement of plans 20–23.

### MSRV review finding and correction

Frozen CI `37392851160` Ubuntu stable passed; Ubuntu MSRV job `112041769543`
passed debug and release workspace tests but failed all-target Clippy at
`sched.rs:245` for `nonminimal_bool` in `retain_claim`. The directly downloaded
completed job log is retained at
`~/.cache/fsm-plan-native-matrix-20261005/ci-37392851160/ubuntu-msrv-job.log`.
The correction uses the equivalent `Option::is_none_or` predicate to reject
missing or mismatching local reservations without suppressing the lint or
changing ownership semantics. This invalidates acceptance of the affected
scheduler source until both compiler gates are rerun; no complete portable
matrix success is claimed. The isolated older-source release gate remains live.

### Frozen release completion and correction verification

The isolated stable release workspace gate at exact `7dc91d5` completed
with exit zero; its full retained log is
`~/.cache/fsm-plan-native-matrix-20261005/local-7dc91d5-stable-release.log`.
This closes that earlier source's local release wait, not the newer regression
slice's gates or unfinished production routing. With available RAM approximately
64 GiB and swap approximately 2.1 GiB of 15 GiB, the next serial check is
MSRV executor all-target Clippy against current product source `f9e02fa`;
its log is `~/.cache/fsm-plan-native-matrix-20261005/current-f9e02fa-msrv-execute-clippy.log`.
The full local host gate and updated native/portable matrix remain required.

MSRV executor all-target Clippy at current product source `f9e02fa` completed
with exit zero, compiling the added native discovery and watcher fixtures
without executing their ignored provisioned controls. The targeted MSRV
scheduler suite also completed with exit zero: all 24 tests passed, including
stopped-owner capacity retention. Its retained log is
`~/.cache/fsm-plan-native-matrix-20261005/current-f9e02fa-msrv-sched.log`.
Stable workspace all-target Clippy is the next serial gate, with log
`~/.cache/fsm-plan-native-matrix-20261005/current-f9e02fa-stable-workspace-clippy.log`.
The older CI run still has four macOS/Windows jobs live; its known Ubuntu
MSRV failure is preserved rather than replaced with these narrower local passes.

Stable workspace all-target Clippy at current product source `f9e02fa` also
completed with exit zero. The serial documentation gate is now running with
warnings denied, logging to
`~/.cache/fsm-plan-native-matrix-20261005/current-f9e02fa-stable-doc.log`.
Neither check executes the new provisioned native assertions or replaces the
current-source full debug/release and platform gates.

The current product source `f9e02fa` stable documentation gate completed with
exit zero and `RUSTDOCFLAGS=-D warnings`. Zero-dependency and embed-acceptance
checks are now running sequentially, retaining separate
`current-f9e02fa-stable-zero-deps.log` and `current-f9e02fa-stable-embed.log`
files in the dedicated cache. The new native controls still have compilation
evidence only, and current-source full workspace and updated platform/native
matrix obligations remain unexecuted.

### Current acceptance checks and corrected-source workspace gate

Current product source `f9e02fa` zero-dependency and embed-acceptance commands
both completed with exit zero; embed acceptance includes 11 completeness
tests and one end-to-end test. Frozen CI `37392851160` macOS stable passed;
macOS MSRV job `112041769370` failed on the same `sched.rs:245`
`nonminimal_bool` already corrected in `f9e02fa`, as independently inspected
in the retained `ci-37392851160/macos-msrv-job.log`. Both Windows jobs remain
live, and the older matrix is not green.

A detached corrected-source checkout at
`~/.cache/fsm-plan-native-matrix-20261005/ownership-review-f9e02fa`
now runs the full serial stable debug workspace gate, with retained log
`local-f9e02fa-stable-debug.log`. Earlier `7dc91d5` debug/release passes do
not validate the newer scheduler correction or regression slice. Updated
native execution, corrected-source release and full matrix acceptance remain
outstanding; task 9401 remains in progress.

The isolated full stable debug workspace gate at corrected product source
`f9e02fa` completed with exit zero, with complete log
`~/.cache/fsm-plan-native-matrix-20261005/local-f9e02fa-stable-debug.log`.
The same isolated source now runs the serial full release workspace gate,
logging to `local-f9e02fa-stable-release.log`; no second local build overlaps it.
The debug pass includes the new portable stopped-owner scheduler fixture but
does not execute ignored provisioned native discovery/watcher controls.
Both Windows jobs on the older `7dc91d5` CI run remain live, and its diagnosed
MSRV lint failures prevent a green-matrix claim.

The isolated full stable release workspace gate at corrected product source
`f9e02fa` completed with exit zero, confirmed through its original session
`17099` and retained `local-f9e02fa-stable-release.log`; the log ends with
successful workspace doc tests. Both current-source full workspace modes now
have successful local execution evidence. The existing CI run `37392851160`
still has both Windows jobs in progress, and its two diagnosed MSRV lint
failures remain preserved. Updated native execution and the corrected-source
platform matrix remain outstanding, alongside production Runner integration;
this local release pass does not complete task 9401.

### Post-claim startup failure ownership state

`NativeExecution::retain_uncertain` now preserves an original durable claim when
helper startup fails, with no helper, completion or inferred authentication.
Progress explicitly reports `Uncertain`; cancellation and successful helper
retirement cannot convert that state to closure or release retained capacity.
The structural regression `missing_transport_cannot_become_verified_completion`
passed on MSRV 1.89, and MSRV executor all-target Clippy passed with warnings
denied. Specification, API policy, embedding and release documentation move
with this additive primitive. This is not integrated production Runner routing;
full frozen host/platform/native gates remain required for the changed source.

At product source `41cfaa1`, the full MSRV executor library suite passed:
22 tests, including the new no-transport uncertainty regression and bounded
capture, worker retirement, discovery and completion-mapping controls.
This remains narrower than full workspace/platform/native acceptance.
The frozen older CI Windows MSRV job `112041769526` is now terminal failure;
its retained `ci-37392851160/windows-msrv-job.log` independently confirms the
same `sched.rs:245` nonminimal boolean lint corrected in `f9e02fa`, rather than
a new portability defect. Windows stable job `112041769321` remains live in
its full release workspace test step after its debug workspace pass; the older
matrix remains non-green and is not replaced or cancelled.

### Uncertainty-state review and frozen debug gate

Review of committed `34c7ed3` against product `41cfaa1` confirms that the new
constructor starts no helper, borrows no writer, supplies no proof, and retains
capacity; existing `settle` still requires verified original completion before
stop/settlement. The regression also checks that cancellation and successful
transport retirement cannot promote the missing transport to `Closed`.
No additional defect was identified in this scoped review; it does not cover
unfinished Runner composition. All 22 stable executor library tests passed.
The existing detached review checkout, whose directory name still contains
`f9e02fa`, has been moved to exact product `41cfaa1` for the serial full stable
debug workspace gate. Its original live session is `95061` and its retained log
is `local-41cfaa1-stable-debug.log`; no pass is asserted before completion.
Windows stable CI job `112041769321` remains live in its release workspace step.

### Older frozen matrix terminal result

CI run `37392851160` at exact source
`7dc91d5ac99257d55a3b9bb6bf946d374a72576d` is now completed with conclusion
failure. Windows stable job `112041769321` passed its full debug/release,
all-target lint, documentation and decimal-regeneration steps; its retained
log is `ci-37392851160/windows-stable-job.log`. Final run metadata is retained
as `ci-37392851160/final-run.json`. Both native legs and zero-dependencies,
Ubuntu stable, macOS stable and Windows stable passed; all three portable MSRV
legs failed on the independently inspected scheduler boolean lint corrected in
`f9e02fa`. The failed matrix is preserved, not described as green, and a new
current-source matrix remains required. Current-source isolated debug session
`95061` remains live; its result is not yet accepted.

The isolated stable debug workspace gate at `41cfaa1` ended with exit 101:
only `fsm-execute --test public_surface` failed, because the additive
`NativeExecution::retain_uncertain` method was missing from the committed
inventory. The full failed log remains `local-41cfaa1-stable-debug.log`.
The inventory correction adds exactly that method; the non-regenerating stable
surface suite passed all 16 tests afterward. Regeneration initially ran a
shared-cache executable retaining the detached checkout's manifest path; its
one-line generated fixture was copied to the active worktree, and that detached
checkout was restored to its frozen source. The corrected committed source
still requires the full frozen workspace and matrix gates; the narrow surface
pass does not replace the failed workspace result.

### Corrected frozen gate and updated matrix

The clean detached checkout now freezes exact corrected product source
`a876df1df10ae977a2e45a5393e590c175652240`. Its full serial stable debug
workspace gate is live in session `34597`, retaining
`local-a876df1-stable-debug.log`; the prior failed `41cfaa1` log remains intact.
Formatting, oversized-file and complete `7dc91d5..a876df1` diff checks passed.
After preserving the terminal older matrix, the authorized review branch alone
was advanced to `a876df1`; CI run `37397745043` is queued at that exact source.
It must execute the newer native discovery inventory/binding and stopped-owner
watcher controls, as well as corrected portable gates, before acceptance.
No production-routing, native-execution or green-matrix claim is made now.

The updated CI stable native job completed successfully at exact corrected
source `a876df1df10ae977a2e45a5393e590c175652240`. Its full artifact is retained
under `ci-37397745043/stable`; the independent `verify_native_evidence.py`
command completed with exit zero and verified all 81 matrix cases against
`rustc 1.99.0 (b940084d7 2026-09-28)`, clean source identity and retained
per-case evidence. This is provisioned native execution for the current
primitive/regression source, not automatic production Runner acceptance:
`executable_bytes_verified` and `gate_released` remain false. The MSRV native
and six portable jobs remain live; the full matrix is not yet green.

The updated MSRV native job also completed successfully at exact corrected
source `a876df1df10ae977a2e45a5393e590c175652240`. Its retained artifact is
`ci-37397745043/msrv`; independent verification completed with exit zero and
verified all 81 cases against `rustc 1.89.0 (29483883e 2025-08-04)` and clean
source identity. Both current-source native compiler axes now have independently
verified provisioned evidence. Production routing and executable-byte/gate
release flags remain incomplete/false; the six portable matrix jobs and isolated
full debug workspace session `34597` remain live.

The isolated full stable debug workspace gate at corrected source `a876df1`
completed with exit zero through original session `34597`; its full retained
log `local-a876df1-stable-debug.log` ends with successful workspace doc tests.
This includes the corrected public surface inventory and the current portable
ownership regressions. With 64 GiB available RAM and 2.1 GiB of 15 GiB swap
occupied, the same frozen checkout now runs the serial full stable release
workspace gate in session `49657`, retaining `local-a876df1-stable-release.log`.
No overlapping local build was started; six portable CI legs remain live,
and production Runner integration remains incomplete.

The isolated full stable release workspace gate at corrected source `a876df1`
completed with exit zero through original session `49657`; its retained log
`local-a876df1-stable-release.log` ends with successful workspace doc tests.
Both full workspace modes now pass locally at this source. The serial full
stable workspace all-target Clippy gate is live in session `62881`, retaining
`local-a876df1-stable-clippy.log`; documentation, explicit zero-dependency and
embed acceptance checks at the same frozen source remain required afterward.
The six portable CI jobs remain live, and production Runner integration is
still unfinished; these gate passes do not complete task 9401.

Frozen-source `a876df1` full stable workspace all-target Clippy completed with
exit zero through session `62881`, retaining `local-a876df1-stable-clippy.log`.
The serial documentation gate is now live in session `56113`, with warnings
denied and retained `local-a876df1-stable-doc.log`. Updated CI Ubuntu stable
also completed successfully; the other five portable legs remain live.
These results leave production Runner composition, the remaining host gates
and full portable matrix acceptance outstanding.

Frozen corrected source `a876df1` documentation completed with exit zero and
warnings denied, retaining `local-a876df1-stable-doc.log`. The explicit serial
zero-dependency and embed-acceptance checks also passed, as independently read
from `local-a876df1-stable-zero-deps.log` (one test) and
`local-a876df1-stable-embed.log` (11 completeness tests plus one external
execution-ownership test). Together with formatting, size, range, debug,
release and all-target lint checks, this completes the required local stable
host gate for this frozen primitive/regression source. Five portable CI legs
remain live; production Runner routing and task 9401 remain incomplete.

Updated CI Ubuntu MSRV job `112057639718` completed successfully at frozen
source `a876df1`, with its full log retained as
`ci-37397745043/ubuntu-msrv-job.log`. Both Ubuntu compiler axes now pass the
portable gate, including the previously failing scheduler lint correction;
this does not replace the preserved older failed matrix. Both macOS jobs
remain live in release workspace testing, and both Windows jobs remain live.
Task 9401 remains in progress because production Runner routing is unfinished.

Updated CI macOS stable job `112057639819` completed successfully at frozen
source `a876df1`, with its full log retained as
`ci-37397745043/macos-stable-job.log`. Six of the nine matrix jobs now pass;
macOS MSRV and both Windows jobs remain authoritatively `in_progress` on the
original run `37397745043`, which has not been restarted. The current-source
local and provisioned native evidence still proves the primitive/regression
slice only; production Runner composition remains unfinished and task 9401
stays in progress.

Updated CI macOS MSRV job `112057639818` also completed successfully at frozen
source `a876df1`; its complete log is retained as
`ci-37397745043/macos-msrv-job.log`. Seven of nine matrix jobs now pass, with
both Windows jobs still confirmed live on the original run. Review of the
preparation lifecycle confirms that delivered domains grant no admission and
helper cancellation/reaping grants no domain closure; the remaining production
host must retain an original-route prepared-domain cleanup operation before
releasing its local reservation. No automatic Runner routing has been added,
so task 9401 and the overall plans remain incomplete.

Production cleanup review found that Root `close` previously always read a binding, so a delivered unclaimed allocation could not be retired through the planned host path; the unbound branch now refuses binding/submission and pending material, retains the authority lock through revocation and native retirement, and publishes only a domain tombstone with no execution receipt, with an expanded `empty_domain_preparation` privileged control for refusal, retirement and cold replay, whose provisioned execution remains required before acceptance.

The cleanup slice passes formatting and diff checks, all 22 portable MSRV
executor library tests, and MSRV all-target executor Clippy; retained logs are
`local-prepared-cleanup-msrv.log` and `local-prepared-cleanup-msrv-clippy.log`.
These checks compile the expanded privileged control but do not execute it;
current CI run `37397745043` predates this change and cannot accept this slice.
The review checked authority-lock retention, durable revocation before removal,
pending binding/submission refusal and the absence of execution receipt output.
Full frozen host gates and provisioned native execution remain outstanding.

Follow-up cleanup review adds direct privileged controls for pending exec-status
evidence and an actual child cgroup: unbound close must refuse both, preserve
the child identity and publish no tombstone, then succeed only after removal
of the test-owned obstacle. The frozen stable debug workspace gate for
`b5ff5f6` remains live in original session `64176`; these additional controls
require compilation and provisioned execution at their own committed source.

Follow-up host cleanup review identified that a numeric `close` request alone cannot convey the retained original domain identity; private broker action `discard-prepared` now validates and matches the full domain under the authority lock before revocation and echoes it after prepared-only retirement, with a privileged wrong-cgroup-identity refusal control and cold original-domain replay controls, while typed host cleanup and production Runner routing remain outstanding.

The isolated full stable debug workspace gate at frozen `b5ff5f6` completed
with exit zero through original session `64176`; its retained log
`local-b5ff5f6-stable-debug.log` ends with successful workspace doc tests.
This accepts that debug gate only and predates the later refusal controls and
original-domain protocol. The clean frozen checkout now checks exact source
`34d00da` with serial MSRV executor library tests followed by all-target Clippy
in session `85425`, retaining `local-34d00da-msrv-lib.log` and
`local-34d00da-msrv-clippy.log`; no overlapping local build was started.
Both original Windows CI jobs remain live, with MSRV advanced to Clippy;
their source `a876df1` predates all prepared cleanup changes.

Exact frozen cleanup source `34d00da` passed all 22 MSRV executor library tests
and all-target executor Clippy with exit zero through session `85425`, as
confirmed from both retained logs. This compiles the new original-domain action
and expanded privileged refusal controls without claiming their execution.
The serial full stable debug workspace gate now runs at that same clean source
in session `68063`, retaining `local-34d00da-stable-debug.log`.
Original-source Windows MSRV CI job `112057639687` also completed successfully,
with its full log retained as `ci-37397745043/windows-msrv-job.log`; eight of
nine original matrix jobs now pass and Windows stable remains live in release
testing. Current cleanup provisioned validation and production host integration
remain outstanding, and task 9401 stays in progress.

NativePreparedCleanup now owns original-route cleanup and retains its complete original domain, confirming only matched successful responses after actual helper retirement and EOF; cancellation and errors remain uncertain, public inventory and capability documentation move with the API, and compilation, response-control execution and provisioned host acceptance remain pending while frozen debug session `68063` continues at the prior source.

The full isolated stable debug workspace gate for exact source `34d00da`
completed with exit zero through session `68063`; the retained
`local-34d00da-stable-debug.log` ends with successful workspace doc tests.
That source includes the Root original-domain cleanup protocol but predates
the owned handle. The clean checkout now validates exact `cd70627` with serial
MSRV executor library tests and all-target Clippy in session `35214`, retaining
`local-cd70627-msrv-lib.log` and `local-cd70627-msrv-clippy.log`; explicit public
surface inventory validation remains required afterward. The original Windows
stable job is still live, and current native cleanup acceptance and automatic
production Runner routing remain unfinished.

Frozen owned-cleanup source `cd70627` passed all 23 MSRV executor library
tests and all-target executor Clippy through original session `35214`.
The explicit public surface suite also passed all 16 tests through session
`60573`, confirming the committed API inventory without regeneration;
its retained log is `local-cd70627-msrv-surface.log`. These checks compile
the privileged controls but do not execute them, and neither these checks nor
the older source CI run complete production host integration or task 9401.

Original matrix `37397745043` completed successfully at exact source `a876df1`:
all nine jobs now pass, including Windows stable `112057639632`, whose full
log and final run metadata are retained under `ci-37397745043`. Its native
artifacts were already independently verified, but its source predates prepared
cleanup and cannot accept that change. The authorized review branch now points
to `e501679509e361cada521b59d5fbbb1c40e121fc`, with new CI run `37401653979`
confirmed queued for the current cleanup source. The serial isolated full
stable debug workspace gate at product source `cd70627` is live in session
`30347`, retaining `local-cd70627-stable-debug.log`. Production Runner routing
and current native acceptance remain unfinished; task 9401 stays in progress.

The full serial stable release workspace suite at frozen product source
`1dddd15` completed with exit zero in original session `80644`, with its full
log retained as `local-1dddd15-stable-release.log`; this predates the physical
replacement control and does not verify that test or resolve the preserved
native MSRV failure. The serial stable workspace all-target Clippy gate at
frozen `7bdfcb6` completed with exit zero in session `74551`, retaining
`local-7bdfcb6-stable-clippy.log`; this compiles the replacement control but
does not execute its privileged assertions. The next serial stable workspace
documentation gate completed with warnings denied and exit zero in session
`25649`, retaining `local-7bdfcb6-stable-doc.log`. Serial executor all-target
MSRV Clippy completed with exit zero in session `26164`, retaining
`local-7bdfcb6-msrv-clippy.log`; neither compilation gate proves privileged
native execution. Serial stable zero-dependency and embedding acceptance
gates completed with exit zero at the same source: one zero-dependency case,
eleven completeness cases and one external execution-ownership case, retaining
`local-7bdfcb6-stable-zero-deps.log` and `local-7bdfcb6-stable-embed.log`.
The full stable debug workspace gate at frozen `7bdfcb6` completed with
exit zero in original session `48863`, retaining
`local-7bdfcb6-stable-debug.log`. Its full serial release workspace gate
completed with exit zero in original session `61836`, retaining
`local-7bdfcb6-stable-release.log`; stable formatting, oversized-file and
committed-range diff checks also pass. All required stable host gates now
pass at this product source, but privileged assertions remain unexecuted by
these portable gates, and the original CI run still has live Windows jobs.
Original cleanup CI Ubuntu stable job `112069961690` completed successfully
at `e501679`; macOS MSRV job `112069961689` also completed successfully,
with its complete log retained as `ci-37401653979/macos-msrv-job.log`;
Ubuntu MSRV job `112069961706` subsequently completed successfully, with
its complete log retained as `ci-37401653979/ubuntu-msrv-job.log`;
macOS stable job `112069961623` also completed successfully, with its
complete log retained as `ci-37401653979/macos-stable-job.log`;
Windows stable job `112069961603` subsequently completed successfully,
with its complete log retained as `ci-37401653979/windows-stable-job.log`;
Windows MSRV job `112069961710` completed successfully, with its complete
log retained as `ci-37401653979/windows-msrv-job.log` and terminal run metadata
as `ci-37401653979/final-run.json`. Original run `37401653979` is now terminal:
all six portable jobs, zero-dependency and stable native pass, but native
MSRV failed as preserved below, so the overall matrix is a failure. That older source cannot
execute the physical replacement control or the later retirement diagnostic.

After preserving the terminal original run, the authorized review branch was
pushed to `482da8f07a1248f3e3a62cd0b62173e22433761b`, with new CI run
`37405315461` now confirmed live across all nine jobs. Native MSRV job
`112081460465` and stable job `112081460634` execute the physical replacement
control and retain the manager-retirement diagnostic if closure fails; no
result or causal explanation for the earlier failure is assumed. The source
contains product `7bdfcb6`, whose stable host gates pass as recorded above;
actual production Runner routing and task 9401 remain incomplete.

Current native MSRV job `112081460465` completed successfully at
`482da8f07a1248f3e3a62cd0b62173e22433761b`; its full artifact is retained as
`ci-37405315461/msrv`. Independent evidence verification exited zero for all
81 total cases, clean exact source and `rustc 1.89.0 (29483883e 2025-08-04)`;
this executes the real physical-store replacement refusal control as part of
`provisioned_broker_access`. Gate release and executable-byte verification
remain false. The same matrix passes the previously failing `private_exec_status`
case, but this does not establish the cause of the preserved prior retirement
deadline failure or constitute automatic production Runner acceptance; stable
native and all six portable jobs remained live at that observation, and task
9401 remains in progress.

Current stable native job `112081460634` subsequently completed successfully
at the same exact `482da8f` source; its full artifact is retained as
`ci-37405315461/stable`. Independent evidence verification exited zero for
all 81 total cases with clean source and
`rustc 1.99.0 (b940084d7 2026-09-28)`. Both native axes now execute and pass
the real physical-store replacement control, while gate release and
executable-byte verification remain false; six portable jobs are still live.
These regression and primitive passes neither diagnose the prior MSRV failure
nor complete actual production Runner routing or task 9401.

Current Ubuntu stable portable job `112081460476` completed successfully at
`482da8f`, with its complete log retained as
`ci-37405315461/ubuntu-stable-job.log`; five portable jobs remain live,
including macOS and Ubuntu MSRV in release workspace tests. This platform
pass does not expand the native primitive evidence into production acceptance.

Current Ubuntu MSRV portable job `112081461039` subsequently completed
successfully at the same `482da8f` source, with its complete log retained as
`ci-37405315461/ubuntu-msrv-job.log`; both macOS and both Windows jobs remain
live, and actual production Runner routing remains unfinished.

Current macOS MSRV portable job `112081460680` subsequently completed
successfully at `482da8f`, with its complete log retained as
`ci-37405315461/macos-msrv-job.log`; macOS stable and both Windows jobs
remain live, so the current portable matrix is not yet terminal.

Current macOS stable portable job `112081460646` subsequently completed
successfully at `482da8f`, with its complete log retained as
`ci-37405315461/macos-stable-job.log`; only Windows MSRV `112081460668`
and stable `112081460715` remain live, both in debug workspace tests.
Production Runner routing and the remaining plans are still unfinished.

Current Windows stable portable job `112081460715` subsequently completed
successfully at `482da8f`, with its complete log retained as
`ci-37405315461/windows-stable-job.log`; only Windows MSRV job
`112081460668` remains live, in release workspace tests. No new push has
cancelled this original run, and task 9401 remains in progress.

Windows MSRV job `112081460668` subsequently completed successfully, with
its full log retained as `ci-37405315461/windows-msrv-job.log` and terminal
run metadata as `ci-37405315461/final-run.json`. Run `37405315461` is now
completed with all nine jobs successful at exact source
`482da8f07a1248f3e3a62cd0b62173e22433761b`; both native artifacts have already
been independently verified for 81 cases each. This accepts the replacement
regression and preceding primitives across the executed matrix, while native
gate release and executable-byte verification remain false, the earlier
native MSRV retirement failure remains preserved without a proven cause,
and actual production Runner routing and task 9401 remain incomplete.

Owned cleanup coverage now extends `provisioned_broker_access`: a fresh
unclaimed prepared domain is retired by an unprivileged subprocess through
`NativePreparedCleanup`, followed by cold original-domain replay through a
second helper; each success checks actual helper reap and both stream EOFs,
sticky matched success after cancellation and reap, unchanged allocation
counter, physical cgroup absence and a domain tombstone without an execution
receipt. This is new harness code, not yet privileged execution evidence;
serial MSRV executor all-target Clippy is running with log
`local-owned-cleanup-msrv-clippy.log`, and task 9401 remains in progress.

The initial owned cleanup harness compiles on MSRV with all-target Clippy
exit zero in session `30746`. Post-compilation review strengthened the
Root-side assertions to match the complete original-domain tombstone, reject
every execution-receipt filename for that allocation, and compare the
physical store's journal sequence and head hash before and after cleanup.
These strengthened assertions still require compilation and native execution;
formatting, file-size and diff checks pass, and no production claim is made.

Strengthened owned cleanup source `9521e41` now passes executor all-target
MSRV Clippy with exit zero in original session `4185`, retaining
`local-9521e41-msrv-clippy.log`; stable formatting, oversized-file and
committed-range diff checks also pass. Its full stable host gates and
privileged assertions remain pending, so the earlier `482da8f` matrix is
historical evidence only for this new harness extension.

The authorized review branch now points to
`8f179410da81c761425a4bdc9396089145a1804e`; CI run `37409097908` is confirmed
live across all nine jobs, including native MSRV `112093256818` and stable
`112093256861` for the owned cleanup extension. The frozen full stable debug
workspace gate at product source `9521e41` is live in original session `54765`,
retaining `local-9521e41-stable-debug.log`. No native result, full host gate
completion or production Runner acceptance is claimed yet.

Owned cleanup native MSRV job `112093256818` completed successfully at
`8f179410da81c761425a4bdc9396089145a1804e`; its full artifact is retained as
`ci-37409097908/msrv`. Independent verification exited zero for all 81 total
cases, clean source identity and `rustc 1.89.0 (29483883e 2025-08-04)`.
The `provisioned_broker_access` case now executes the unprivileged public
owned-cleanup and cold replay control, including helper reap/EOF, original
tombstone, absence of all execution receipts and unchanged journal assertions.
Gate release and executable-byte verification remain false; stable native,
six portable jobs and the original local debug session remain live at this
observation, and actual production Runner routing remains unfinished.

Physical-store discovery review added a production-entry refusal control that
renames the original operator store, creates a different directory at the same
pathname, and retains the unchanged protected registration bytes; discovery
must refuse with `native discovery store registration missing` and leave the
allocation counter unchanged, then the fixture restores and checks the original
directory identity before subsequent controls. Formatting and diff checks pass;
stable compilation passes, while privileged execution of this new control
remains pending and the original CI run is still live. This control
does not exercise concurrent replacement during discovery or complete Runner
integration, and task 9401 remains in progress.

Current cleanup CI MSRV native job `112069961429` failed; its complete job
log and artifact are retained under `ci-37401653979/msrv`. The expanded
`empty_domain_preparation` control passed in 0.14 seconds, but the matrix
stopped at `private_exec_status`, where the `/bin/sh` MCP fixture refused
completion with `runner cleanup uncertain: closure native retirement deadline`
at `exec_status_native_tests.rs:276`. This is a native acceptance failure,
not a successful matrix or verified full artifact; its cause remains under
investigation and no deadline was relaxed. Stable native and six portable
jobs remain live on the original run, as does frozen debug session `30347`.

Current cleanup stable native job `112069961667` completed successfully at
`e501679509e361cada521b59d5fbbb1c40e121fc`; its full artifact is retained as
`ci-37401653979/stable`. Independent verification completed with exit zero
for all 81 cases, clean source identity and
`rustc 1.99.0 (b940084d7 2026-09-28)`. Gate release and executable-byte
verification remain false, and this primitive/regression pass does not complete
automatic Runner acceptance or replace the preserved MSRV failure. Source
review locates that failure in the existing bound completed-submission closure
loop, which waits for manager retirement and physical absence under a two-second
deadline; available evidence does not yet establish why MSRV exceeded it.

Retirement failure review found that the deadline error discarded the last
actual manager-retirement observation. The private diagnostic now includes
that observed boolean, without issuing a new query after deadline, exposing
identifiers, extending the deadline or relaxing closure checks. This supports
distinguishing a still-loaded manager unit/job from residual-domain retirement
on a future provisioned failure; compilation and native validation of the
diagnostic remain pending behind live frozen debug session `30347`.

The isolated full stable debug workspace gate at owned-cleanup source `cd70627`
completed with exit zero through original session `30347`; its retained log
`local-cd70627-stable-debug.log` ends with successful workspace doc tests.
This gate predates the retirement diagnostic revision. The clean frozen
checkout now validates exact diagnostic source `1dddd15` with serial MSRV
executor library tests and all-target Clippy, retaining
`local-1dddd15-msrv-lib.log` and `local-1dddd15-msrv-clippy.log`.
Current CI's six portable jobs remain confirmed live; the native MSRV failure
and remaining production Runner integration are still unresolved.

Frozen diagnostic source `1dddd15` passed all 23 MSRV executor library tests
and all-target executor Clippy with exit zero through session `32074`, as
confirmed from both retained logs. The serial full stable release workspace
gate now runs at the same clean source, retaining
`local-1dddd15-stable-release.log`; six original portable CI jobs remain live.
These checks do not diagnose or replace the preserved native MSRV retirement
failure, and production Runner integration remains incomplete.

The stable native job `112093256861` in run `37409097908` completed successfully at clean source `8f179410da81c761425a4bdc9396089145a1804e`; its downloaded artifact independently passes `verify_native_evidence.py` with `rustc 1.99.0 (b940084d7 2026-09-28)` across all 81 cases, including the public owned prepared-cleanup success and cold-replay control, while `production_backend`, `gate_released`, and `executable_bytes_verified` remain false because production Runner/service integration is still incomplete; six portable jobs remain in progress and local debug session `54765` remains live.

Full stable debug workspace acceptance at frozen clean product `9521e41559787f3a501fda7e3983a10b29dd679a` completed with exit 0 in original session `54765`; the complete log is `local-9521e41-stable-debug.log` under the dedicated task cache, and the next serial release workspace gate was started only after that terminal result and a healthy memory check (61 GiB available, 2.1 GiB swap used), with one build worker and one test thread; integrated production-host acceptance remains pending.

Implementation review found both public tick entry points return on watcher scan failure before bounded runner capture observation; they now observe owned transport on that refusal path without consuming outcomes or acquiring a writer, with a Linux independent-marker regression after a 4 MiB child write, while timeout handling, native Runner routing, and integrated-host acceptance remain unfinished; compilation and regression validation are queued behind the live frozen-source release session `31810`.

Review of the scan-refusal regression found its first version covered only the writer-opening public tick path; a second independent-marker run now covers `tick_with` with a borrowed memory writer and asserts its records remain unchanged on each failed scan, without using runner polling to drive the marker assertion; both regression validations remain queued behind release session `31810`.

The first scan-refusal regression run failed because a nonexistent store is intentionally observed as empty; the fixture now uses an unchanged non-directory store path, and both corrected public tick regressions pass (2/2, 0.72 s), while a sensitivity run temporarily removing only the two added observation calls fails both independent-marker deadlines and restores the fixed source in a finally block; logs `local-scan-refusal-corrected.log` and `local-scan-refusal-sensitivity.log` preserve both outcomes, and the earlier full stable release workspace gate at frozen `9521e41` completed with exit 0 in session `31810`, which does not validate this later service change.

Changed-source acceptance at `9b85488` now includes successful Rust 1.89.0 executor all-target Clippy with warnings denied (session `2545`) and both corrected scan-refusal independent-marker regressions (2/2, 0.71 s, session `78965`); logs are `local-9b85488-msrv-clippy.log` and `local-9b85488-msrv-scan-refusal.log`, and broader changed-source workspace gates and actual native Runner/service integration remain pending.

The original owned-cleanup CI run `37409097908` at exact source `8f179410da81c761425a4bdc9396089145a1804e` now also has terminal successful Ubuntu stable full gate `112093257000`, whose terminal status is retained in `ci-37409097908/partial-run.json`; GitHub refused the job-log download until the overall run completes, so complete log retention remains pending and the empty download file is not evidence; five portable jobs remain live (Ubuntu MSRV has reached fuzz compilation), and this historical-source CI does not validate the later scan-refusal service fix or prove native production routing.

Original CI run `37409097908` at `8f179410da81c761425a4bdc9396089145a1804e` now has successful terminal Ubuntu MSRV full gate `112093256939` as well as Ubuntu stable, zero-dependency and both independently verified native matrices; exact-source partial metadata is retained in `ci-37409097908/partial-run.json`, four macOS/Windows jobs remain live, and full job-log downloads remain pending until the overall run completes; changed-source local debug session `45924` remains live and actual native Runner/service integration remains unfinished.

Original owned-cleanup CI run `37409097908` at exact source `8f179410da81c761425a4bdc9396089145a1804e` now also has successful terminal macOS stable full gate `112093256908`, verified from retained `ci-37409097908/partial-run.json`; macOS MSRV and both Windows jobs remain live, full job-log download remains pending until overall completion, and neither this historical-source CI nor the still-live changed-source debug session `45924` proves finished native production routing.

Full stable debug workspace acceptance at isolated clean source `9b85488` completed successfully with exit 0 in original session `45924`, including both corrected scan-refusal regressions; complete log `local-9b85488-stable-debug.log` is retained under the dedicated task cache, and the serial full release gate was started only after that terminal result and a healthy memory check (59 GiB available, 2.1 GiB swap used), with one build worker and one test thread; native Runner/service integration and its concurrent/crash acceptance remain unfinished.

### Post-settlement restart review

Review of current source found a second composition obligation beyond retaining active claims: `watch.rs::outstanding_acks` accepts only `RecordKind::EffectAcked`, and its caller filters recovery through the current handler table via `has_advance`; native `Store::settle_execution_on` publishes `RecordKind::ExecutionSettled` and consumes the owner, while `NativeCompletion` carries the checked original handler contract outside its acknowledgement candidate. Therefore routing active native runs alone cannot satisfy crash-after-ack/before-event recovery, especially after handler removal or a seal that no longer retains the original claim record. Integration must recover a matching original native settlement and its authenticated original contract without consulting current handler selection, preserve the existing acknowledgement-before-event key, and prove cold restart plus sealed-history cases before claiming production-host acceptance; it must not treat the legacy outstanding-ack scan as native recovery or infer a contract from a fingerprint. Existing native `advance_native_settled` proves application with a retained completion, but does not itself supply this restart discovery path.

Review of original-contract application found `Pipeline::advance_native_settled` omitted the physical-store proof check that owned settlement already performs; the advance path now checks the current directory before ledger replay or event mutation, and the protected native Acked control substitutes a distinct directory inode while retaining the same original completion and durable settlement, asserts wrapped physical-store refusal plus unchanged records/state/head, and restores the original inode via an owned guard before further fixture work; the shared settlement helper was extracted to keep Rust files below 1000 lines, and compilation/native verification are queued behind release session `10082`, while production-host routing and post-ack restart discovery remain unfinished.

Review strengthened the native advance inode-replacement control to first prove that the writer names the original fixture directory, the completion proof accepts that original physical store, and exact original Acked settlement replay is available; the replacement then changes only physical directory identity while retaining those original materials, so the refusal is not an unrelated missing-settlement or mismatched-proof test; compilation and provisioned native validation remain pending behind release session `10082`.

Original owned-cleanup CI run `37409097908` at exact source `8f179410da81c761425a4bdc9396089145a1804e` has successful terminal macOS MSRV full gate `112093256978`, bringing its completed successful jobs to seven of nine (both native matrices independently verified, both Ubuntu gates, both macOS gates, and zero-dependency); exact partial metadata is retained in `ci-37409097908/partial-run.json`, both Windows release jobs remain live, and this historical-source evidence does not validate the later native physical-store advance guard or complete production-host integration.

Full stable release workspace acceptance at isolated clean scan-refusal source `9b85488` completed with exit 0 in original session `10082`, with full log `local-9b85488-stable-release.log` retained; this complements its full debug pass but does not validate the subsequent native advance guard, which is now frozen at clean `6a2ea1f66d054a5b01c9cb91af7b6d1bc670a8ed` in `advance-review-6a2ea1f` for serial compilation and acceptance, while production native routing and post-ack restart recovery remain unfinished.

Rust 1.89.0 executor all-target Clippy with warnings denied passes at isolated clean native-advance guard source `6a2ea1f66d054a5b01c9cb91af7b6d1bc670a8ed` (session `67215`, exit 0, log `local-6a2ea1f-msrv-clippy.log`), including compilation of the extracted protected native inode-replacement control; actual provisioned execution of that control and complete changed-source host gates remain pending, with no claim of finished native Runner/service or post-settlement restart integration.

Full stable debug workspace acceptance at isolated clean native-advance guard source `6a2ea1f66d054a5b01c9cb91af7b6d1bc670a8ed` completed with exit 0 in original session `12397`, with complete log `local-6a2ea1f-stable-debug.log` retained; the serial full release gate was started only after that terminal result and healthy memory checks, with one build worker and one test thread, while provisioned execution of the physical-store advance control, production native routing and post-ack original-contract restart recovery remain pending.

Original owned-cleanup review CI `37409097908` is now terminal success at exact source `8f179410da81c761425a4bdc9396089145a1804e`: all nine jobs passed, including Windows MSRV `112093256961` and Windows stable `112093256986`; exact terminal metadata and all six nonempty portable job logs are retained in `ci-37409097908/final-run.json` and the platform/toolchain logs under the dedicated task cache, complementing both previously independently verified 81-case native artifacts; this evidence predates the scan-refusal service fix and native advance physical-store guard, whose changed-source acceptance remains separate, and does not complete production native Runner/service integration or post-settlement original-contract restart recovery.

Full stable release workspace acceptance at isolated clean native advance guard source `6a2ea1f66d054a5b01c9cb91af7b6d1bc670a8ed` completed with exit 0 in original session `96579`, with complete log `local-6a2ea1f-stable-release.log` retained; this complements the same-source full debug and MSRV all-target compilation checks, and the remaining stable host gates run serially with one build worker and one test thread after healthy memory checks; provisioned native execution of the new physical-store advance control and production Runner/service plus post-ack original-contract recovery remain unfinished.

Complete required stable host acceptance passes at isolated clean product `6a2ea1f66d054a5b01c9cb91af7b6d1bc670a8ed`: full debug and release workspace suites, all-target Clippy with warnings denied (session `48074`, exit 0), documentation with `RUSTDOCFLAGS=-D warnings` (session `29966`, exit 0), zero-dependency tests, embedding acceptance, formatting, oversized-file and diff checks; complete remaining logs `local-6a2ea1f-stable-clippy.log`, `local-6a2ea1f-stable-doc.log`, `local-6a2ea1f-zero-deps.log`, and `local-6a2ea1f-embed-acceptance.log` are retained under the dedicated task cache, with clean checkout identity independently rechecked; this clears the local review prerequisite but does not establish provisioned native execution of the advance replacement control or complete native Runner/service and post-ack restart integration.

Initial production shared-tick startup recovery is now wired: watcher scan exposes its already verified snapshot internally, both tick paths adopt unresolved original owners into the same Runner, one original-completion helper is active at a time, and normal/failed-scan Runner observation polls or reaps retained transports without a writer; original owner/stopped material survives missing observations and physical-store substitution refuses before scheduling. MSRV executor all-target Clippy initially passes; the added public borrowed-tick cancelled-owner/replacement regression and final changed-source checks are pending. This does not finish native admission, settlement/event application, stopped-owner reconciliation or post-ack cold discovery, and the task remains In progress.

Initial shared-owner recovery review: the public borrowed-tick cancelled-owner/physical-directory substitution regression passes at MSRV (session `25694`, exit 0, `local-native-owners-replacement-corrected.log`), and final executor all-target MSRV Clippy with warnings denied passes (`local-native-owners-final-msrv-clippy.log`); the initial test failed compilation because StoreState lacks PartialEq, corrected to the established complete `snapshot::store_states_eq` comparison. Removing only the physical-store retention guard makes that public tick regression fail as expected (sensitivity session `58166`, exit 0 after asserting test exit 101, log `local-native-owners-replacement-sensitivity.log`); original source is restored in a finally block. No journal or handler launch is authorized by recovery, and ready/uncertain local owners deliberately remain held until subsequent exact settlement integration; full changed-source host and native acceptance remain pending.

Native advance physical-store replacement control is now provisioned and verified at exact clean review source `3ed993126ddfa0ebd2029d82a6af806ce0b366d0` in CI `37413459427`: MSRV job `112106804059` and stable job `112106803964` both completed successfully, and independent verifier checks both retained artifacts with 81 expected cases, actual compilers `rustc 1.89.0 (29483883e 2025-08-04)` and `rustc 1.99.0 (b940084d7 2026-09-28)`, frozen inventories and report/log digests; complete artifacts and verification logs are retained under `ci-37413459427/{msrv,stable}` and corresponding verification logs in the dedicated task cache. `gate_released` and `executable_bytes_verified` remain false; overall portable review jobs remain separate, and this evidence predates initial shared-owner startup recovery product `1218d57`, whose full stable debug workspace acceptance is live in session `34618` at clean frozen checkout `native-owners-review-1218d57`. No complete native production-host acceptance is claimed.

Recovered-completion application review against initial host product `1218d57`: `NativeExecution::settle` sets retained capacity false only after a matching durable original disposition, while `Pipeline::advance` returns `AckedNoAdvance` both when no original advance is declared and when its event is temporarily disabled. Therefore shared application must separate consumed capacity from a pending original-contract event handoff: remove Interrupted/Attempted owners only after exact settlement, release Acked capacity after durable consumption, retain the checked original completion when its declared event is disabled or application refuses, and retry that handoff after a changed journal prefix using the same event key. Dropping every AckedNoAdvance completion would lose an interrupted native advance; continuing to count every consumed handoff as unresolved ownership would retain capacity incorrectly. The legacy watcher cannot repair this, because it scans EffectAcked and current handlers. Full debug acceptance for initial recovery remains live in original session `34618`; application and authenticated post-ack cold discovery are not implemented by this review.

Recovered-owner application is now wired through both shared ticks: ready completions cause a writer attempt even with no other directives, one fairly selected owner settles under its original proof/contract, and Acked outcomes apply their original event only after durable consumption. Consumed owners no longer contribute retained capacity; a declared disabled advance keeps its original completion, parks at the writer prefix and retries after journal progress. Added an expanded provisioned broker scenario and unprivileged public-tick probe for writer contention, readonly refusal, disabled original advance and later resume, exact ack/event ordering and repeated ticks, with no current handler table or catalogue. Compilation, actual native execution and final changed-source gates are pending behind live initial-recovery debug session `34618`; new preparation/admission, shutdown, reconciliation and authenticated post-ack cold discovery remain unfinished.

Recovered application compilation review: executor all-target MSRV Clippy passes after correcting the new probe to the actual Store event API (`local-native-application-msrv-clippy-corrected.log`, original session `26547`, exit 0); its first compile failure and log are retained rather than treated as runtime failure. The new unprivileged shared probe receives an explicit operator-owned fixture TMPDIR for its Runner scratch directory, rather than inheriting an inaccessible Root task cache. Initial owner-recovery source `1218d57` separately completed full stable debug workspace acceptance in original session `34618` with exit 0 and complete `local-1218d57-stable-debug.log`; that historical pass does not validate the later recovered application path or execute its provisioned probe. Final changed-source host and native runtime acceptance remain pending.

The existing public cancelled-owner/physical-store replacement regression also passes with recovered application wiring (session `41034`, exit 0, complete `local-native-application-owner-regression.log`); review of the expanded provisioned scenario added exact original prepared-successor cleanup before fixture teardown, so the additional scenario cannot leak its empty competing domain. These checks do not substitute for execution of the new real recovered-completion application control.

Final executor all-target MSRV Clippy with warnings denied passes for recovered application and the expanded native probe (session `40087`, exit 0, retained `local-native-application-final-msrv-clippy.log`); formatting, Rust source size and diff checks pass. The changed product is ready for frozen full stable host acceptance, followed by exact-source provisioned execution of the new shared-tick control; native new-run admission, complete shutdown/reconciliation and authenticated post-ack cold discovery remain unimplemented, so task 9401 stays In progress and no production gate is released.

Full stable debug workspace acceptance passes at isolated clean recovered-application source `0432aa09ddf766ef47e0e6a00179ad70ee9261e0` (original session `55745`, exit 0, complete retained `local-0432aa0-stable-debug.log`); the serial full release suite is live in session `27946` at the same frozen checkout after healthy memory checks (62 GiB available, 2.1 GiB swap used), with one build worker and one test thread. Earlier review CI `37413459427` at exact source `3ed993126ddfa0ebd2029d82a6af806ce0b366d0` now has seven successful terminal jobs, including macOS stable `112106804194`, with both Windows jobs still live; exact partial metadata is retained in `ci-37413459427/partial-run.json`, and complete portable job logs are not yet claimed. Neither historical-source CI nor the current portable debug pass executes the new provisioned shared-tick application control or completes native admission, shutdown, reconciliation or post-ack cold discovery.

Sealed post-ack recovery review at product `0432aa0`: `store/idempotency.rs::lookup_request` explicitly refuses `store/sealed_replay_unavailable` when a carried key remains but its claiming record is below the cut; `base/execution_claims.rs::{validate,at_cut}` retains original claim hashes only for unresolved owners. Native advance also compares the actual acknowledgement outcome/result with the checked original completion. Therefore a protected completion lookup plus an acknowledgement fingerprint alone cannot preserve that comparison after sealing and must not be substituted for missing outcome evidence. Cold recovery needs authenticated persistent handoff metadata containing the original claim/route, original claim hash, actual acknowledged outcome/result commitment and original event identity or checked contract source, carried through sealing and removed only after matching event application; its writer atomicity, bounds, format/API consequences, prior-format migration and crash cases require review before implementation. This is a concrete persistence obligation, not an implemented index or permission to infer a lost response. Current release session `27946` remains live and the production native gate remains unreleased.

Complete required stable host acceptance passes at frozen clean recovered-application source `0432aa09ddf766ef47e0e6a00179ad70ee9261e0`: full debug and release workspace suites (release original session `27946`, exit 0), workspace all-target Clippy with warnings denied (`3700`, exit 0), warnings-denied documentation (`75127`, exit 0), zero-dependency tests, embedding acceptance, formatting, Rust source size and diff checks. Complete logs `local-0432aa0-stable-release.log`, `local-0432aa0-stable-clippy.log`, `local-0432aa0-stable-doc.log`, `local-0432aa0-zero-deps.log`, and `local-0432aa0-embed-acceptance.log` are retained under the dedicated task cache; embedding/zero-dependency test results and clean checkout identity were independently read. All intensive gates ran serially with one worker/thread and healthy memory checks. Provisioned execution of the new shared-tick control is still required, and native new-run admission plus authenticated post-ack sealed recovery, shutdown and reconciliation remain unfinished; no production gate or task completion is claimed.

Earlier native-advance guard review CI `37413459427` is terminal success at exact clean source `3ed993126ddfa0ebd2029d82a6af806ce0b366d0`: all nine jobs passed, including Windows stable `112106804065` and Windows MSRV `112106804263`; exact terminal metadata and all six nonempty portable job logs are retained in `ci-37413459427/final-run.json` and platform/compiler logs, complementing both independently verified 81-case native artifacts. This final verdict predates initial owner recovery and recovered-application product `0432aa0`; changed-source local host acceptance for that product is complete, but its expanded provisioned shared-tick control still needs exact-source native CI verification, and the production integration task remains unfinished.

### Public API inventory review correction

The frozen `e8df2cf` stable debug workspace run finished with two failures in
`public_surface`: the inventory omitted the intentionally documented
`NativeExecution::start_retained` and `NativeExecution::launch_bound` methods.
All other workspace targets passed in that run; this is not a successful full
workspace gate. The correction adds exactly those two inventory entries, with
no implementation or fixture-scanner changes. The focused stable public-surface
check passed all 16 tests in a verified temporary scope with `MemoryMax=1G` and
`MemorySwapMax=0`; evidence is retained in the task cache as
`local-public-surface-corrected.log`. A full corrected workspace gate remains
required, and task 9401 remains in progress.

### Exact local reservation consumption composition

Shared native application now passes the scheduler into the owned settlement
path and checks `NativeExecution` retained ownership after the application
attempt, including its error branch. That flag becomes false only after
authenticated durable settlement or authenticated replay; optional event failure
occurs later and must not retain consumed capacity. The scheduler removes an
in-flight reservation only if its retained full claim equals the consumed
original claim, so another incarnation and an unbound local reservation cannot
be released. Deferred event completion remains in the native owner map.

Review checked the `settle` flag transition, the post-Ack event-error path,
exact-claim refusal, portable no-op compilation, and unchanged public inventory
and persistent formats. The MSRV focused identity regression passed, followed
serially by executor all-target Clippy with warnings denied, in a verified
1 GiB scope with swap disabled; retained evidence is
`local-native-claim-release-msrv.log`. Actual provisioned capacity reuse through
this new composition and full frozen-source gates remain required; fresh native
admission and sealed post-Ack recovery remain unfinished.
