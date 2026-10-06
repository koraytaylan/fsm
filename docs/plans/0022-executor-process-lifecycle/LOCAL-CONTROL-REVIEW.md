# Local control transport review

The next integration unit is the owner-only exact-incarnation Unix control
endpoint and downstream `execute stop` caller, following the passing owned
session runtime 3157f1c; current production selectors remain unchanged.

The task-cache server draft derives physical device/inode identity from the
actual driver's original writer, generates a 256-bit kernel-random incarnation,
requires an existing owner-only root and creates a private directory and socket.
Its request parser checks the closed schema and finite bounds before invoking
the actual cloned control, and its absolute frame deadline prevents a client
from renewing the read budget by trickling bytes; metadata waiting opens no
Store and cannot contend for the journal writer.

Review found three blocking implementation defects before source integration:

- Four accepted drain requests can occupy all four response workers for the
  full original shutdown deadline, causing a later abort to be refused by the
  connection cap; parsing/admission capacity must be independent of outstanding
  response waits, with bounded retained response state and no unbounded threads.
- The draft does not publish authenticated discovery metadata containing its
  incarnation, so its generated secret cannot yet be obtained by the intended
  downstream caller; discovery must validate owner, mode, original physical
  identity, schema and ambiguity before any request is transmitted.
- Partial publication leaves its private directory/socket behind, and close
  only stops listener admission; cleanup must verify captured inode identities
  before removing this exact endpoint and must expose bounded uncertainty
  rather than join a potentially stalled filesystem operation.

Additionally, the caller's finite budget must include filesystem discovery and
Unix connect itself, which can block before stream timeouts can be installed;
a bounded caller worker must check its original deadline again before writing
after a delayed connect, and a missing response must never fabricate admission
closure, native termination or writer release.

The draft is rustfmt-parsed only, unapplied and uncompiled, and does not count
as an implemented endpoint; load-bearing downstream tests must exercise stale
identity, closed-schema and exact request byte limits, silent/trickling clients,
repeated drain then abort, ambiguous executors and actual held writer/control
response independence before the unit is committed as a capability.

Task 9402 stays planned, task 9401 stays in progress, and plan progress stays
3/7; installed native tree, production quiet-stdin/blocked-stdout, signal and
paired standalone acceptance remain required.


## Implemented opt-in endpoint and client follow-up

The replacement uses one nonblocking transport loop with at most 64 retained
connections and eight accepts per iteration; independent request parsing and
metadata polling replace waiting response workers. Saturation evicts an old
response without clearing its accepted lifecycle request, so an abort after
72 long drains still reaches the actual control and preserves the first deadline.
Silent clients and incomplete frames have an absolute 250 ms input budget;
output frames have a separate 250 ms delivery budget. Request/response ceilings
are 1024 bytes/128 KiB excluding LF, with capped retained connection storage.

Publication now writes a private closed-schema incarnation/physical-store
identity file; discovery validates owner, permissions, schema, socket type,
identity and ambiguity. The client bounds discovery, connect and transport in
one worker under its original caller deadline, checks before late transmission,
and strictly validates the delivered report identity and types. At most eight
client workers are charged per process, retaining the charge until actual worker
retirement even after the caller times out. No transport error confirms native
cleanup, and the report grants no claim-reuse authority.

Explicit close requests listener admission closure and waits only to its finite
bound for asynchronous exact captured-inode cleanup; replacement files remain
untouched. Partial publication failures retain the same scoped cleanup guard.
Drop only requests transport closure and promises no native cleanup or delivery.
The endpoint uses the actual driver's cloned control and never opens the journal
or runs native closure in its transport loop. Host polling remains necessary.

Focused stable/MSRV all-target CLI Clippy and twelve actual Unix transport tests
passed in session 34926 under asserted MemoryMax=1G/MemorySwapMax=0; the retained
log is local-control-check-v4.log. Initial session 75002 failed because the draft
used JsonLimits::default instead of the existing JsonLimits::DEFAULT constant;
the corrected checks passed. Mutation session 27776 exited 0: individually
neutralizing the request byte bound, absolute input deadline, closed schema,
incarnation match and abort-capacity eviction caused their downstream tests to
fail with exit 101, and restored source passed all twelve tests. Separate logs
local-control-mutation-*.log and local-control-mutations-restored.log are retained.

These are actual sockets and durable writer locks with empty native inventories;
no native tree completion, closure receipt or installed production acceptance
is inferred. The public library endpoint/client are opt-in; execute stop CLI,
production publication/selection, signals, installed tree and paired standalone
integration still remain, and the full changed-source stable gate is required.
Task states and plan progress remain unchanged at 3/7.


Final follow-up shares one bounded inventory snapshot per transport iteration
rather than copying every unresolved ID list separately for each waiting drain.
After correcting the snapshot method signature, final focused session 4656
passed stable/MSRV all-target Clippy and twelve transport tests; the retained
log is local-control-check-final-v3.log. The expanded refusal test also covers
stale physical device and inode. Mutation session 61319 repeated all five
expected downstream failures against this final source and passed restored
tests; local-control-mutations-final.log records the terminal passing run.


Full local control endpoint stable host gate session 47222 completed with exit
zero against exact runtime 9d2439f21e5a3f1d8f51a7f3cbfaddaf8f4bcae6, under
verified MemoryMax=1G and MemorySwapMax=0; retained
local-control-endpoint-stable-gate.log covers formatting, source size, debug
and release workspace tests, all-target Clippy, warning-free documentation,
zero dependencies and embed acceptance. The documentation-only afb7f2b
review did not change that frozen runtime. Portable/installed native and
production selector acceptance remain unproved; task states remain unchanged.
