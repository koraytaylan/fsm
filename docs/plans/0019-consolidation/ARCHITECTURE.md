# Architecture — Plan 0019

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Your task's **Tests:** block is the complete acceptance inventory.
3. Stay inside your task's `touches` list.
4. Run the gates locally before every commit, and from this plan onward run the widened one: `cargo test && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`.
5. **No behaviour changes.** `CONTRIBUTING.md`: a cleanup commit asserts that no byte written to disk changed. Every task here is bound by that, and the goldens are how it is proved rather than promised.
6. Where a lint fix would alter behaviour, the fix is an `#[allow]` with a stated reason. A rewrite smuggled into a cleanup commit is the exact thing the separate-commit rule exists to prevent.

## 0000 — Orientation: the measured starting state

Measured on this host at `develop` `47d35a2`:

- `cargo clippy --workspace --all-targets` **fails to compile** `crates/fsm-core/tests/enumerate_small.rs`: `error: use of eprintln!` at line 796, from the workspace-level `print_stderr = "deny"` in the root `Cargo.toml`. This is a hard error, not a warning, so every finding behind it is currently unreported.
- Roughly 95 further findings across `oracle/eval.rs`, `oracle/step.rs`, `spec_validate.rs`, `canon_golden.rs`, `cli/diagram.rs`, `cli/machine.rs`, `crash_harness.rs`, `cli_golden.rs`, `naive_caller/*`, `mcp_full.rs` and neighbours — all in files predating plan 0009. Plans 0009 through 0016's own files were cleared in `46450d0`.
- The dominant kinds are `useless_conversion` on `&str` literals in flag tables, and `type_complexity` on tuple-heavy test helpers.
- `crates/fsm-store/tests/append_latency.rs` is the only performance test and is `#[ignore]`d; `docs/RELEASE.md` invokes it manually with `FSM_BENCH_ROOT`.

The per-job ceiling is 45 minutes across three operating systems and two toolchains, already dominated by `crash_harness.rs` and `executor_chaos.rs`. Widening a lint costs compile time, not test time, but the latency guard costs test time and must be sized by measurement.

## 0087 — The gate

### Clearing the findings (task `8701`)

Order matters: the hard error first, because nothing behind it is visible until it is fixed.

`enumerate_small.rs:796` prints a summary line through `eprintln!`. It is a genuine diagnostic an author wants when running that suite by hand, so the fix is not deletion. Use the mechanism the workspace already provides for a test that must write to a stream, and if none applies, an `#[allow(clippy::print_stderr)]` **with a reason comment** naming why this test prints. The reason is the point; a bare `#[allow]` moves the problem rather than resolving it.

For the rest, prefer the mechanical fix the lint suggests. Two rules keep this a cleanup rather than a redesign:

- A fix that changes what a test asserts is not a fix. If removing an `.into()` changes a type in a way that changes behaviour, `#[allow]` it and say why.
- `type_complexity` on a test helper is usually telling the truth. A named struct or type alias is the right fix where it makes the helper readable; where the tuple is genuinely clearer, `#[allow]` with the reason.

Every golden in the repository must be byte-identical afterwards. That is the assertion that this task changed nothing, and it is stronger than any review.

### Widening the gate (task `8702`)

`CONTRIBUTING.md`'s "Verification and portability" block lists the stable host gate; the clippy line becomes `cargo +stable clippy --workspace --all-targets -- -D warnings`. `.github/workflows/ci.yml` gets the same change in whichever job runs clippy.

This is a **CONTRIBUTING decision**, not a cleanup: it makes test code permanently subject to production lints, and it is the reason plan 0009-onward files were cleared in the first place. Record the decision where a future contributor meets it — the sentence should say what changed and why test code is held to the same bar, so nobody narrows it back to buy a green build.

### The latency guard (task `8703`)

`append_latency.rs` stays `#[ignore]`d as a **measurement**, and gains a sibling that runs by default as a **guard**. The distinction is the design: a measurement reports a number for a human; a guard asserts a bound and fails.

The guard's shape is dictated by what CI actually is — a shared, noisy, variable machine:

- Assert a **ceiling with a wide tolerance**, not a regression against a stored number. A tight bound on a shared runner produces flakes, and a flaky performance test is deleted within a month.
- Commit the baseline and the tolerance as named constants with the measured numbers and the host in a comment, so the next person to widen it has to state why.
- Size the iteration count by measurement on this host and record the debug and release timings in the commit message, exactly as `executor_policy_chaos.rs` did for `FSM_POLICY_CHAOS_ITERS`.
- Provide an environment variable to raise the iteration count for a real measurement run, so the guard and the harness are one code path with two budgets.

`docs/RELEASE.md`'s manual step stays: the guard catches a collapse, and the manual harness produces the table. They answer different questions.

## 0088 — The provisional boundary

### Declaring the surface (task `8801`)

`docs/API-POLICY.md` marks `fsm-execute` provisional "because it has no outside-workspace acceptance check". True, and unbounded: nothing notices a new public item, so the label currently covers whatever the crate happens to expose this release.

Add a committed inventory of `fsm-execute`'s public surface and a test that compares the crate against it. A new public item fails the test until it is added to the inventory, which turns "provisional" into a boundary somebody decided rather than one that accumulated.

This deliberately does **not** stabilise the crate. It still has no outside-workspace acceptance check, and plan 0017 moves the store underneath it. The `API-POLICY.md` entry gains one sentence saying the surface is now enumerated and where the inventory lives, so a reader can see exactly what "provisional" covers.

The same technique would suit `fsm-core` and `fsm-store` and is deliberately not applied to them here: both already have acceptance checks that fail when their surface regresses, which is a stronger guarantee than an inventory, and adding a second mechanism where a better one exists is the speculative generality `CONTRIBUTING.md` warns about.
