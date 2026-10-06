# Owned native session composition review

Implementation base: 98702f6; reviewed the owned session composition, shared
protocol loop, bounded input adapter and existing native driver obligations.

The opt-in owned entry has one journal/protocol owner and one caller clock.
A SessionStore facade lends the original writer only during dispatch; idle
observation borrows the driver after that writer view ends. Input workers never
borrow the writer, clock or runner, and controller waiters hold only metadata.
No Arc/Mutex Store, competing stdin readers or independent injected clocks are
introduced. The displayed handler contracts come from the driver's actual
scheduler table, not an independently supplied display table.

The reader is constructed on its worker, keeping existing borrowed API bounds
and permitting a non-Send reader. One queued frame, one current consumer frame
and one worker frame are bounded by the existing 16 MiB wire-byte ceiling.
LF is separate from Vec storage; oversize and idle markers allocate no fake
frames. Reverse elicitation reads the same stream and retries only idle markers,
while explicit stop interrupts its wait. A marker is accepted only at a frame
boundary, preserving ordinary partial-frame and I/O-error behavior.

Idle observation calls the admission-free original driver, never its scheduling
tick. The quiet-client regression seeds a pending effect and due machine
deadline, waits until an idle observation completed, then requires exact cold
records/state after actual stop. Ordinary explicit requests retain the existing
tick boundary. The owned instructions distinguish admitted completion from
pending/retry/deadline progression; borrowed instructions remain unchanged.

Output uses the existing 256-frame/8 MiB allocation-capacity budget including
in-flight writes. Session exit explicitly closes that queue and observes actual
delivery within the first request deadline; healthy EOF cannot lose a queued
reply simply because an empty native driver stopped immediately. Held writes
leave output_drained false. Session-local live guards request bounded feed stop
without joining held I/O, while borrowed sessions keep their joining behavior.
Detached input/feed workers are not claimed retired: the native shutdown report
and output delivery fact have explicitly separate scopes.

Invalid finite bounds refuse before worker construction or fence closure. EOF,
protocol errors and startup errors abort when no earlier request exists; reuse
preserves the first deadline and never de-escalates an earlier abort. A public
original deadline view prevents renewing transport drain bounds. The native
report distinguishes elapsed timeout from other uncertainty. Native/journal
operations remain worker-dependent; control waits remain independently bounded
if those operations stall, and no native ownership is cleared on uncertainty.

Verification: focused session 24844 exited 0 under asserted 1 GiB RAM/zero-swap
limits with stable/MSRV CLI+executor all-target Clippy, 56 CLI library tests,
four downstream owned session tests, twelve borrowed serve-mode tests, nine
elicitation tests, 46 executor library tests, three downstream lifecycle tests
and sixteen public surface tests; local-owned-stdio-integration-focused-v3.log
is retained. Earlier final-check session 71228 failed at a missing instruction
call argument, corrected before that pass. Mutation session 24100 exited 0:
disabling only the worker wire bound caused the direct queued-frame refusal
assertion to fail with test exit 101, and restoration passed all three input
adapter tests; mutation/restoration logs are retained. Thus the outer framing
guard cannot mask a disabled worker allocation guard.

These held-I/O tests use real blocking reader/writer trait implementations and
actual durable writer locks, with no invented native completion or closure
material; their native inventory is empty. Provisioned bound/executing/hung
native tree tests through this new session entry, actual production binary
quiet stdin/blocked OS stdout, portable CI, paired standalone writer strategy,
owner-only exact-incarnation endpoint, CLI stop and signal integration remain
pending. Current CLI selectors still use their previous route, and this public
owned entry is opt-in. Full changed-source stable gate session 30244 subsequently exited 0 at exact
runtime 3157f1cd12efd1d7e60495fe525705c197a8a6f4 under verified 1 GiB RAM
and zero swap, covering formatting, source size, debug/release workspace tests,
all-target Clippy, warning-free documentation, zero dependencies and embed
acceptance; local-owned-stdio-integration-stable-gate.log is retained, and tasks
9401/9402 are not completed or promoted and plan progress remains 3/7.

