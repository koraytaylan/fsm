# Plan 0001 — Foundations & Walking Skeleton — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.

- **Goal:** land the zero-dependency bedrock (workspace, policy gates, JSON, canonical form, SHA-256, decimals) and prove the MCP stdio wire end-to-end with a walking `fsm serve` skeleton.
- **Root cause:** every foundation module replaces a battle-tested crate under the zero-dependency constraint, and MCP host compatibility is the highest-uncertainty risk — both must be retired first, fixtures-first.
- **Approach:** scaffold two crates with pinned manifests and machine-checked gates; land each foundation module against committed external-truth vectors (JSONTestSuite-style corpus, NIST FIPS 180-4, a Python integer-arithmetic decimal oracle); ship a minimal JSON-RPC loop pinned by a byte-exact recorded transcript.
- **Progress:** 0/13 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A zero-dependency two-crate workspace whose JSON, SHA-256, and decimal foundations are vector-tested, with `fsm serve` completing a byte-exact MCP initialize/ping/tools handshake.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
