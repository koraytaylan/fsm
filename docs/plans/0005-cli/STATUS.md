# Plan 0005 — Command-Line Interface — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.

- **Goal:** put every engine and store capability behind one table-driven command tree with a single renderer and a frozen structured-output contract.
- **Root cause:** after plan 0004 the engine and store are reachable only from tests — there is no human-usable surface and no pinned output contract for the MCP server to match.
- **Approach:** land the args table and output frame first, then offline, store, and ops commands over the existing core and store APIs, and freeze the contract with end-to-end golden sessions plus per-command `--json` fixtures that plan 0006 must match byte-for-byte.
- **Progress:** 8/8 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run — (the single Makina run, `01M018Z4PKM8RQSARHTNJ824TX`, marked the plan failed and every later task skipped after task 0101 tripped the reviewer cap on a footprint false positive — four test-output files sat outside its `touches`. The plan was therefore landed directly on `develop`, one commit per task in dependency order, and sealed here after the fact.); base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode `direct-to-develop`; final integration `d36b954` (the `v0.1.0` tag commit).
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** The full `fsm` command tree drives authoring, execution, diagnosis, and audit ad hoc, with `--json` output byte-identical to the future MCP structured results.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