### Production embedded boundary review — 2026-10-06

At runtime 113cc3f, ordinary standalone execute retains a native paired owner,
but serve_mode still constructs ExecutorLoop with Runner::new. run_with_mode
then locks stdin on the caller and invokes the borrowed serve_dir_with path.
Replacing only the runner leaves quiet stdin able to suspend native observation
and does not publish a control endpoint; production integration must use the
owned input factory and owned native session composition together.

The borrowed serve_dir_with fallback opens a diagnostic/read-only session when
the writer is unavailable or unhealthy, disabling its executor. Production
embedded routing must explicitly retain the intended diagnostic behavior while
never presenting a disabled executor as a published, owned native actor; only
a verified durable writer may construct the owned driver. Endpoint publication
must bind that actual driver, and startup refusal must not fall back to legacy
handler execution. Existing borrowed embedding APIs cannot acquire a Send or
static input requirement merely to simplify CLI routing.

OwnedSessionReport currently omits the original shutdown deadline and returns
an I/O error after actual cleanup without exposing its separate report. The
production wrapper needs the same original-deadline retirement and preserved
initiating-error facts as standalone ownership; output drainage and native
cleanup cannot silently gain separate timeout budgets. The session's sole
reader factory must construct stdin.lock inside its worker, rather than move a
borrowed lock between threads. Actual quiet-input, reverse-reply, blocked-output
and EOF/error binary tests must exercise this selected production composition.
This review establishes remaining integration work, not completion of plan 20
or acceptance of nonempty native execution.

The CLI also sends the same selected mode to http::run_http when --http is
present. That function currently matches ReadOnly separately and uses a plain
Store writer for every other mode, discarding the embedded executor. A new
production mode variant must therefore account for HTTP explicitly rather than
claiming all serve --execute selection is native after changing stdio alone.
The ordinary borrowed Embedded API and HTTP session owner need independent
coverage; a blanket fallback match can silently disable requested execution.
This existing behavior is an unresolved production scope item, not permission
to omit HTTP ownership from the completion audit.

Plan 0020 SCOPE.md explicitly requires this HTTP correction and distinguishes
a transport session ending from the host stopping. Therefore the current
owned stdio EOF-to-abort composition is appropriate for process-scoped stdio
only; it must not be reused for each HTTP request or session teardown, which
would close shared host admission and stop other clients' accepted workflows.
HTTP must route bounded session commands to a retained host whose explicit
shutdown owns native retirement, preserving session isolation and notification
ordering. The existing Sessions container owns per-client cancellation and
subscriptions separately from the Store, matching that required separation.

The reporting prerequisite is now implemented: the additive reporting entry
retains actual protocol I/O failure, original deadline and cleanup/output facts,
while the existing entry preserves error-return behavior. Session 32806 passed
stable/MSRV focused checks and actual failing-reader/writer-release plus exact
first-control-deadline regressions. This resolves the reporting prerequisite
identified above, not production stdio/HTTP routing or nonempty acceptance.

Production run_with_mode now selects retained native stdio on supported Linux.
Exact contended/unhealthy startup results reuse diagnostic session composition
without reopening; borrowed APIs retain explicit selection and input bounds.
The actual binary drain/abort controls passed with stdin open and quiet,
confirming original writer release and incarnation cleanup. Review corrected
cleanup error precedence to preserve initiating protocol failures; expanded
session 7711 validates this and existing diagnostic fallback suites. HTTP
still discards its executor and remains incomplete, and these empty stdio
controls cannot establish native nonempty execution or autonomous scheduling.

The actual production contention diagnostic regression passed in session 66144:
initialize/EOF remains usable under a held writer, with truthful healthy/busy
instructions, no control root or endpoint, unchanged journal records and the
other writer still excluding acquisition. This resolves the tested production
contention fallback obligation; unhealthy startup, reopening races and native
nonempty behavior still require their own evidence.

