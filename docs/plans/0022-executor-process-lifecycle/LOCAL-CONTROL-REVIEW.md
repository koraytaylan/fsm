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
