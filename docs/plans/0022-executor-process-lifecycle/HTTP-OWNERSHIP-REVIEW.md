# HTTP ownership implementation review

Reviewed source: 7189bfc, before transport implementation.
Task 9401 remains in progress; no HTTP native ownership acceptance is claimed.

## Current production boundary

`http::run_http` consumes ServeMode only to select writable/read-only Store
opening, then constructs Endpoint without retaining the embedded executor.
Endpoint::request holds the entire lives map and SerializedWriter mutex across
handle_request. A client interaction therefore excludes every other session
from store dispatch and session cancellation processing.

The extracted elicitation preparation/settlement values are necessary but
insufficient to correct this path: Endpoint's Notifier writes to a SharedSink,
and begin_stream/replay occurs only after handle_request returns. An actual
client cannot receive elicitation/create before the handler starts waiting for
its answer. A preloaded mailbox or synthetic answer cannot prove the real
reverse-request exchange.

MailboxReader::fill_buf waits 50 ms, then returns an empty slice on a quiet
poll. SessionIo::read_line classifies that slice as EOF; request_and_await thus
cannot distinguish a still-connected quiet HTTP session from disconnection.
Mailbox itself retains an unbounded Vec of responses and has no closed state.
Fixing only the writer lock would preserve these protocol and allocation defects.

## Required next implementation boundary

The private owner must receive typed owned store operations, including
elicitation preparation and settlement, and return immutable response data;
transports never retain a Store borrow while waiting. The prepared contract,
original request key and accepted result survive the interaction, while actual
settlement revalidates current store state through ordinary event dispatch.
The host retains embedded native executor state and polls admission/completions
independently of HTTP input, using the plan 0020 count/byte/session-generation
limits and reserved controls; a per-request timer beside SerializedWriter is
insufficient.

Per-session reverse delivery must publish and flush the elicitation request
before waiting, through the actual bounded HTTP stream or session output path.
Mailbox state needs distinct idle, response and closed observations, finite
count/byte accounting and independent cancel/close/stop access. Idle must yield
to deadline/control checks without becoming EOF or an infinite inner reader
loop. Expired session generations must neither receive a newer session's reply
nor settle an accepted answer into a replacement session.

## Production-facing proof obligations

- An actual endpoint client receives the reverse request, waits longer than
  one mailbox poll, and then supplies the answer successfully.
- Another session completes a genuine read and mutation during that wait;
  cancel, session DELETE and host stop remain independently accessible.
- A competing mutation changes the event's eligibility while the form is
  open; settlement respects current state and original idempotency identity.
- Saturated request/output/mailbox queues retain the specified controls,
  reject over-capacity allocations before journal dispatch and do not silently
  drop completion or reverse-response ownership.
- Quiet input differs from disconnect; every stopped/expired-session path
  observes its original bound and never fabricates successful native cleanup.
- Genuine provisioned HTTP process/MCP execution shares one original writer
  and native owner with correct read-only/unavailable-authority refusals.

Existing http_multi_client proves concurrent ordinary writes and coherent
journal replay, but it does not cover an outstanding reverse request or native
execution ownership; passing it alone cannot establish this boundary.
This review adds no runtime capability; implementation and runtime proof remain
open, with the capability documentation and full gates required when wired.
