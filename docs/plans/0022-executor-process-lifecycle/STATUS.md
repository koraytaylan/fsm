# Plan 0022 — Executor Process Lifecycle — Unregistered

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and the integration coordinator owns
lifecycle updates.

- **Status:** Unregistered; negative feasibility probes landed independently in `4206e8e`;
  production lifecycle implementation has not started.
- **Goal:** prevent a successor from overlapping a surviving local handler
  tree, with bounded shutdown and evidence-based restart for both process and
  MCP handlers.
- **Root cause:** direct-child handles and short-lived writer locks do not
  contain descendants or persist execution ownership across executor death.
- **Approach:** resolve the native containment prerequisite, journal claims
  before launch, prove tree closure before reuse, and drive every execution
  host through the same shutdown and recovery protocol.
- **Progress:** 0/7 tasks done; 0 blocked; 0 dropped; 1 planned gated task.
- **Integration:** not started; no validation base, landing OIDs, native
  feasibility result or implementation gate evidence is claimed.
- **Exceptions:** task `lifecycle-containment-feasibility` requires a proved
  charter-compatible backend or an explicit project decision before dependent
  implementation; durable fail-closed behavior alone cannot complete this plan.
- **Feasibility evidence:** independent probe/decision commit
  `4206e8e27c853741d9f2a290090f0c8ed8edf7a3` retains the unreleased gate. [lifecycle decision record](../../EXECUTOR-LIFECYCLE.md)
  and `crates/fsm-execute/tests/lifecycle_platform.rs` demonstrate surviving
  descendants and retained pipes after root kill and normal root exit on
  Linux, at Rust 1.89.0 and stable 1.98.1. These are negative probes, not
  positive containment acceptance. Delegated user authority now selects a
  provisioned Linux/systemd backend
  for the initial contained executor, preserving safe Rust, zero crates and
  MSRV 1.89; see the decision record. A protected dynamic-UID host experiment
  contained an orphan descendant and closed its pipe on unit stop. The
  reproducible compiled-Rust driver passes four partial native cases at MSRV
  and stable: root exit, root kill, changed process group and launcher death,
  with protected UID/membership checks and unrelated-process survival. Full
  native proof and gate release remain pending; no task is marked done or
  given a landing OID.
- **Protected native review:** decision/probes landed in `3f397f5`, with
  failed-unit collection repair in `dd97260`. Frozen range
  `69ad2aa3b12080407314c8be86bac741ac4cbc68..dd9726081b86e06b41d0c08295c06fac71b8d35f`
  passes both toolchain probes from a clean checkout. Retained reports are
  `/tmp/fsm-systemd-final-msrv.json` and `/tmp/fsm-systemd-final-stable.json`:
  exact source, dirty false, four cases each, gate unreleased. Review separates
  failed service outcomes from actual domain/pipe closure and collects stopped
  probe records; no task-owned units remain. Formatting, all-targets Clippy,
  warning-free docs, zero-dependency, embedding and full stable debug/release
  workspace gates pass. MSRV negative probes and all 45 Python harness tests
  also pass. Native macOS/Windows remain unexecuted. Permanent identity,
  journal-bound launch authorization, spawn during stop, privileged supervisor
  recovery, signals and uncertainty refusal still need positive native proof.
- **Shutdown-race preparation:** the native probe now additionally proves
  two descendants can fork after a real stop job reaches deactivating and
  remain contained until final killing. A separate kernel freeze/kill case
  observes frozen=1 and populated=1 before arming a fork, then requires no
  leaf marker, domain removal and bounded pipe EOF. All six positive cases
  pass at MSRV and stable. A separately labelled SendSIGKILL=no mutation
  fails with an observed populated domain; emergency fixture cleanup cannot
  turn that failed result into a pass and leaves unrelated processes alive.
  UID/effective-capability/no-new-privilege assertions cover root, initial
  descendant and the late descendants. These remain partial native proofs;
  permanent identity, privileged supervisor recovery, atomic authorization,
  signal notification and uncertain replacement refusal are pending.
- **Outcome:** pending implementation and native lifecycle evidence.

_A committed bundle remains Unregistered until Phase R binds its validation base._