Production native stdio now carries typed executor errors to the CLI renderer
and attaches actual native, endpoint and output facts to post-session failure.
The real directory stdin descriptor regression passed in session 80616,
retaining the OS IsADirectory kind while confirming empty cleanup and writer
reacquisition. This evidence covers actual production input failure, not
blocked output, native nonempty retirement or HTTP execution.

Actual production blocked stdout now passes: the test observes the named
output worker waiting in a kernel pipe operation before requesting abort,
then receives actual Stopped/writer release and bounded owner exit with
output_drained=false, leaving both pipe handles open. Three full binary
repetitions passed in session 44223 after compact fixture paths corrected
the unrelated 108-byte socket publication refusal. Publication failures now
retain typed diagnostics. Endpoint removal is not claimed once output
drainage consumes the first deadline, and nonempty native retirement remains
unverified.

Full gate 19246 has exposed failures in mcp_execute_workflow after native
production selection. These tests launch the actual serve --execute binary,
discover contracts, run seven real child handler operations and assert exact
acknowledgements plus compensation. Their setup does not register the physical
store with a protected native authority. The current host lacks installed
authority, but the test failure's exact diagnostics must still be inspected
before attributing it to provisioning rather than a lifecycle regression.

These required production domain-semantic cases cannot be replaced by empty
stop tests, removed, or rerouted silently to legacy execution to regain a green
gate. They need exact-source provisioned native execution covering the same
handler order, acknowledgements, preflight refusal and compensation outcomes,
with unavailable-authority refusal covered separately in the portable suite.
The no-fail-fast gate session 19246 terminated with exit 101 after finishing
the debug workspace tests; its only failed target was mcp_execute_workflow,
and the script did not reach release, Clippy, documentation, or later gates.

Further source review found drive_executor still writes each action line
synchronously to stderr even for the Native SessionStore before queued logging
notification admission. The stdout pipe regression used empty execution
inventory, so it does not prove that executor diagnostics cannot block the
native owner. Native admitted observation and explicit request ticks must use
bounded diagnostic admission for this operator stream as well, with actual
drainage/loss tracked independently under the original deadline. The existing
borrowed APIs may retain their explicit behavior, but production native stderr
cannot retain this blocking write. Exact real blocked-stderr execution tests
are required; queued protocol output alone does not cover the operator stream.

The prepared stall-diagnostic test draft includes a bounded 8192-character
executor stderr prefix in the existing stalled assertion, retaining all
workflow scenarios and deadlines. It was applied after the full gate
terminated, and it does not change native authority provisioning.

The applied diagnostic compiled and the exact discovered-handlers workflow
ran in session 24467 under verified MemoryMax=1 GiB and MemorySwapMax=0,
terminating with exit 101 after its original 30-second deadline. Captured
actual executor stderr repeatedly reports `observed pending check_prerequisite
inst-run/3/0` followed by `error exec/mode`; the bounded prefix is retained in
workflow-stall-diagnostic-check.log. This establishes a native mode refusal
at execution, but does not identify the specific authority refusal reason or
prove installed-native workflow acceptance. Formatting and diff checks pass;
release and other full-gate stages remain unexecuted for this change.

The existing DiagnosticOutput implementation is now shared under MCP notify
instead of the standalone module, preserving its original platform bounds,
queue limits, drop counts, worker ownership, and nonjoining close behavior.
Session 26006 exited 0: both existing blocked/saturated and malformed-line
regressions passed, stable CLI all-target Clippy passed, and MSRV CLI
all-target compilation passed under verified 1 GiB/zero-swap bounds. This is
a prerequisite refactor only: the native session still synchronously writes
stderr and requires wiring plus actual blocked-stderr acceptance; the full
workspace gate and non-Linux platform execution were not repeated here.

