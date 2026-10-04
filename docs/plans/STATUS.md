# Plans — roll-up board

One row per plan. Task status is authored in each plan's `tasks/*.md` frontmatter and summarized by its `STATUS.md`.

| Plan | Title | Status | Tasks | Outcome | Status doc |
|---|---|---|---|---|---|
| 0001 | Foundations & Walking Skeleton | ✅ Complete | 13/13 | A zero-dependency two-crate workspace whose JSON, SHA-256, and decimal foundations are vector-tested, with `fsm serve` completing a byte-exact MCP initialize/ping/tools handshake. | [status](0001-foundations-walking-skeleton/STATUS.md) |
| 0002 | Expression Language | ✅ Complete | 7/7 | Guards, actions, and invariants parse, typecheck, and evaluate as a total, step-budgeted, exactly-typed expression language with three-valued partial evaluation and span-precise errors. | [status](0002-expression-language/STATUS.md) |
| 0003 | Statechart Engine | ✅ Complete | 17/17 | Hierarchical machine definitions validate, compile, and execute through a pure `step()` with LCA exit/entry pipelines, history, explain traces, simulation, and static analysis, proven against a naive oracle interpreter. (Regions and explicit deadlines, which the plan reserved for a later version, landed under this plan's umbrella pre-tag.) | [status](0003-statechart-engine/STATUS.md) |
| 0004 | Journal & Store | ✅ Complete | 10/10 | Every mutation commits through a hash-chained fsync'd journal that recovers, verifies, and replays bit-identically, surviving a kill -9 crash harness. | [status](0004-journal-and-store/STATUS.md) |
| 0005 | Command-Line Interface | ✅ Complete | 8/8 | The full `fsm` command tree drives authoring, execution, diagnosis, and audit ad hoc, with `--json` output byte-identical to the future MCP structured results. | [status](0005-cli/STATUS.md) |
| 0006 | MCP Server | ✅ Complete | 8/8 | A complete tool surface with resources, prompt, and instructions passes byte-exact golden transcripts and a naive-caller error-recovery suite. (The planned 13 tools shipped, plus `deadline_poll` — 14 — added with the deadlines feature.) | [status](0006-mcp-server/STATUS.md) |
| 0007 | Hardening, Examples & Docs | ✅ Complete | 8/8 | Fuzzing, chaos, and determinism suites guard the engine; worked example machines, the completed SPEC, and the README shipped the initial v0.1.0 release. | [status](0007-hardening-examples-docs/STATUS.md) |
| 0008 | Effect Executor | ✅ Complete | 13/13 | A standalone `fsm execute` process watches the effect outbox, runs operator-configured handlers as subprocesses, acknowledges outcomes into the journal, and polls due deadlines — so a triggered workflow proceeds gate-to-gate unattended, resumable after a kill at any instant, with the engine's semantics untouched. | [status](0008-effect-executor/STATUS.md) |
| 0009 | Reactive Semantics | ✅ Complete | 20/20 | A bounded run-to-completion macrostep — eventless transitions, internal events raised from blocks, and generated done events for finished compounds and regions — so a machine reacts to itself between external events, sealed in one atomic record, with non-reactive definitions producing byte-identical bytes. | [status](0009-reactive-semantics/STATUS.md) |
| 0010 | Machine Composition | ✅ Complete | 14/14 | A state invokes another machine by content hash, waits for it, and reads its result through `$done.invoke.<slot>`; one instance signals exactly one other; and every edge between instances is a journal record rather than an inference. | [status](0010-machine-composition/STATUS.md) |
| 0011 | Definition Evolution | ✅ Complete | 11/11 | An explicit, journaled, idempotent migration moves a running instance onto a corrected definition under a mapping the new definition declares, with a preview that answers "what would this do" before anything is written. | [status](0011-definition-evolution/STATUS.md) |
| 0012 | Live MCP Surface | ✅ Complete | 14/14 | The server can speak first: instances addressable as resources, subscriptions that push updates when a workflow advances, list-changed notifications, structured logging, progress on long calls, and cancellation that is honest about what it can interrupt. | [status](0012-live-mcp-surface/STATUS.md) |
| 0013 | MCP Affordances | ✅ Complete | 10/10 | Derived tool annotations and titles so a host can auto-approve reads and gate writes, completion of machine ids, instance ids, and enabled event names, and an elicitation path that turns a machine's typed event fields into a form a person can fill in. | [status](0013-mcp-affordances/STATUS.md) |
| 0014 | Audit Surface | ✅ Complete | 9/9 | The five CLI-only audit capabilities — explain, verify, replay, doctor, annotate — reach the MCP surface, and a server whose store will not open still starts so the diagnostic tools are available when they are needed. | [status](0014-audit-surface/STATUS.md) |
| 0015 | Transport And Multi-Client | ✅ Complete | 14/14 | A hand-rolled Streamable HTTP transport with sessions, server-sent events, and an honest loopback-first security boundary, so one process serves many clients against one store — and a contended `serve` degrades instead of exiting. | [status](0015-transport-and-multi-client/STATUS.md) |
| 0016 | Executor Policy | ✅ Complete | 12/12 | Journaled retries with deterministic backoff, exhaustion that still fires the machine's failure path, bounded concurrency with per-instance fairness, and a handler kind that calls another MCP server's tool. | [status](0016-executor-policy/STATUS.md) |
| 0017 | Journal Lifecycle | ✅ Complete | 16/16 | An operator seals a prefix of the journal into an immutable archive and detaches it, so disk and open cost track the retention window instead of the store's lifetime — cutting at a segment boundary the pin allows, carrying request fingerprints and the record-derived indexes under two new additive domains so no historical root moves, and refusing any cut that would weaken idempotency rather than degrading it. | [status](0017-journal-lifecycle/STATUS.md) |
| 0018 | Machine Cases | ✅ Complete | 7/7 | A machine's expected behaviour is committed beside it and falsified by a change, run by a pure scripted runner that sends, polls, and acknowledges without opening a store — and a definition that declares `supersedes` is checked against the cases of the machine it replaces, reported as a delta rather than gated. | [status](0018-machine-cases/STATUS.md) |
| 0019 | Consolidation | ✅ Complete | 4/4 | The committed gate widens to `--all-targets` and passes there, the workspace's one performance signal becomes a guard with a measured ceiling, and `fsm-execute`'s provisional surface is enumerated so an addition is a decision. Lands before 0017. | [status](0019-consolidation/STATUS.md) |
| 0020 | Autonomous Embedded Execution | Unregistered | 0/7 | Planned: one bounded writer host advances effects, retries and deadlines without client polling, with responsive stdio and HTTP sessions. | [status](0020-autonomous-embedded-execution/STATUS.md) |
| 0021 | Executor Contract Preflight | Unregistered | 0/6 | Bounded effect analysis and manual policy landed independently in `f0a489a` with reviewed Linux gates; outcome validation landed independently in `aa6fc90` with reviewed MSRV/Linux gates; CLI checks landed independently in `04dbb17` with seven reviewed MSRV/stable tests; shared execution admission and MCP checks remain incomplete. | [status](0021-executor-contract-preflight/STATUS.md) |
| 0022 | Executor Process Lifecycle | Unregistered | 0/7 | Feasibility investigation: Linux MSRV/stable negative probes confirm surviving descendants and retained pipes; backend decision and positive native proofs remain pending. | [status](0022-executor-process-lifecycle/STATUS.md) |
| 0023 | Operational Acceptance | Unregistered | 0/7 | Evidence reporter verified on its exact commit; portable fixtures landed in `7c8c49c` and passed the full Linux consumer-install suite. Independent observer landed in `9903ef3` with 13 reviewed fault/ledger tests; traced external fixtures landed in `20c67d5` with nine subprocess tests; independent MCP fixture in `fb4291f` adds seven reviewed tests (45 harness tests total). Integrated executor, sustained native and live-model evidence remain incomplete. | [status](0023-operational-acceptance/STATUS.md) |

## Review follow-up: plans 0020–0023

These four bundles address the remaining concerns from the review at
`67ad5e3`; authoring and committing a plan does not implement its capability
or earn a higher readiness score. Task statuses remain planned, with no
landing OIDs, validation bases or execution evidence fabricated. A committed
bundle is **Unregistered** until Phase R; before commit it is AwaitingCommit.

The integration order is **0022 → 0020 → 0021 → 0023**: establish the lifecycle
boundary against the existing executor first, adopt it in the autonomous
host, add shared contract admission, then prove the complete candidate; plan
numbers identify concerns, not an instruction to ignore prerequisites.
Independent design and test-fixture work can proceed earlier, but final
acceptance requires the integrated production paths. Each local task DAG uses
only task IDs in its own bundle; the coordinator enforces this cross-plan order.

Plan 0022 has a deliberate native-containment feasibility gate: the existing
safe standard-library surface does not itself establish the required process
tree guarantee, and any new runtime prerequisite or charter/platform change
requires a recorded decision before dependent implementation. Plan 0023's
live-model task requires real host access and human-reviewed transcripts;
missing evidence is never a passed skip. Neither gate prevents authoring
these plans, and neither is represented here as already resolved.
