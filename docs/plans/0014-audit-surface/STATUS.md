# Plan 0014 — Audit Surface — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** put the five CLI-only audit capabilities — `explain`, `journal verify`, `journal replay`, `doctor`, and `annotate` — into the MCP surface, and keep the server alive when the store will not open so the diagnostic tools are reachable at the moment they are needed.
- **Root cause:** the primary operator of this engine is a model, and the model cannot verify the hash chain it is told is tamper-evident, cannot reach the best diagnostic affordance in the system, cannot leave a note in the audit trail it is producing, and — when a store is genuinely unhealthy — loses its server entirely, because `serve` exits with one stderr line the user may never see.
- **Approach:** expose existing logic without changing a single conclusion, adding only an incremental seam so verification can report progress and honour cancellation; make an unopenable store a **reported** state rather than a fatal one, serving the three diagnostic tools from a read-only classification and refusing everything else with the health, blast radius, and remedy; and deliberately withhold `repair`, because it destroys data and its safety argument rests on a human reading quarantined bytes — the tools name the command, a person runs it.
- **Progress:** 6/9 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A model that just cancelled a workflow can say why in the journal, check that the chain it is appending to is intact, and — when something is wrong — get the health, the blast radius, and the exact command to hand to a person.

_Task frontmatter is authoritative; this file is the roll-up._
