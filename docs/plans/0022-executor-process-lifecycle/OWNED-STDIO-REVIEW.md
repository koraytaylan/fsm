# Owned production stdio implementation review

The actual run_with_mode entry constructs stdin/stdout and owns its directory
session, store, ExecutorLoop and GlobalClock. The borrowed serve_session_degraded
API accepts non-Send input and a borrowed dyn Clock and must remain usable;
threading that API by adding Send/static bounds would break its callers.

Implement the independent production route at the owned entry. Read capped
protocol frames on an input worker; serialize all journal mutations and their
clock reservations on one store owner. Preserve the single owner because
GlobalClock uses thread-local reservation state; creating separate mutable
clocks for protocol and lifecycle writes would change deterministic injected
clock behavior. No reader or output worker may hold the store while waiting.

Queue complete canonical output frames to one output worker with a declared
byte/count budget. A blocked writer may stall protocol delivery but cannot hold
an executor/store lock or stop already-admitted observation. Notification and
response ordering must preserve complete lines; budget exhaustion must remain
explicit and bounded rather than accumulate an unbounded output backlog.

The lifecycle pass must exclude queue admission, preparation start, bound entry,
retry scheduling and machine deadline polling. Native authority supplies actual
handler timeout; the host observes original work and applies genuine completion
or proof-backed interruption. Ordinary request ticks retain the existing
admission policy while Running, but a published stop must close it before a
concurrent protocol tick can admit work.

A separate endpoint/control report deadline cannot wait for the store owner,
output worker, native discovery, filesystem I/O or journal fsync. Native closure
requests require the retained exact local admission identity; foreign observed
claims are protected, not stopped. If writer/native work stalls, return Uncertain
by the original report deadline and retain claims, helper ownership and bounded
control state. Separate closure observation from writer settlement so native
closure can proceed during writer contention; do not claim writer release when
its actual owning task remains blocked.

Implement and prove paired-owner controls, quiet open stdin, blocked stdout,
no new pending effect after timeout, all local admission phases, repeated/stale
controls and actual helper retirement. This draft introduces no production
behavior or acceptance evidence.

## Reverse protocol requests

The existing serve_session_degraded request path builds SessionIo from the same
input and passes it to methods::handle_request; elicitation waits for a client
response there. An owned input worker must demultiplex replies for outstanding
server requests and preserve queued client requests/cancellations, rather than
remove SessionIo support or race two stdin readers. A store owner blocked on
elicitation still cannot be the only owner of native lifecycle progress.
Bound reverse-request state and input backlog, preserve request-ID correlation,
and prove stop/timeout progress with a client that never answers elicitation.
Avoid holding store/execution locks while awaiting a reverse-protocol response;
revalidate writer state when applying the client's eventual answer.

FeedHandle::stop_and_join also waits for a feed that can be inside Notifier::send.
The owned output route must ensure enqueue is bounded and nonblocking so feed
shutdown cannot join a thread blocked on actual stdout. The output worker itself
requires independent bounded report observation instead of unconditional join.

## Review status

Source inspection is evidence of implementation constraints, not runtime proof.
The independent owned route, reverse-request demultiplexing, lifecycle driver and
output integration remain unimplemented. The cache-only output queue and
interrupted-retirement drafts are unapplied and untested. Required gates and
actual production native acceptance remain pending; this review closes no task.
