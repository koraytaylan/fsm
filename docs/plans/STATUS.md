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