Native session stderr wiring is implemented in the worktree: SessionRuntime
passes bounded DiagnosticOutput to both tick paths, owned reporting closes
both queues and uses the original stop deadline, and production retains
operator_output_drained and operator_lines_dropped separately. Expanded
serial stable/MSRV checks in session 38977 exited 0 with six owned session,
16 transport, 11 production stop, 61 CLI unit, and diagnostic startup
regressions passing. Review then added an asynchronous broken-operator-output
check before input polling, preserving initiating BrokenPipe and owner abort;
exact revised-source checks in session 3377 exited 0, log
native-stdio-operator-output-final-check.log, under verified 1 GiB/zero swap.
The revised source passes stable/MSRV all-target checks, six owned sessions,
16 control transport tests, all 11 production control cases, and 61 CLI units.
Actual kernel-blocked stderr with action diagnostics remains required, as
does the full workspace gate; this does not establish installed native
nonempty workflow acceptance or complete task 9401.

Actual stderr regression development exposed a proof weakness in the earlier
stdout test: a `fsm-protocol` name prefix also matched the input worker,
whose pipe wait does not prove blocked output. The worktree now requires
`fsm-protocol-o`, and the existing stdout test passes with that stronger
observation. The new stderr case currently fails to observe a blocked output
worker, including after switching to effect-observing tools/call requests;
session 80874 exited 101 with 11 existing cases passing and this new case
failing. It remains uncommitted pending fixture diagnosis, and stderr
backpressure acceptance is not established by the initial broad-prefix runs.

The stderr fixture now feeds up to 2000 real effect-observing calls from a
separate input worker, avoiding test-controller pipe writes while filling
variable-capacity stderr pipes. It observes specifically fsm-protocol-output
blocked in a kernel pipe wait, obtains actual authenticated Stopped and
writer_released=true, and reacquires the physical Store writer before
opening stderr. Stderr remains blocked beyond the original 500 ms delivery
deadline; after release the actual process exits unsuccessfully and captured
bytes contain the real pending-action diagnostic. Session 63222 exited 0:
all 12 production control tests, stable all-target Clippy, and MSRV all-target
compilation passed under verified 1 GiB/zero-swap limits, log
production-stderr-fed-check.log. Earlier insufficient-fill runs are superseded
for this fixture; the stronger existing stdout observation also passes.
The final CLI error renderer still writes stderr synchronously after native
cleanup, so process exit can wait for stderr even though the native owner
and writer have retired; this test releases that separate renderer and does
not claim bounded process exit while stderr remains blocked. Full workspace
and installed-native nonempty execution gates remain incomplete.

Follow-up review identified the synchronous pre-initialization request
warning in serve_session_core as another native owner stderr path. It now
uses the existing bounded operator queue for owned sessions, preserving
borrowed behavior. The actual pipe regression deliberately omits the
initialized notification and verifies captured warning and action lines
after authenticated stop and writer reacquisition while stderr was held.
Expanded checks in session 36725 exited 0 under verified 1 GiB/zero-swap
limits: stable/MSRV all-target checks, 12 actual production control tests,
six owned sessions, 16 transport cases, and 61 CLI units pass, with diagnostic
startup regressions also passing; log native-stdio-warning-output-check.log.
Full workspace and native platform acceptance gates were not run here. Final process error
rendering and opt-in panic-hook stderr remain separate boundaries requiring
review, and installed-native workflow acceptance is still unproven.

Actual stderr sensitivity review completed in session 48737, exit 0,
under verified MemoryMax=1 GiB and MemorySwapMax=0. Replacing only warning
queue admission with synchronous stderr made the real pipe regression exit
101; replacing only action queue admission with synchronous stderr also
made it exit 101. Both failures were test failures after successful
compilation, not compilation refusal. The mutation harness restored exact
original serve.rs bytes in finally, then all 12 actual production control
tests passed; git status confirmed no residual runtime changes. Evidence is
retained in native-stderr-mutations.log. This establishes sensitivity to
both owner-blocking stderr paths, without native authority acceptance or
verified handler entry, and does not remove the separate final-renderer
process-exit limitation. Tasks 9401/9402 and plan completion remain unchanged.

