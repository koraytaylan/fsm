# Standalone production integration review

At runtime 40e7da0, cli/execute.rs calls service::run, whose local runner is
still constructed with Runner::new. It publishes no control endpoint and its
infinite loop has no explicit stop transition. PairedNativeExecutor is an
opt-in composition and does not yet replace this production path.

The existing exclusive contract counts three consecutive writer-unavailable
ticks before failing. PairedNativeExecutor::tick currently returns only lines,
discarding TickOutcome.writer_unavailable. Production integration must preserve
that structured fact rather than parse rendered error strings or silently
weaken exclusive behavior. Keep the existing public tick convenience API while
adding an outcome-returning driver entry for the production loop.

The production emit callback is synchronous. Calling it inside the ownership
loop would let blocked stdout prevent admitted observation and closure even
though the independent endpoint can still return an uncertain report. The
standalone loop needs bounded queued log delivery with actual delivery tracked
separately from native cleanup and writer release, retaining the first shutdown
deadline and avoiding a joining Drop guarantee. Saturation must not stop the
ownership pump; dropped diagnostic lines need an explicit bounded policy.

The current log_line writes plain lines to stdout; log_mode alone writes stderr.
The existing MCP OutputControl has the needed complete-frame accounting and
nonjoining close, including in-flight allocation charging, but its start/enqueue
methods are private to notify and Notifier serializes JSON-RPC values. Share the
queue internally rather than routing standalone lines through JSON-RPC or
copying its accounting implementation. Preserve ordinary line bytes and stream
selection; a multiline diagnostic must be split or explicitly refused before
queue admission. The Notifier marks any enqueue refusal as broken, so its policy
cannot stand in for standalone dropped-line handling.

The poll interval is an ordinary scheduling interval, not a bound for shutdown
responsiveness: endpoint requests must wake or be observed by a bounded admitted
pump without waiting an arbitrarily long configured interval. A stopped actor
must not enter another ordinary tick or retain the endpoint indefinitely.

Publication must create or validate the same private default root used by
execute stop, bind the exact retained actor and original physical directory,
and close its incarnation endpoint under a finite bound after shutdown. A
missing endpoint must never be reported as authenticated cleanup. Existing
read-only/check/list-dead paths must remain free of executor publication.

Acceptance must launch the actual standalone binary, discover its endpoint,
issue drain/abort while another writer is held, and verify actual actor exit,
writer facts, admission closure and incarnation cleanup; nonempty claims require
installed native proof and original interrupted settlement, not empty tests.
Exclusive contention, blocked log output, long scheduling intervals and repeated
controls must exercise the same production owner. The approved Linux signal
decision in EXECUTOR-LIFECYCLE.md keeps ordinary SIGTERM as default termination,
with protected supervisor lease EOF and durable claims retained until verified
closure/settlement; it does not promise graceful drain or require a signal
callback. Actual SIGTERM/SIGKILL production acceptance must test that policy,
separately from independently woken explicit drain/abort control.
This review identifies integration obligations and does not claim they are
implemented or promote either ownership or shutdown tasks.

Endpoint retirement must reuse the original shutdown deadline: a fresh close
timeout after output drainage extends the caller's budget. The cached design
adds an absolute-deadline close and retains the original deadline in the actual
standalone result; an expired close still stops transport admission but does
not promise filesystem removal. Cleanup error/removal facts must remain separate
from both initiating execution errors and authenticated native shutdown facts.

Output drainage can consume that original budget before endpoint removal starts.
Do not hide unconfirmed endpoint cleanup behind a successful native Stopped
report or perform synchronous filesystem deletion on the ownership thread.
Conversely, immediately closing the endpoint when Stopped becomes visible can
race the accepted stop response; native cleanup, endpoint removal and response
delivery require distinct observations. Production acceptance must exercise
generous and expired budgets and that response/retirement race.

The implementation and actual expired/replaced-file and excessive-deadline
transport tests are currently cached drafts, uncompiled and unapplied while
full stable gate 76940 verifies frozen runtime; this review is not acceptance.
