# Plan 0005 — Command-Line Interface — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.

- **Goal:** put every engine and store capability behind one table-driven command tree with a single renderer and a frozen structured-output contract.
- **Root cause:** after plan 0004 the engine and store are reachable only from tests — there is no human-usable surface and no pinned output contract for the MCP server to match.
- **Approach:** land the args table and output frame first, then offline, store, and ops commands over the existing core and store APIs, and freeze the contract with end-to-end golden sessions plus per-command `--json` fixtures that plan 0006 must match byte-for-byte.
- **Progress:** 0/8 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** The full `fsm` command tree drives authoring, execution, diagnosis, and audit ad hoc, with `--json` output byte-identical to the future MCP structured results.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
