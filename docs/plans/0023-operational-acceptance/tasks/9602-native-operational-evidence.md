---
id: native-operational-evidence
title: "Native Operational Evidence"
workstream: "0096"
kind: task
depends_on:
  - operational-soak-harness
gated: false
touches:
  - .github/workflows/ci.yml
  - .github/workflows/operational-acceptance.yml
  - acceptance/run-native.py
  - acceptance/profiles/operational.json
  - docs/OPERATIONS.md
status: planned
merged_as: ""
---
# Native Operational Evidence

A Linux container cannot prove Windows or macOS process lifecycle behavior.

**Steps:**

1. Add native installed-binary acceptance/smoke coverage for the existing
   three-OS, two-toolchain support matrix without weakening its Rust gates.
2. Add a separately invoked sustained workflow for immutable candidate SHAs,
   with enough bounded runtime for the eight-hour profile on explicitly
   suitable native runners; document runner provisioning and cost separately
   from the existing short CI jobs.
3. Validate candidate provenance and fixture revisions, run the entire
   required scenario inventory, and retain reports plus failure diagnostics;
   upload must occur on failure as well as success.
4. Run sustained stable candidates on Linux, macOS and Windows, including
   native signals/termination and hard-kill cases from 0022, not emulations.
5. Record missing runners, unavailable containment or cancelled jobs as
   incomplete evidence; fix defects and rerun affected axes before closing.

**Tests:**

- All six native smoke axes pass with matching candidate identity and no
  required skips; consumer-install Podman acceptance passes separately.
- Three native sustained reports each exceed both architecture floors and
  meet the frozen correctness/resource/latency bounds.
- A cancelled job, unsupported profile, stale SHA and missing artifact each
  prevent evidence from being presented as a complete candidate pass.

- **Done when:** inspectable CI/run artifacts prove the complete native smoke
  matrix and three sustained OS runs for one candidate, with no substituted
  cross-compilation, passing skip or unresolved lifecycle prerequisite.
