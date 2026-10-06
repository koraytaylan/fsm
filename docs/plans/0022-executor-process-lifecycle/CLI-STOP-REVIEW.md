# Execute stop integration review

The reviewed endpoint runtime is 9d2439f; its full stable host gate remains
the existing session 47222, with source frozen until terminal verification.
The execute-stop.rs.draft and execute-stop-cli-tests.rs.draft in the dedicated
task cache are rustfmt-parsed, unapplied and uncompiled.

The command must register the longer `execute stop` path independently of the
existing `execute` command, using the existing longest-prefix dispatcher;
`--mode drain|abort` and finite `--timeout-ms` are mandatory and must refuse
before transmission. Global `--data-dir` resolution remains the existing CLI
behavior. Stop must not load a handler table, open Store, take the writer lock,
create a missing data directory or fall through to the ordinary execute loop.

The intended lookup root is HOME/.cache/fsm/control, with explicit
`--control-dir` for hosts publishing elsewhere; stop only reads that root and
does not create it. This is the same owner-only discovery mechanism as the
library endpoint, not a PID lookup or an absence-based cleanup heuristic.
Multiple matching endpoints remain an error rather than stopping all domains.

The output contract uses the existing CLI renderer and existing error codes:

- A strictly validated actual `stopped` report is emitted on stdout with exit
  zero; the transport validator also requires actual writer/helper retirement,
  complete empty ownership inventory and closed admission for this phase.
- An actual `uncertain` report is retained in `exec/inflight_deferred` details,
  emitted on stderr with exit one, preserving its native ownership/writer facts.
- Transport failure uses the same existing error code but explicitly null
  admission/writer/native-cleanup fields; it cannot imply that a missing endpoint
  or lost response means the original executor stopped.
- Invalid arguments use the existing args error and exit two; non-Linux hosts
  return the existing unsupported exec/mode error without native control I/O.

The client's finite budget includes filesystem discovery, connect and response
transport; the server retains the first accepted lifecycle deadline, so a later
CLI abort may receive an earlier request's uncertainty without deadline renewal.
Ordinary CLI rendering is synchronous and distinct from that transport bound;
it must not be confused with the embedded server's independently queued output.

Prepared real-binary tests drive the production argument dispatcher against an
actual library-published endpoint and durable writer: stopped follows owner poll
and cold reopen, an earlier finite request returns actual uncertainty while the
writer stays held, invalid arguments preserve admission, and missing discovery
creates no data directory and yields only unknown cleanup facts. Each test child
has an observation deadline and is killed/reaped if a regression prevents exit.
These tests do not substitute for a production server publishing its endpoint,
installed native trees, blocked server stdout, signals or paired standalone
writer strategy; those acceptance requirements remain pending.

Implementation and code-facing docs must land together after the current gate;
task 9401 remains in progress, 9402 stays planned and plan progress stays 3/7.


## Implemented stop command follow-up

The actual production argument dispatcher now registers execute stop separately
from execute, validates explicit drain/abort and finite timeout before any
transmission, and uses private exact-incarnation discovery without opening Store.
The operator can choose --control-dir for an explicit library publisher; the
default is HOME/.cache/fsm/control, which this read-only command does not create.
Stopped reports use stdout/exit zero; uncertainty uses the existing
exec/inflight_deferred stderr error with the actual report in details, and
transport failure preserves null unknown facts rather than claiming admission
closure or native cleanup. Unsupported platforms use the existing exec/mode.
Current executor selectors and production publication remain unchanged.

Focused session 44567 exited zero under asserted MemoryMax=1G/MemorySwapMax=0,
passing stable/MSRV all-target CLI Clippy, four real-binary tests (including
both modes, exact maximum and maximum-plus-one), twelve transport tests, nine
argument tests and 77 review regressions; execute-stop-check-v3.log is retained.
The initial script named a nonexistent registry test target after its other
checks passed; it was corrected to the actual review_regressions suite.
Mutation session 89989 exited zero: disabling mode refusal or allowing the
exact maximum-plus-one caused the production-binary guard test to fail with
exit 101, and restored source passed all four binary tests; retained logs are
execute-stop-mutation-mode.log, execute-stop-mutation-maximum.log and
execute-stop-mutations-restored.log.

These tests control a real library-published endpoint with empty native inventory
and an actual durable writer, including a held-writer uncertainty response;
they do not prove native tree/production server publication, signals or paired
standalone ownership. Full changed-source stable gate and portable/installed
acceptance remain required; task 9401 stays in progress, 9402 stays planned and
plan progress stays 3/7.
