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
