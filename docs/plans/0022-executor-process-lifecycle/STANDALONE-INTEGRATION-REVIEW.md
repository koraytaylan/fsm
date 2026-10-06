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
loop would let a blocked stderr prevent admitted observation and closure even
though the independent endpoint can still return an uncertain report. The
standalone loop needs bounded queued log delivery with actual delivery tracked
separately from native cleanup and writer release, retaining the first shutdown
deadline and avoiding a joining Drop guarantee. Saturation must not stop the
ownership pump; dropped diagnostic lines need an explicit bounded policy.

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
Exclusive contention, blocked log output, long scheduling intervals, repeated
controls and ordinary OS termination must exercise the same production owner.
This review identifies integration obligations and does not claim they are
implemented or promote either ownership or shutdown tasks.
