# Plan 0006 — MCP Server — ✅ Complete

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** ✅ Complete.

- **Goal:** expose the complete 13-tool surface, resources, prompt, and instructions over the hardened stdio transport, with every domain error arriving in-band and teaching its own fix.
- **Root cause:** the plan-0001 skeleton proves only the handshake — there is no negotiation hardening, no real tool, no resource, and the error-channel rule exists only on paper.
- **Approach:** harden lifecycle first (2801), land the tool table in three strictly separated files (schemas → descriptions → dispatch) over the plan-0004 store and plan-0005 renderer, add resources and the prompt as parallel stubs pre-routed by the lifecycle task, and pin the whole surface with per-revision byte-exact transcripts, a CLI-parity test, and a naive-caller recovery suite.
- **Progress:** 8/8 tasks done; 0 blocked; 0 dropped.
- **Integration:** `complete`; run — (the single Makina run, `01M018Z4PKM8RQSARHTNJ824TX`, marked the plan failed and every later task skipped after task 0101 tripped the reviewer cap on a footprint false positive — four test-output files sat outside its `touches`. The plan was therefore landed directly on `develop`, one commit per task in dependency order, and sealed here after the fact.); base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode `direct-to-develop`; final integration `d36b954` (the `v0.1.0` tag commit).
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** The complete 13-tool MCP surface with resources, prompt, and instructions passes byte-exact golden transcripts and a naive-caller error-recovery suite.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
