# Plan 0019 — Consolidation — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** make the gate honest before plan 0017 asks it to prove a journal format change — clear the `--all-targets` clippy findings, widen the committed gate so test code is held to production lints permanently, guard the one performance signal in the workspace, and bound the provisional `fsm-execute` surface.
- **Root cause:** `cargo clippy --workspace --all-targets` does not merely warn, it **errors** — a workspace-level `print_stderr` deny is violated at `crates/fsm-core/tests/enumerate_small.rs:796`, hiding roughly 95 further findings behind a failed compile — and the committed gate omits `--all-targets`, so none of it has ever been red. Meanwhile `append_latency.rs` is the only performance measurement in the workspace, it is `#[ignore]`d and run by hand, and `fsm-execute`'s provisional label bounds nothing because no test notices a new public item.
- **Approach:** fix the hard error first since nothing behind it is visible until it is, then clear the rest with the mechanical fix the lint suggests — and where a fix would change behaviour, an `#[allow]` with a stated reason rather than a rewrite smuggled into a cleanup. Every golden must be byte-identical afterwards, which is what proves the plan changed nothing. Widen the gate in `CONTRIBUTING.md` and `ci.yml` and record the decision where a contributor meets it. Add a latency **guard** beside the existing measurement — a wide ceiling rather than a tight regression bound, because a flaky performance test on a shared runner is deleted within a month. Enumerate `fsm-execute`'s public surface so an addition is a decision rather than an accident.
- **Progress:** 1/4 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `47d35a241c39e2c1ffad648dd68f023a62ec0fb1`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** the gate that plan 0017 will be judged against is green on every target, a performance collapse fails a test instead of surviving to a release, and the provisional surface is one somebody chose.

_Task frontmatter is authoritative; this file is the roll-up._
