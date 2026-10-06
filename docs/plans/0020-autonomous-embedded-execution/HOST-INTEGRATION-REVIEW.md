# Production HTTP host integration review

At runtime 9cfc90b, production stdio retains native ownership and independently
observes admitted work while input is quiet; ordinary autonomous scheduling
remains plan 20 work. HTTP still opens a plain Store and discards Embedded's
executor in http::run_http, so changing stdio alone does not satisfy this plan.

Endpoint::dispatch holds the lives mutex and SerializedWriter::with_store
across the entire handle_request invocation, including SessionIo backed by
MailboxReader. That reader can wait on a per-session mailbox for 50 ms per
observation. A client elicitation therefore retains both shared ownership
locks while waiting, contrary to the required independent host and session
isolation. Timer-driven execution cannot be added under this same lock without
letting a client wait suspend the execution owner and other eligible calls.

Plan 8901 requires typed owned command admission with 32 commands/32 MiB per
host and 8 commands/16 MiB per session, charging retained parsed payloads and
wire copies, stable session generations, per-session FIFO and independent
control capacity. Replacing the mutex with an unbounded channel or merely
moving the same blocking callback to an owner thread does not satisfy those
requirements. State responses and durable-change markers must capture the
same committed prefix before the next operation is observed.

Production HTTP must retain one native host across sessions; client teardown
cannot call the process-scoped stdio EOF-to-abort path. Session cancellation,
elicitation and notification routing must return to their bounded session
channels without retaining Store or host ownership. Stop belongs to the actual
native host and its original deadline, helper retirement and proof-based
settlement, not to each POST or SSE stream. Installed native nonempty lifecycle
acceptance remains a prerequisite for final transport acceptance.

This is source-grounded prerequisite review; it does not mark plan 20 tasks
started or completed, and real quiet-client stdio/HTTP handler, retry, deadline,
compensation and concurrent eligible-request tests remain required.
