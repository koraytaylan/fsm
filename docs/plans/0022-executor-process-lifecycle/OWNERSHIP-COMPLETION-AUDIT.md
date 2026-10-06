# Task 9401 completion audit — 2026-10-06

Completion is unproven at 941c4f7. This audit uses the original five steps
and test requirements, not a reduced definition of ownership integration.

| Requirement | Current evidence | Remaining proof |
| --- | --- | --- |
| Durable claim and writer recheck before launch | Native selected admission and bound-entry provisioned controls passed at 665a71b; preparation/ownership separation is implemented | Actual production constructor selection remains legacy; installed standalone and embedded happy paths required before selection changes |
| Original identity and contract through recovery | Published claim anchors, physical store/authority checks and original contract reconstruction implemented; earlier provisioned removed-table recovery passes | Cold event-only implementation and named controls are later source and have no installed execution evidence |
| Verified stopped disposition, retry/ack and ordered outcome event | Atomic format-12 handoffs and historical migration/checkpoint fixtures implemented; prior native matrix passed | New cold two-seal, rejected-key and conflicting-key process/MCP controls must execute at their exact source |
| Shared standalone/embedded/public startup sequence | Public selected tick controls have native evidence; common original-owner recovery implemented | Production standalone/standalone and standalone/embedded races with independent external tree markers remain required; public control tests do not prove production routing |
| Bounded uncertainty health with no secret disclosure | Current ownership retention/refusal diagnostics exist | Additive durable prefix counts are now implemented and locally tested; actual live lifecycle health, API inventory and actionable startup uncertainty inspection still require completion |

The original crash matrix still requires crashes after claim, launch,
verified stop, stopped record, attempt/ack and before outcome event, with
sequential retry or recovery and verified journals; stopped-result versus
second-executor races must prove single consumption and original backoff.
Existing format/replay tests cannot replace those actual production trees.
Writer-held timeout controls must preserve durable ownership and settle once
available, and old-incarnation results/removed tables must never authorize a
conflicting run. Installed controls cannot be accepted from compilation.

The complete stable gate at f5ba23e is local evidence only; the later named
case refactor has stable/MSRV all-target Clippy and 28 library-test passes.
Terminal review 37460020929 proves all nine jobs at 665a71b, predating cold
host code. All production/gate/executable-byte acceptance flags stay false.
Remote successor push was rejected by automatic approval review and exact
authorization is pending; no indirect upload or production-default workaround
is authorized by this audit. Task 9401 remains in progress, and its dependent
shutdown/reconciliation/crash tasks and plans 20, 21 and 23 remain incomplete.
