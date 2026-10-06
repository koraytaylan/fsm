# Owned native driver implementation review

Reviewed against the shared admission fence, original completion pass,
claim-bound closure, authenticated interruption and exact helper retirement.

The driver owns the original Store, Watcher, Scheduler and Runner; a cloned
control holds only the shared admission fence and request/report metadata.
No metadata mutex spans Store Drop, journal mutation, native request startup,
receipt authentication or protocol I/O. Invalid bounds refuse before fence
closure; repeated requests preserve their first deadline and never downgrade
abort. A held worker cannot hold control wait past that deadline.

The driver closes only locally admitted original claims. It cancels foreign
client helpers without closing foreign domains. Four retained closure helpers
and four startup attempts per poll bound both successful and failed targets;
a rotating run-id cursor prevents a failed low run-id from starving others.
Preclaim uncertainty remains charged through the complete admission inventory.

A closure receipt arriving before an execution response is not authority to
invent interruption: the driver observes execution again and waits for actual
execution reap and both EOFs before concluding no original completion exists.
A retained authentic completion stays on its original policy path. Interrupted
settlement uses the authenticated original claim and releases capacity only
through the existing separate helper retirement guard. No outcome event or Ack
is invented, and pending effects remain pending.

Stopped publication requires successful original physical-store observation,
no retained local claims, no preclaim reservations, all owned helpers retired,
no retained closure helpers, and actual Store Drop before publication. Driver
Drop cannot publish Stopped. Incomplete observation and missing native proof
retain uncertainty, ownership and writer rather than manufacture success.

Verification: focused stable session 97397 exited 0 under asserted 1 GiB RAM
and zero-swap limits; stable/MSRV all-target executor Clippy, executor library
tests, two downstream actual-writer/control tests, sixteen public surface tests
and source-size checks passed, with local-owned-lifecycle-focused-v3.log
retained. Earlier compile/lint failures were corrected before this pass.
The API inventory recognizes the driver and reexports; nested control fields
are additionally exercised by the downstream integration tests.

This is an explicitly driven opt-in library implementation. Installed native
bound/executing driver controls have not executed here; production quiet stdin,
blocked stdout, independent lifecycle worker, owner-only exact-incarnation
control endpoint, CLI stop and signal integration remain unfinished. Tasks
9401 and 9402 are not completed and the plan stays 3/7. Full stable host gate
for this changed source is required next, followed by installed native and
portable CI review once the exact private upload is authorized.

## Follow-up finding: admission closure is not a stop request

Review of frozen runtime f90871b found ExecutorControl::requested reads only
the shared admission fence. A host can close that fence before transferring its
runner into from_owned_parts without issuing any lifecycle request. A successful
empty poll then drops the writer while request metadata still says Running;
record_stopped correctly refuses, but the writer was released prematurely.
This distinction must be fixed before accepting the driver implementation.

The prepared correction checks actual request metadata, preserving direct
admission closure as its own irreversible safety operation. A downstream
regression transfers an actually preclosed native runner, polls an empty owned
Store and requires its writer to remain held until an explicit Abort request.
Draft code and regression are retained as owned-driver-request-distinction-*.rs.draft
in the task cache; they have been formatted but not applied or executed while
full stable session 85037 remains live against frozen f90871b.

The next installed control is also drafted as owned-driver-supervisor-probe,
owned-driver-root-assertions, owned-driver-host-cases and owned-driver-root-matrix
cache files. It adds independent process and MCP bound-owner driver axes while
retaining existing primitive closure/interruption tests, authenticates actual
Root closure through the real driver, proves writer reopen after Stopped and
checks exactly two interruption records, unchanged instance/pending state, no
Ack and exact cold replay. These drafts are not executed native evidence.

## Applied request distinction and live-bound driver controls

The admission/request distinction is corrected: the driver checks actual
request metadata before authorizing shutdown writer release. Focused session
18750 exited 0 with stable/MSRV all-target executor Clippy, 46 library tests,
three downstream lifecycle tests and sixteen public surface tests. Load-bearing
mutation session 91927 exited 0: restoring only the old predicate produced
exit 101 at the new actual writer-retention assertion, and restoration of the
correction passed all three downstream tests; both mutation/restoration logs
are retained. These checks ran with asserted 1 GiB RAM and zero swap.

Source-reviewed defective runtime f90871b full stable session 85037 was
intentionally stopped in its exact owned systemd scope, then confirmed terminal
143 before source mutation; its retained partial log is not a passing full gate.
This avoids finishing acceptance of a known defective unit or overlapping jobs.

The new provisioned owned driver control now starts from an actual live bound
original native owner with no closing/closed/closure/entry artifacts; it does
not pre-close the domain through a primitive fixture. The driver itself must
request and authenticate Root closure, settle interruption, retire execution
and closure helpers, release its original writer and publish Stopped. Cold
writer reopen must show exactly two original interruption records, unchanged
instance/pending state, no Ack and exact replay. Independent Root assertions
require the protected original receipt, absence of every protected entry
artifact, removal of the original cgroup, and preservation of the successor
cgroup with allocation counter still two. Both process and MCP bound-owner
axes are added to the existing provisioned broker matrix; prior primitive
closure/interruption cases remain present.

Live-bound probe session 11667 exited 0 with stable/MSRV all-target compilation
and all three downstream lifecycle tests; installed native execution is still
unexecuted on this host and cannot be inferred from compilation. These controls
do not cover executing/hung trees, independent production stdio progress, local
owner-only endpoint, CLI stop or signals. The corrected combined source still
requires its full stable host gate and later exact-source portable/native CI.
No task is completed or promoted and plan progress remains 3/7.
