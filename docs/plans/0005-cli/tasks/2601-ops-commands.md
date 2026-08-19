---
id: ops-commands
title: "Ops Commands"
workstream: "0026"
kind: task
depends_on:
  - instance-commands
gated: false
touches:
  - crates/fsm-cli/src/cli/ops.rs
status: planned
merged_as: ""
---
# Ops Commands

The auditor and operator surface: full-chain verification with granular integrity exit codes, snapshot-free replay comparison, a store health report, and the explicit quarantine-then-truncate repair.

**Steps:**

1. Fill `crates/fsm-cli/src/cli/ops.rs::SPECS` with `journal verify [--report]` mapping `JournalHealth` to exit codes 0 Ok / 2 TornTail / 3 ChainBroken / 4 StateHashMismatch / 5 NonCanonical / 6 LockIo, `--report` printing per-segment progress and the final counts-and-hashes summary.
2. Add `journal replay [--to-seq N]` — refold ignoring snapshots and report hash agreement or the first divergent seq.
3. Add `doctor` (data dir and VERSION, read-only store health, snapshot inventory, quick verify summary, effective env) and `repair --truncate-torn-tail` (invokes the plan-0004 repair, printing quarantine path and truncation seq; on any non-torn-tail health it refuses and exits with that health's verify exit code). Doctor never probes lock availability because inspection commands do not acquire the advisory lock.
4. Write the inline test module encoding exactly the inventory under **Tests** (spec `run` functions over temp *copies* of the plan-0004 journal fixtures).

**Tests:**

- Inline in `ops.rs` — the health→exit-code map is a single function unit-tested over all six variants (`Ok`→0, `TornTail`→2, `ChainBroken`→3, `StateHashMismatch`→4, `NonCanonical`→5, `LockIo`→6), so every mapping row is pinned even where no directory fixture exists.
- Directory-driven `journal verify`: copies of the committed fixtures exit with their codes — `clean/`→0, `torn_tail/`→2, `interior_flip/`→3, `seq_gap/`→3, `non_canonical/`→5 — and `--report` on `clean/` prints the per-segment lines plus the counts-and-final-hashes summary matching the fixture's pinned values (`StateHashMismatch` and `LockIo` are covered by the map unit above; no directory fixture reproduces them deterministically here).
- `journal replay`: on a healthy store built in-test, reports agreement (live vs snapshot-free refold), exit 0; `--to-seq N` stops the refold at N and compares the prefix.
- `doctor`: renders the data dir path, the `VERSION` value, read-only store health, the snapshot count, the quick verify summary line, and the effective `FSM_DATA_DIR`/`FSM_LOG` values without probing or acquiring the advisory lock.
- `repair --truncate-torn-tail`: on a torn-tail copy prints the quarantine path and truncation seq, exit 0, and a follow-up `journal verify` exits 0; on an `interior_flip/` copy it refuses with the verify-style report and exits 3 (the found health's code), directory untouched.

- **Done when:** inline ops tests prove the six-way exit-code mapping against the fixtures and the repair refusal path, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
