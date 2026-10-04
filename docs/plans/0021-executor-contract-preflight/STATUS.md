# Plan 0021 — Executor Contract Preflight — Unregistered

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file; task-level truth lives in [tasks/](tasks/) frontmatter, and the integration coordinator owns lifecycle changes.

- **Status:** Unregistered; implementation has not started.
- **Goal:** diagnose machine-handler incompatibilities before authoring completes and prevent incompatible workflows from starting external operations.
- **Root cause:** machine compilation and handler-table parsing validate separate inputs, while discovery and `--check` do not enforce their shared contract before spawn.
- **Approach:** one bounded structural analyzer, explicit manual policy and uncertainty, shared admission at the production execution boundary, and equivalent CLI/MCP reports verified with independent fixtures.
- **Progress:** 0/6 tasks done; 0 blocked; 0 dropped.
- **Integration:** not registered; no validation base, task landing OIDs, or implementation evidence exists yet; this bundle describes future work and must be committed before Phase R can bind its validation base.
- **Exceptions:** none.
- **Outcome:** not implemented; completion requires the task acceptance evidence and the stable host and relevant platform gates, with unexecuted environments recorded.

_Task frontmatter is authoritative; this file is the roll-up._
