# Plan 0016 — Executor Policy — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** give the executor the policy it was deliberately shipped without — journaled retries with deterministic backoff, exhaustion that still fires the machine's failure path, bounded concurrency with per-instance fairness — and a second handler kind that calls another MCP server's tool.
- **Root cause:** a transient failure is currently a permanent one because there is no retry, nothing bounds in-flight handlers so one instance's queue can starve the store, and an engine that speaks MCP fluently as a server has to shell out to reach the ecosystem it belongs to.
- **Approach:** journal every failed attempt as its own record so the attempt count and the backoff deadline are both derived from records and a restarted executor resumes mid-retry rather than restarting the count; compute backoff from the last attempt's journaled timestamp with **no jitter**, because jitter would break the restart-equivalence property the executor's determinism rests on; apply both caps over a stable total order with round-robin fairness so the same observation always yields the same directives; and add the MCP handler kind without widening the security boundary — a literal rooted `argv[0]`, one fixed tool name, and a table-declared argument template, with one process and one tool call per effect.
- **Progress:** 5/12 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A blipped network costs a retry instead of a compensating branch, a store with a thousand pending effects runs eight at a time fairly, and a workflow gate can be handled by any MCP tool an operator is willing to name.

_Task frontmatter is authoritative; this file is the roll-up._