Embedding guide review removed an obsolete paragraph claiming the shipped
production native stdio selector, owner-only endpoint, stop command, and
paired standalone strategy were unfinished. It now states the actual Linux
x86_64/aarch64 compositions, borrowed/HTTP boundary, separate operator
delivery facts, and still-unfinished signal and nonempty native acceptance.
Source tracing confirms NativeAdmissions::queue discovers namespace and
generation from the physical store and maps discovery refusal to exec/mode;
this is a prerequisite explanation, not proof that every observed exec/mode
comes from discovery. The workflow fixture remains unregistered and all four
production workflow failures remain unresolved. Documentation-only diff
checks pass; runtime gates were not rerun for this prose correction.

Native workflow fixture compatibility review found a second concrete
prerequisite beyond physical-store registration: workflow_handler reads
FSM_WORKFLOW_DIRECTORY and FSM_WORKFLOW_FAILURES from inherited environment,
while production containment launch uses env_clear and only LANG/LC_ALL,
and entry executes the approved argv without reinjecting the operator
environment. A registered fixture must pass directory and failure inputs
through explicit approved arguments, arrange actual handler identity access
to its resource files and executable, and preserve all original discovery,
acknowledgement, failure and compensation assertions. No weaker legacy
execution route or fabricated native receipts satisfy this acceptance.
Runtime source remains frozen at db058013b9133f0c344abce8debf06912a9dcbe3
while full stable gate session 86747 remains live; its later stages are
configured to run even when known workflow failures recur.

The cache-only workflow-explicit-handler-arguments.rs.draft removes those
operator environment dependencies and carries the same fixture inputs in
approved argv, preserving resource/run placeholders and all four scenarios;
rustfmt syntax formatting passed, but it is unapplied and not Cargo-compiled
while the frozen full gate remains live. Registration, handler identity
access, and installed-native acceptance still need implementation and proof.

Final process delivery review traced args::serve_run through
cli::execute::report to render::emit_error, which synchronously writes
stderr after native cleanup. Cache-only native-final-error-output.rs.draft
carries the original Instant alongside the initiating ExecError and renders
through bounded OutputControl with no join or fresh deadline. It is syntax
formatted but unapplied/uncompiled while gate 86747 remains live. Integration
requires preserving original error classification and cleanup facts, actual
process exit with the stderr reader still closed, and explicit treatment of
a final frame that cannot be confirmed before an expired deadline; current
healthy-error delivery tests cannot be weakened silently to accept absent
diagnostics. This remains an incomplete bounded-exit requirement.

Full stable gate session 86747 at frozen runtime commit
db058013b9133f0c344abce8debf06912a9dcbe3 completed its debug workspace
stage with exit 101: the only failed target is mcp_execute_workflow, whose
four original scenarios again stall at the pending handler with exec/mode
diagnostics. The actual 12 production control tests pass, including both
output-worker-specific pipe regressions. Format and source-size stages
passed. Unlike the earlier fail-fast script, this gate records every stage
and proceeds to release workspace verification, currently live under the
same 1 GiB/zero-swap scope; release, Clippy, docs, zero-dependency, and embed
results remain unproven until their authoritative stage exits are recorded.
No acceptance flag or task status is promoted by this partial result.

Full stable gate 86747 terminated with exit 1 after all eight stages at
frozen runtime db058013b9133f0c344abce8debf06912a9dcbe3. Debug and release
workspace tests each exit 101 with only mcp_execute_workflow failing its
four scenarios; format, source-size, workspace all-target Clippy, denied-warning
docs, zero-dependency, and embed acceptance stages all exit 0. The overall
gate is failed, not accepted; native-stderr-full-stable-gate.log retains every
stage. After authoritative termination, the explicit-handler-arguments
workflow draft is applied, with a real helper subprocess test clearing
inherited operator environment; registration and native acceptance remain
incomplete, and the previous gate does not validate this later test change.

