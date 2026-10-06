# Production control ownership integration review

Current runtime cd0213b implements the actual owned library session, private
endpoint and production-dispatched stop client; it does not yet publish that
endpoint from the current serve/execute hosts or activate the native selector.
The existing full stable gate is session 45893, with runtime source frozen.

Source inspection establishes the ownership transfers required next:

- `mcp::serve::ExecutorLoop` holds watcher, scheduler, runner and pipeline but
  owns no Store; `serve_dir_with` opens the one writer and lends it to each tick.
  A production owned session must transfer that same writer and those original
  components into OwnedNativeExecutor::from_owned_parts, retaining the actual
  handler table and native ownership rather than constructing a replacement
  runner or opening a second writer.
- `run_with_mode` currently hands a borrowed stdin lock to serve_dir_with;
  the production owned route must construct its sole reader in the existing
  owned input worker and keep reverse replies on that same reader, with one
  engine clock and the already implemented queued output/control separation.
- The startup path currently degrades healthy writer contention to read-only
  and preserves diagnostics for an unhealthy store; the native route must
  preserve that behavior, never start handlers without a healthy writer and
  never advertise a native owned executor endpoint for a diagnostic-only host.
- `service::run` uses tick_reporting to acquire/release Store each iteration;
  that is the existing paired standalone writer strategy. Replacing it with
  OwnedNativeExecutor::new holding one Store for its whole lifetime would break
  paired deployment and does not satisfy the required standalone integration.

The paired driver therefore needs an explicit writer strategy which preserves
its original runner/scheduler/claims across temporary writer availability,
observes admitted native transports and stops local domains while writer access
is blocked, and settles only through a healthy original writer when available.
Its independent control must retain unresolved claims/reservations at the first
deadline and report uncertainty if durable settlement cannot finish; releasing
a temporary writer or proving native closure alone cannot count as completed
durable shutdown. Retained authentic completion policy must precede interrupted
settlement, and foreign claims/trees must remain outside this actor's stop scope.

The candidate production-native-selection.patch changes the two constructors
but supplies neither the independent production stdio pump nor the paired
control/writer strategy, so applying that patch alone cannot complete task 9402.
It remains unapplied until the required installed exact-source native happy
and executable-selection acceptance is proved; opt-in library/control tests
are not a substitute and do not authorize changing those acceptance flags.

Required next acceptance includes actual current production binary quiet stdin,
blocked OS stdout and elicitation stop; native completing/hung process and MCP
trees; writer contention and failed journaling; and two genuinely live executor
actors demonstrating that one control stops only its original local ownership.
The retained successor cgroup in current bound probes is useful domain exclusion
evidence but is not a second live executor or paired writer acceptance.
Actual OS console/ordinary termination and hard-kill recovery remain separate
axes, with no Drop-based guarantee or PID/absence-based proof substitution.

This review defines the remaining implementation rather than declaring any
production capability complete; task 9401 stays in progress, 9402 stays planned
and plan progress remains 3/7, with plans 20/21/23 still pending in their full scope.

## Existing read-only closure capability

Inspection of run/native_client/shutdown.rs establishes that NativeShutdown::start
already accepts a verified durable read-only snapshot: it rejects memory and
poisoned journals, requires the original unresolved claim and verified record
hash, and checks physical-store/registered authority binding before requesting
closure. It does not require or acquire the writer lease. Its settlement method
separately requires the healthy original writer.

The paired implementation can reuse this proved separation, retaining or
refreshing a verified original snapshot for local closure selection while
deferring settlement to temporary healthy writer acquisition; it should not
invent a weaker arbitrary-control target API or relax original binding guards.
Transport observation must precede writer attempts, and actual original
completion/helper retirement must still precede interrupted settlement.

The current owned poll couples closure selection to a successful
observe_admitted_with result and retains one Store; those assumptions must be
split for paired operation rather than treating that method as writer-independent.
An unavailable/unverified snapshot still produces conservative uncertainty,
preserves local claims and reservations and cannot prove native termination.
This finding narrows the required implementation mechanism without narrowing
the paired live-actor, writer-contention or production acceptance requirements.


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
