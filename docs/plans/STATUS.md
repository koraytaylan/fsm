# Plans — roll-up board

One row per plan. Task status is authored in each plan's `tasks/*.md` frontmatter and summarized by its `STATUS.md`.

| Plan | Title | Status | Tasks | Outcome | Status doc |
|---|---|---|---|---|---|
| 0001 | Foundations & Walking Skeleton | 📋 Planned | 0/13 | A zero-dependency two-crate workspace whose JSON, SHA-256, and decimal foundations are vector-tested, with `fsm serve` completing a byte-exact MCP initialize/ping/tools handshake. | [status](0001-foundations-walking-skeleton/STATUS.md) |
| 0002 | Expression Language | 📋 Planned | 0/7 | Guards, actions, and invariants parse, typecheck, and evaluate as a total, step-budgeted, exactly-typed expression language with three-valued partial evaluation and span-precise errors. | [status](0002-expression-language/STATUS.md) |
| 0003 | Statechart Engine | 📋 Planned | 0/17 | Hierarchical machine definitions validate, compile, and execute through a pure `step()` with LCA exit/entry pipelines, history, explain traces, simulation, and static analysis, proven against a naive oracle interpreter. | [status](0003-statechart-engine/STATUS.md) |
| 0004 | Journal & Store | 📋 Planned | 0/10 | Every mutation commits through a hash-chained fsync'd journal that recovers, verifies, and replays bit-identically, surviving a kill -9 crash harness. | [status](0004-journal-and-store/STATUS.md) |
| 0005 | Command-Line Interface | 📋 Planned | 0/8 | The full `fsm` command tree drives authoring, execution, diagnosis, and audit ad hoc, with `--json` output byte-identical to the future MCP structured results. | [status](0005-cli/STATUS.md) |
| 0006 | MCP Server | 📋 Planned | 0/8 | The complete 13-tool MCP surface with resources, prompt, and instructions passes byte-exact golden transcripts and a naive-caller error-recovery suite. | [status](0006-mcp-server/STATUS.md) |
| 0007 | Hardening, Examples & Docs | 📋 Planned | 0/8 | Fuzzing, chaos, and determinism suites guard the engine; three worked example machines, the completed SPEC, and the README make the initial release shippable. | [status](0007-hardening-examples-docs/STATUS.md) |
| 0008 | Effect Executor | 🚧 In progress | 2/13 | A standalone `fsm execute` process watches the effect outbox, runs operator-configured handlers as subprocesses, acknowledges outcomes into the journal, and polls due deadlines — so a triggered workflow proceeds gate-to-gate unattended, resumable after a kill at any instant, with the engine's semantics untouched. | [status](0008-effect-executor/STATUS.md) |