Applied explicit handler arguments pass session 99338, exit 0: the real
helper subprocess executes check_prerequisite with inherited workflow
environment cleared, preserves the literal resource/run arguments, and
writes the expected call while leaving the resource phase active. Stable
CLI all-target Clippy and MSRV all-target compilation also pass under
verified 1 GiB/zero-swap limits; workflow-explicit-arguments-check.log retains
the evidence. All original production scenarios and deadlines remain present;
this helper proof does not establish authority registration, native handler
entry, or success/compensation workflow acceptance.

The cleared-environment helper regression now launches all seven real
operations in three independent sequences: success, partial perform_work
failure, and restore failure. It verifies exact 0/7 subprocess exit codes,
ordered call history, complete versus partial external work, and actual
active/suspended resource phase after restoration. The scoped serial command
exited 0 with the regression, stable CLI all-target Clippy, and MSRV
all-target compilation passing; workflow-helper-compensation-check.log
retains results. This strengthens only fixture argv/environment compatibility;
no native entry or original production workflow acceptance is claimed.

Actual broken-operator-output coverage now launches production native stdio
with a real pending action, removes the sole stderr pipe reader, requests
one executor-observing tool call, and leaves stdin open. Within the original
test observation bound the actual process exits 1, the Store writer can be
reacquired, pending work and journal sequence remain unchanged, and the
incarnation endpoint is removed. This tests asynchronous OS BrokenPipe
handling on quiet input, not a synthetic failing writer or installed handler
entry. Initial compilation caught an incorrect Store accessor, corrected to
its public state.last_seq; final session 59587 exited 0 with all 13 actual
production controls, stable all-target Clippy, and MSRV all-target compilation
passing under verified 1 GiB/zero-swap limits. Evidence remains in
production-broken-stderr-final-check.log. Full later-source workspace and
installed-native workflow acceptance remain incomplete.

Workflow fixture external resource files now live in a separate resource
subdirectory, and approved helper argv names that resource path instead of
the directory containing the store, handlers, and diagnostics. Original
workflow assertions still inspect the same call order, phase and work bytes
through the separate path. The actual seven-operation cleared-environment
sequences pass across success, partial-work failure and restoration failure;
stable CLI all-target Clippy and MSRV compilation also pass under verified
1 GiB/zero-swap limits, command exit 0, log
workflow-resource-separation-check.log. No filesystem access is broadened by
this refactor; provisioned handler identity access, physical-store registration
and immutable catalogue approval still require native integration and proof.

Broken stderr coverage now also runs an empty-store session that admits
exactly one pre-initialization tools/list warning and then remains quiet
with stdin open; no later log enqueue can mask missing asynchronous worker
failure observation. Both that case and the original pending-action case
confirm failed exit, physical writer reacquisition, unchanged journal/pending
state, and removed endpoint. Session 50355 exited 0: all 13 actual production
control tests, stable CLI all-target Clippy, and MSRV all-target compilation
pass under verified 1 GiB/zero-swap limits, log
production-single-warning-failure-check.log. Installed-native execution and
bounded final-renderer exit remain separate incomplete requirements.

### Quiet operator failure guard mutation review (2026-10-07)

Reviewed committed source 7f3e777 by temporarily neutralizing only the
operator-output broken-state predicate in `serve_session_core`; the actual
production single-warning case failed after 2.01 seconds with
`broken stderr did not abort quiet native session` (Cargo exit 101), proving
that subsequent diagnostic enqueueing is not what makes this case pass.
The review script restored the original source in its finally block and
all 13 actual production control tests then passed (exit 0).
The serial scope asserted MemoryMax=1073741824 and MemorySwapMax=0 before
Cargo; evidence is retained in the task cache as
`native-quiet-stderr-mutation.log`, with no residual runtime diff.
This is empty/unadmitted production coverage, not installed native handler
execution acceptance; plan 22 remains incomplete and the previously recorded
full-gate workflow failures remain unresolved.

### Workflow failure diagnostic allocation review (2026-10-07)

