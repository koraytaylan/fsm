# Control pump integration review

The completion-only pass in 94d3f5a is necessary but does not establish the
independent production lifecycle pump required by task 9402: it still reads
filesystem metadata, folds a snapshot and settles through the writer.

The current production entry in mcp/serve.rs owns protocol input, Store,
Clock and ExecutorLoop on the same thread. SessionIo can also read input
inside an elicitation request. Moving only the outer input read therefore
does not isolate native progress from an unfinished elicitation; moving only
output to Notifier::queued does not isolate progress from journal fsync.

The next implementation must establish these ownership boundaries:

- One bounded input reader owns stdin and dispatches complete frames to the
  request owner; reverse replies and cancellations use that same reader.
- One request/journal owner retains Store and its Clock, preserves ordering
  and handles ordinary requests; it cannot be the deadline-report owner.
- One native transport owner retains the actual runner and preparation,
  execution and closure helpers; journal requests carry original immutable
  claim/contract material, never replacement handler-table resolution.
- An independent control/report owner closes admission before acknowledging
  stop, binds requests to the exact endpoint incarnation and can report
  Uncertain by the deadline without waiting for the journal/native workers.
- Queued protocol output retains its explicit control handle; shutdown
  observes drainage under the remaining deadline and never joins a writer
  blocked in write or flush.

Admission closure must cover enqueue, allocator dispatch, claim publication
and bound entry as separate transitions; an atomic flag checked only at the
start of a tick leaves a concurrent stop/publication race. Already-published
local claims transfer provenance before an uncertain reservation is removed.
Observed foreign claims remain recoverable but cannot become abort targets.

The native owner must retain both execution and closure transports after an
interrupted settlement: scheduler capacity is released only after genuine
helper reap/EOF and exact original settlement reconciliation, with the
authenticated closure proof retained separately. An expired response or
missing current claim does not prove either retirement or successful cleanup.
Unknown allocation and uncertain preparation phases stay charged when their
original identity cannot be recovered; an empty durable claim list does not
make these phases Stopped.

Review gates for that integration are the actual production quiet-stdin and
blocked-stdout stop cases, elicitation outstanding during stop, concurrent
publication and repeated controls, writer contention/stalled journaling, and
two live executors where stopping one leaves the other's native domains
untouched. The current local completion-pass tests prove scheduling exclusion;
the provisioned bound-owner probe is compiled but unexecuted here, and none
of these production lifecycle gates is satisfied by that narrow evidence.
