# Owned native session composition review

## Ownership and command boundary

Production Linux embedded stdio constructs the retained native execution host.
The host owns the sole durable Store, original lifecycle driver and injected
clock; session handles submit owned commands without borrowing the writer.
The owner captures immutable results, committed sequence and publication
interval before the next operation. Session input, output and user interaction
remain outside that Store boundary.

The ownership acceptance harness selects the same native owner on Linux and
asserts actual shutdown inventory, helper retirement and writer release on exit.
Other platforms exercise the writer-only command boundary without claiming
native execution. Public borrowed APIs retain their separately specified input
bounds and lifecycle contracts.

Primary source owners are `mcp/host/`, `mcp/serve/hosted.rs`,
`mcp/serve/owned.rs`, `mcp/owned_input.rs`, and `local_control/` under
`crates/fsm-cli/src/`, plus the original executor lifecycle driver under
`crates/fsm-execute/src/service/lifecycle/`.

## Admission and session isolation

Application admission is bounded to 32 commands / 32 MiB per host and
8 commands / 16 MiB per original session generation. Charges include retained
payload capacities and the RPC cancellation-control copy, remain held after
dequeue, and retire with the original reservation. Separately reserved,
coalesced stop and generation-scoped cancel/close controls remain available at
application saturation; unknown IDs cannot install future cancellation.

Busy admission precedes Store dispatch. Stop rejects queued work, finishes an
already executing complete operation and rejects later admission. Closing an
original generation suppresses its response and cannot route a replacement
session's response to its old receiver. Lost response delivery cannot reverse
a committed operation or replace its ordinary idempotency key/fingerprint.

## Lifecycle and output separation

Quiet native observation retains the original driver. Monotonic waits do not
supply journal time; one injected clock sample supplies each native decision or
shutdown-observation pass. Original stop requests preserve their first deadline
and escalation; owner exit is never itself evidence of stopped native work.
Shutdown returns the retained driver and actual report, including uncertainty.

Protocol output and operator diagnostics have separate bounded workers and
explicit drainage/loss facts. Their blocked writes must not retain the durable
writer after proven native retirement. Session teardown observes original
shutdown bounds rather than creating a new output deadline. Final native-error
delivery preserves the initiating error and actual delivered bytes, and does
not turn uncertain cleanup into success.

Held-I/O and empty-inventory tests establish their actual scopes; they do not
prove closure of a nonempty native process tree. Genuine provisioned workflow
and contention acceptance is described in [WORKFLOW-NATIVE-REVIEW.md](WORKFLOW-NATIVE-REVIEW.md).
Failed native authorities and namespaces retain their exact identities until
matching retirement is established; fixture observations never fabricate
production receipts.

## Transport boundary and remaining obligations

Stdio process EOF may request host shutdown. An HTTP request or session ending
must retire only that session, preserving the shared host and other accepted
workflows. HTTP currently lacks the required retained autonomous host and remains
an explicit task, rather than inheriting stdio acceptance.

The command-owner inventory is frozen in plan 0020 task 8901. Completion fairness,
scheduling, session-channel ordering and full stdio/HTTP acceptance retain their
own inventories. Plan 0022 additionally requires launch/settlement crash recovery,
changed-handler/late-result exclusion and its full shutdown matrix; plan 0023
requires sustained and live-client operational evidence. No empty-inventory,
focused test or workflow verdict substitutes for these remaining requirements.

## Frozen ownership evidence

At `26c682203df56c498132ab3881b48bd04cdbb46f`, all thirteen named ownership
acceptance tests passed in Linux debug and release on stable and MSRV; all twelve
applicable cases passed on macOS in both profiles and toolchains. The thirteenth
case is explicitly Linux-only and proves native idle ownership and writer
release. Both native matrices and all twelve provisioned workflow scenarios
also passed at that exact source.

The independent ownership review, named test occurrences and successful Linux
gate stages are preserved outside the repository under digest
`bc05b865c0725b896ca520c39c8816b2af0258c4f51c855f7c6e1641f3c64b72`;
macOS evidence has digest
`f42b2b86b2d021b7e02d6aa982797463d6a16184fa58d70c7974775cab86e675`.
Completion is recorded only by authoritative task frontmatter and STATUS.

## Historical evidence

The exact prior review is retained in task-cache `plan-status-archives`,
addressed by SHA-256:
`5ca997bd9e08211fbeb1b4b404aa5c54587967365a745cfb8ad1570075018949`.
Volatile sessions, PIDs and intermediate logs remain outside the repository.