Review found the stalled-workflow assertion decoded the entire stderr file
before truncating to 8192 characters; it now reads at most 8192 input bytes
before lossy UTF-8 decoding, retaining useful diagnostics even when the
prefix ends within a multibyte character.
The in-memory regression proves the reader consumed exactly 8192 bytes and
did not consume the suffix; it creates no filesystem fixture.
Stable focused regression and CLI all-target Clippy passed in a verified
1 GiB RAM / zero-swap scope (workflow-bounded-diagnostic-check.log).
Format and diff checks passed; full workspace gates and native workflow
execution were not repeated for this test-only diagnostic change, and the
previous production workflow failures remain open.
This bounds assertion allocation only and does not establish the cause of
the previously reported host memory or swap pressure.

### Final renderer process-exit and framing review (2026-10-07)

A cache-only safe-Rust probe started a named stderr worker writing 1 MiB
to an actual unread pipe; an independent Python observer checked that exact
child's named worker was blocked in a pipe wait before releasing main through
stdin, while retaining the stderr reader open and unread. Normal main return
exited successfully in 0.001 seconds on this host; therefore a detached blocked
stderr worker itself did not prevent process exit in this experiment.
The compile and probe ran serially in a verified 1 GiB / zero-swap scope;
source and evidence are retained as blocked-stderr-exit-probe.rs,
blocked-stderr-exit-probe.py and blocked-stderr-exit-probe.log in the task cache.
This is runtime feasibility evidence, not production bounded-exit acceptance.

Review also rejects the earlier native-final-error-output.rs.draft as an
integration candidate in its current form: render::write_error(false, ...)
emits multiline human text (including a hint line), whereas OutputControl's
enqueue rejects internal newlines by its protocol framing contract.
Ignoring enqueue failure would silently discard healthy human diagnostics.
Final diagnostic transport must preserve the human and JSON rendering bytes
with separate diagnostic framing while sharing bounded allocation and
nonjoining delivery semantics; protocol single-line validation stays intact.
Original-deadline expiry still requires an explicit delivery policy and
actual production tests, and no final-renderer implementation is accepted.
No runtime source changed; documentation diff checks passed, and full Cargo
gates were not repeated for this review-only update.

### Separate bounded diagnostic framing implementation (2026-10-07)

The shared output worker now has a crate-private diagnostic enqueue path
preserving arbitrary rendered bytes, including multiline human errors;
protocol enqueue still performs its original single-line newline validation
before using the same retained/in-flight accounting and worker.
Existing production operator-line admission uses the diagnostic path while
retaining its 64 KiB line ceiling, malformed-line refusal and loss counting.
Real worker tests preserve actual render::write_error human and JSON bytes,
prove protocol enqueue still rejects the human frame, and exercise diagnostic
allocation limit/plus-one and frame limit/plus-one (including empty frames).
Stable notify unit tests and all 13 actual production control tests passed;
CLI all-target Clippy and MSRV all-target compilation passed in a verified
1 GiB / zero-swap serial scope (diagnostic-framing-check.log).
Formatting and source-size checks passed. This internal transport change
alters no public capability or rendering contract; four public guides need
no corresponding capability update. Final renderer wiring, original-deadline
delivery policy and production bounded-exit acceptance remain incomplete.
The full stable gate will run on the committed source before acceptance;
prior native workflow failures remain open and are not bypassed.

### Native workflow fixture launch-path review (2026-10-07)

Source review identifies an additional concrete fixture prerequisite:
containment/launch.rs explicitly sets DynamicUser=yes and ProtectHome=yes,
whereas mcp_execute_workflow's approved handler executable is current_exe
and its explicit resource argument is below the cache fixture under /home.
Physical-store registration and catalogue publication alone cannot make those
paths reachable by an admitted production handler under this profile.
This is source evidence about the launch profile, not a claim that the current
unregistered first-effect failure reached launch or that a handler ran.

