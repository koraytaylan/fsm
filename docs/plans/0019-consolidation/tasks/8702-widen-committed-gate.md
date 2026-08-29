---
id: widen-committed-gate
title: "Widen The Committed Gate"
workstream: "0087"
kind: chore
depends_on:
  - all-targets-clippy-clean
gated: false
touches:
  - CONTRIBUTING.md
  - .github/workflows/ci.yml
status: planned
merged_as: ""
---
# Widen The Committed Gate

Test code is compiled code, it is the larger half of this repository by line count, and until now no lint has reached it.

**Steps:**

1. Change the clippy line in `CONTRIBUTING.md`'s stable host gate to `cargo +stable clippy --workspace --all-targets -- -D warnings`.
2. Make the same change in whichever job runs clippy in `.github/workflows/ci.yml`, leaving the matrix itself untouched — that file is authoritative for platform coverage and this task changes what a job runs, never which jobs exist.
3. Record the decision in `CONTRIBUTING.md` where a contributor meets it, in one or two sentences: test code is held to the same lints as production code, because a test suite this large is read far more often than it is written and a lint that stops at the crate boundary covers the smaller half. `CONTRIBUTING.md` already states that tests are first-class code; this makes the toolchain agree with it.
4. Check the release workflow. If `.github/workflows/release.yml` runs clippy in its verify job, it inherits the same change; if it invokes the `CONTRIBUTING.md` gate by reference, confirm the reference still resolves to the widened command and say which it was in the commit message.
5. Change no behaviour and no other gate. `--all-targets` is the only widening in this plan; do not add lints, do not change the formatter configuration, do not touch the MSRV.
6. Confirm the widened gate is green on the MSRV as well as stable before committing. A lint clean on one toolchain and dirty on the other is a gate that fails for the next contributor rather than for this one.

**Tests:**

- `cargo +stable clippy --workspace --all-targets -- -D warnings` exits zero.
- The same command at the MSRV pinned in `rust-toolchain.toml` exits zero.
- `CONTRIBUTING.md`'s gate block contains the widened command, and no other command in that block changed — assert by diffing the block.
- `.github/workflows/ci.yml` runs the widened command, and the job matrix is unchanged — assert by diffing the file for matrix keys.
- The full `CONTRIBUTING.md` gate passes end to end, every phase folded into one explicit verdict rather than a per-phase line, since a per-phase result has been skimmed past in this repository before.
- The recorded decision names both what changed and why, so it survives as a reason rather than a rule.

- **Done when:** the widened clippy command exits zero on stable and at the MSRV, `CONTRIBUTING.md` and `ci.yml` both run it, the CI matrix is unchanged, the release workflow's inheritance is confirmed and stated in the commit message, the full gate passes as one verdict, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
