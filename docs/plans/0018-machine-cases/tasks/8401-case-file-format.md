---
id: case-file-format
title: "Case File Format"
workstream: "0084"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/cases/format.rs
  - crates/fsm-core/src/cases/mod.rs
  - crates/fsm-core/src/lib.rs
  - crates/fsm-core/tests/case_format.rs
  - crates/fsm-core/tests/fixtures/cases_v1.json
status: done
merged_as: ""
---
# Case File Format

A case file that ignores a mistyped key reports success it did not earn, so every key set is closed and every refusal names the key.

**Steps:**

1. Create `crates/fsm-core/src/cases/` with `format.rs` holding the parser and the format constant `fsm.cases/1`, and export the module from `crates/fsm-core/src/lib.rs`.
2. Parse the document shape: `{format, machine, cases: [{name, context?, script: [...], expect: {...}}]}`.
3. **Close every key set at every level.** An unknown key is a refusal naming the key and listing the accepted ones. A case file with a typo'd `expects` that silently asserts nothing is worse than no case file at all, and it is the failure mode this format exists to prevent.
4. A script step is exactly one of `send`, `poll`, or `ack`, discriminated by which key is present. Zero keys present and two keys present are both refusals, each naming what was found.
5. `send` carries an event name and an optional `payload`; `poll` carries an explicit `now_ms`; `ack` carries an effect name and an `outcome` with an optional `result`. The runner never invents a timestamp, so `poll` without one is a refusal — `fsm-core` has no clock and must not acquire one.
6. `expect` fields — `configuration`, `context`, `enabled`, `effects`, `terminal` — are individually optional and each asserts only itself. `effects` names the pending effects by the name they were emitted under and asserts nothing about their arguments: the script already names effects by name when it acks them, so the file has one vocabulary rather than two. Say so at the site, because a reader will expect to be able to pin an argument. A case naming only `configuration` asserts only configuration. Document this at the site; a reader will assume an absent field means "expect empty".
7. `machine` is a name or content hash carried for reporting only. The definition under test arrives separately, which is what lets one case file run against two definitions in `0086`.
8. Add three ceilings to the existing limits register: cases per file, script steps per case, and total document bytes. Each is a named constant with a `def/limit_*`-shaped error, and each gets an exact-limit and limit-plus-one regression, per `CONTRIBUTING.md`. **Shaped like them, not in their namespace.** `spec_appendix.rs` requires every `def/*` code to have a row in SPEC's structural-rules table, which is a table of rules about a *machine definition*; a case-file ceiling has no place in it, and adding one would make that table mean something else. So the codes live under `case/*` — `case/shape`, `case/unknown_key`, `case/limit_bytes`, `case/limit_cases`, `case/limit_steps` — registered in `ALL_CODES` and listed in Appendix A like every other code.
9. Reuse the workspace JSON parser and `JsonLimits`; add no second parser and no second limit mechanism.
10. Stay pure. This module reads no file — it takes bytes. `crates/fsm-cli/tests/policy.rs` scans `fsm-core` for `std::fs` and will fail the build if reading moves in here.

**Tests:**

- `crates/fsm-core/tests/case_format.rs`: `crates/fsm-core/tests/fixtures/cases_v1.json` is a committed golden that parses to the expected structure, compared through `include_str!`.
- An unknown key at the document, case, script-step, and `expect` levels is refused, and each refusal **names the offending key** — four cases, one per level.
- A script step with no discriminating key is refused; one with two is refused naming both.
- A `poll` with no `now_ms` is refused.
- An `ack` with no outcome is refused; an unknown outcome value is refused.
- A `format` other than `fsm.cases/1` is refused, and the message names the format found.
- Each of the three ceilings has an exact-limit case that passes and a limit-plus-one case that is refused with the right code.
- A document over the byte ceiling is refused without being fully parsed.
- A case with **no** `expect` at all parses and asserts nothing — legal, because its script still has to run.
- An `expect` with only `configuration` parses and records that only configuration is asserted — assert on the parsed structure, since this is the property step 6 documents.
- Parsing is deterministic and allocation-bounded on hostile input: a deeply nested payload is refused by `JsonLimits` rather than recursing.

- **Done when:** `cargo test -p fsm-core --test case_format` passes every case above, every key set is closed with refusals naming the key at all four levels, all three ceilings have exact and limit-plus-one regressions, the committed golden matches byte for byte, `cargo test -p fsm-cli --test policy` confirms the module stays pure, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