The provisioned workflow harness must use a protected, traversable copy of
the exact test helper artifact outside protected home paths and a separate
fixture-owned writable resource directory reachable by changing handler UIDs;
its operator store remains private to the actual non-root operator.
Existing runner native cases demonstrate a fixture resource under /dev/shm;
that precedent is resource access only, not permission to execute an artifact
there or to make an operator store writable by handler identities.
Authority catalogue input also needs a root-protected source parent, as
authority::run checks protected_directory before catalogue publication.
Register, catalogue, provision-broker and serve use the actual production
authority interface with a non-root, non-handler operator UID.
Each scenario needs its own immutable catalogue because approved failure
arguments differ; all original discovery, ordering, acknowledgement and
compensation assertions remain required.
The full stable gate remains live at session 50807 on runtime 7cf0e39;
this review changes no runtime source and proves no provisioned workflow.

### Original-deadline final pipe delivery feasibility (2026-10-07)

A small independent Linux pipe experiment opened /proc/self/fd/<owned-writer>
with O_WRONLY|O_NONBLOCK, verified the original handle remained blocking,
delivered exact multiline diagnostic bytes through the new handle, filled
the pipe and observed EAGAIN immediately on the next small-frame write.
The original handle's flags remained unchanged after saturation.
Host Linux UAPI headers define PIPE_BUF=4096 and O_NONBLOCK=00004000.
This supports an immediate atomic small-frame attempt in the final caller
renderer, after native owner return, without taking the stderr worker's lock
or adding a waiting budget when the original deadline has already expired.

The cache-only native-final-error-output.rs.draft now uses this path for
frames up to 4096 bytes, checks the opened handle is a FIFO, and waits on
WouldBlock/Interrupted only while the original deadline still has time.
Other handle types, failed independent opens and larger frames use a bounded
diagnostic worker with the same original deadline and no join.
Formatting passed; this candidate is neither applied nor Cargo compiled
while stable gate 50807 remains live on runtime 7cf0e39.
Production tests must establish both healthy final diagnostic preservation
and exit with stderr still blocked; inherited-handle access failures and
non-pipe deadline-expired delivery remain explicit limits to review.
This experiment is not production shutdown or native execution acceptance.

### Local native prerequisite revalidation (2026-10-07)

Read-only host preflight outside the filesystem sandbox confirms operator UID
1000, successful noninteractive sudo id reporting UID 0, systemd 259, and
a read/write cgroup2 mount; the sandbox's read-only cgroup view is not the
host's authority capability. No authority binary or authority base is currently
installed. These observations permit preparing an exclusive local native
fixture run after serial gate 50807 terminates; they do not prove supported
native profile, actual handler admission, closure or acceptance.

Reviewed authority_install.py requires a regular exact-digest artifact within
64 MiB, root-protected installation ancestors, exclusive hard-link publication
without overwriting or reusing an existing installation, and removal only
after matching device/inode, owner/mode and digest.
The next local provisioned run must use that owned-only lifecycle with exact
fresh Cargo artifacts and task-cache TMPDIR, preserve native unresolved
ownership on failure and record every terminal probe result before cleanup.
No installer or native probe was run during this preflight, and the current
full stable gate continues alone with runtime unchanged and zero swap.

### Native probe authority retirement review (2026-10-07)

Before local provisioned execution, review found authority_probe.py always
invokes installer removal in its finally block, whereas native Fixture::Drop
ignores cleanup failure and deliberately retains a namespace when a domain
is changed, populated or unknown. A failed/timed-out probe can therefore
remove its exact installed authority while unresolved native state remains.
The unmodified harness must not be used for the upcoming local acceptance.

Cache-only authority-probe-retirement-guard.py.draft requires initially clear
authority state, refuses artifact removal if any namespace survives or state
cannot be inspected, and records exact retained installed device/inode/digest
and source commit for subsequent matched recovery. It changes no native
closure predicate and never removes or force-clears a retained domain.
Python AST syntax validation passed; source is unapplied and unexecuted while
full stable gate 50807 remains live on runtime 7cf0e39 with zero swap.
Meaningful guard tests and terminal local native execution remain required
before accepting the harness change or promoting any lifecycle task.
