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

## Implemented idle/closure correction

The mailbox is extracted into http/mailbox.rs while retaining its existing
endpoint exports. A quiet poll returns WouldBlock, which reverse waiting handles
by returning to the original elicitation deadline check; closed mailboxes yield
EOF. Session DELETE closes/wakes the original mailbox before acquiring lives,
clears queued replies and refuses late posts. VecDeque replaces front-removal
shifting, but mailbox admission remains unbounded and is not claimed complete.
The new reverse-wait regression uses the actual Notifier/SessionIo/mailbox path
and replies after 150 ms, across multiple 50 ms polls; a separate case proves
idle versus closure and late-post refusal. This is not full HTTP streaming proof.

Initial session 41047 was stopped by the exact test PID after the old HTTP
isolation test's 1 ms fake-clock increments caused a prolonged wait; no passing
aggregate is claimed. That case now uses an injected timeout-sized clock step
and preserves its cross-session assertion. Terminal corrected session 90623
exits zero: stable/MSRV all-target CLI Clippy, both new mailbox cases, nine
elicitation tools, eleven HTTP POST cases, nine session cases and the actual
HTTP notification case pass, with formatting/file-size checks. The session-ID
subprocess-only test retains its existing ignore marker and is driven by its
existing fallback test. Controller kernel limits are asserted one GiB/zero swap.
http-mailbox-idle-corrected-check.log SHA-256 is
c0b0a18ec71335b7b176f759772c6dc0f95a3cfbc297c9783792c67d7a0ada6d.
SPEC/API-POLICY/EMBEDDING/RELEASE describe the exact correction and remaining
limitations; full host/platform gates and guard sensitivity remain pending.

## Implemented reverse-response queue admission

Production response POST now uses explicit mailbox admission: at most 64
queued values and 32 MiB of charged payload storage per original mailbox.
The charge includes Value storage, owned string/array capacities, six bytes
per string/key byte for worst-case escaping and a conservative 4096-byte
object-entry allowance; recursive depth over 32 refuses. Saturating arithmetic
cannot wrap into acceptance. Queue slots remain separately count-bounded.
Dequeue releases the queue's charge while transferring payload ownership to
the existing reader; this does not account the complete host/output lifetime.
Count/byte exhaustion returns HTTP 503 without enqueueing or touching Store;
closed admission returns 404. Close remains independent of queue saturation.
Existing direct Mailbox::post callers close their mailbox on failed admission
rather than silently dropping an unanswered response; HTTP uses explicit
try_post results and preserves previously admitted values on overload.

Terminal session 60211 exits zero under asserted kernel one-GiB/zero-swap
limits: stable/MSRV all-target CLI Clippy, four mailbox cases, twelve HTTP
POST cases, nine session cases and nine elicitation tool cases pass, with
formatting/file-size/diff checks. The new actual endpoint case admits exactly
64 responses, rejects the next with 503, verifies unchanged original journal
records and successfully DELETEs the saturated session. The charged-byte
case accepts exactly 32 MiB including spare owned String capacity, rejects
limit-plus-one, and proves release on dequeue/close. Initial session 10097
failed compilation because the test moved records out of a Drop store; the
corrected test clones its small original journal snapshot.
http-mailbox-admission-corrected-check.log SHA-256 is
1b18e231f0325e53ddca7fc055585f37eeebb169e8580007a31fef2a752afe75.
Capability docs move with this transport policy. Guard-neutralization proof,
renewed full host/platform gates, full allocation lifetime, reverse streaming
and actual HTTP executor ownership remain outstanding; no task promotion.

### Admission guard sensitivity

Terminal session 57875 exits zero: neutralizing only the count predicate makes
the actual endpoint overload regression fail with HTTP 202 instead of 503;
neutralizing only the charged-byte predicate makes the exact byte-boundary
case fail because admission unexpectedly succeeds. Each negative run exits
101 and matches the intended assertion, rather than a compilation failure.
The original mailbox source is restored in finally after each mutation and
verified byte-for-byte, SHA-256
a8a80e596921aef826d50d61dbd9faea14c2b9fe03a748c1a5868bc545383d42.
Healthy actual endpoint and all four mailbox cases then pass on stable/MSRV;
tracked source is clean afterward. The controller verifies actual one-GiB/
zero-swap limits before any builds. Sensitivity log SHA-256 is
d5493ae7e3c1e4d47104fb2c32fbfe18b6367707eaee787c6230bd37eb403305
under http-mailbox-admission-sensitivity.log in the explicit task cache.
This establishes those two guards, not HTTP streaming or execution ownership;
the original full-plan scope and outstanding acceptance gates remain unchanged.

## Renewed full stable host gate

Terminal session 75429 exits zero against frozen source
14142897c7055e3b7555928a3270420cc746625c, with tracked source unchanged
after each stage and at terminal observation. All eight required stages pass:
formatting, file-size limits, debug and release workspace tests, all-target
workspace Clippy with warnings denied, warning-free workspace documentation,
CLI zero-dependency checks and embed acceptance. The controller asserts actual
kernel one-GiB/zero-swap limits and runs builds/tests serially with one worker;
all artifacts use the explicit task cache. The complete log
http-mailbox-full-stable-gate.log has SHA-256
87af71723a220e6263e2d16a9770adef1ef77dbe703ac1bf9591de35b6ca2493.
This renews Linux stable host acceptance for the committed mailbox changes;
it does not replace native platform CI, the official authority matrix or the
outstanding HTTP ownership and autonomous execution acceptance inventories.
Task 9401 remains in progress and plans 20–23 remain incomplete.
