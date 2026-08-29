# Plan 0018 — Machine Cases — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** let a machine's expected behaviour be committed beside it and falsified by a change, and let a definition that declares `supersedes` be checked against the cases of the machine it supersedes.
- **Root cause:** the engine's own behaviour is pinned by four test layers and the machines it runs are pinned by nothing. `validate`, `simulate`, `analyze`, and `diagram` all describe a definition; none of them expects anything of it, so a model that revises a machine has no way to state what the old one did and no way to discover the new one no longer does it — which became sharper the moment plan 0011 made definitions editable.
- **Approach:** the smallest thing that works — a `fsm.cases/1` file beside a machine, a pure scripted runner in `fsm-core` that generalizes `simulate`'s loop to send, poll, and acknowledge, and a command that compares field by field and names what moved. Nothing is persisted, no store is opened, and no clock is read, so a case that passes on one platform passes on all of them. Regeneration goes through the repository's existing `FSM_REGEN_FIXTURES` idiom and refuses to run against an uncommitted file, so a deliberate behaviour change is a reviewable diff. The `supersedes` delta is a report and never a gate, because a corrected machine usually changes behaviour on purpose.
- **Progress:** 7/7 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `47d35a241c39e2c1ffad648dd68f023a62ec0fb1`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** a machine author edits a definition and finds out immediately which of its committed behaviours moved, and a migration is a reviewed diff instead of a hope.

_Task frontmatter is authoritative; this file is the roll-up._
