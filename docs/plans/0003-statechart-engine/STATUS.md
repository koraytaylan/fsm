# Plan 0003 — Statechart Engine — 📋 Planned

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 📋 Planned.

- **Goal:** land the hierarchical machine model and the pure `step()` — structural validation, content-addressed identity, expression binding, static analysis, LCA exit/entry pipelines, history, traces, state hashing, simulation, and enabled-events — proven against a naive oracle before any journal exists.
- **Root cause:** the transition semantics ossify the moment the first journal record is written in plan 0004, so every semantic ruling must be pinned by prose-derived goldens and differential proof now, not discovered later as compatibility.
- **Approach:** parse and validate the tree-shaped `fsm.machine/1` format, compile expressions with exact assignment typing, isolate all tree/LCA/history computation in one module, implement the atomic action pipeline exactly per SPEC pseudocode, and hold the whole engine to an exhaustive small-tree differential against a deliberately naive second interpreter.
- **Progress:** 0/17 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `2d2f8ce57b53bf773ab80b2200e25ab40a8f4afd`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** Hierarchical machine definitions validate, compile, and execute through a pure `step()` with LCA exit/entry pipelines, history, explain traces, simulation, and static analysis, proven against a naive oracle interpreter.

_Last updated: 2026-08-14, against `develop` @ `2d2f8ce`._
