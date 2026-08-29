---
id: case-script-runner
title: "Case Script Runner"
workstream: "0084"
kind: task
depends_on:
  - case-file-format
gated: false
touches:
  - crates/fsm-core/src/cases/run.rs
  - crates/fsm-core/tests/case_runner.rs
status: done
merged_as: ""
---
# Case Script Runner

`simulate` stops where a real workflow starts — it polls no deadline and acknowledges no effect — so the scripted runner covers the three things a workflow actually does, on the production stepper and nothing else.

**Steps:**

1. Create `crates/fsm-core/src/cases/run.rs` taking a `CompiledMachine`, a `Tree`, and one parsed case, and returning a per-step observation. No I/O, no clock, no `HashMap`.
2. Build on the same primitives `crates/fsm-core/src/simulate.rs` uses. Do **not** write a second interpreter: `crates/fsm-core/tests/oracle.rs` is a second interpreter on purpose, to catch engine bugs, and a second one here would report a machine as broken when the interpreter was.
3. Leave `simulate`, `SimReport`, `SimStep`, and `OnReject` untouched. They are public API under `docs/API-POLICY.md`; this is a new entry point beside them.
4. Map `send` onto the stepper call `simulate` makes, with the step index in milliseconds as its timestamp exactly as `simulate` does, and `poll` onto `fsm_core::step::poll_deadline` with the script's **explicit** `now_ms`.
5. **There is no pure acknowledgement primitive, and there must not be one.** Acking lives entirely in `fsm-store` (`store/instance/ack.rs`): it removes the effect id from `instance.pending` and journals a record. In pure terms an ack is exactly that removal — it changes no configuration, raises no event, and drives no transition. Implement it as the removal, and write the note at the site, because "an ack never drives a transition" is an engine rule and this is the clearest place it is visible.
6. It follows that a case does **not** get the executor's `on_ok` / `on_failed` behaviour for free: that mapping lives in the handler table, not in the machine, so a case that wants the follow-up event writes the `send` itself. Say this in the module doc — an author who expects an ack to advance a workflow will otherwise write a case that mystifies them.
7. Record after every step: outcome, configuration, context, emitted effects, pending effects, and enabled events. The last two are what `SimStep` lacks and what a workflow case needs.
8. An `ack` naming an effect that is not pending fails the case with a message listing the effects that **were** pending. That is the mistake an author makes, and the list is the fix — a bare "unknown effect" costs a round trip to discover.
9. **Run the whole script and report every divergence.** Do not stop at the first failure: an author correcting one expectation wants to see the other two in the same run.
10. Inherit every engine bound without relaxing any. A case that exceeds the 64-microstep reaction bound, the evaluation budget, or a payload ceiling reports the engine's own error exactly as any caller does. A case runner that quietly raised a bound would be testing a machine the engine will not run.
11. Take creation context overrides from the case's `context`, using the same coercion path the existing callers use (`fsm_core::replay::parse_ctx_val` against the machine's declared slot), so a context value written in a case file means what it means everywhere else. An undeclared slot and a value of the wrong type are both refused before the run starts, and the refusal names the slot and lists what the machine declares.
12. Track pending effects by **name**, not by id. A live store allocates effect ids; a pure run has no allocator and must not grow one, and the name is the vocabulary the case file already acks in. Creation can emit, so an effect emitted by the entry actions of the initial configuration is pending from the first instant — a case that acks one before its first send is correct.
13. An ack's `result` reaches no machine. It is carried in the file because the reporting and regeneration surfaces show it, and dropping it in the runner would make a case's text and its run disagree; the runner names the field and ignores it deliberately rather than by omission.

**Tests:**

- `crates/fsm-core/tests/case_runner.rs`: a three-step script of send, poll, and ack drives a committed example machine to the expected configuration.
- A `poll` at a time before a deadline is due applies nothing; the same poll at or after it applies exactly one due deadline. Two due deadlines need two polls — assert this, since it is the engine's rule and a case author will assume otherwise.
- An `ack` of a pending effect clears it from `pending` and **changes nothing else** — configuration, context, and enabled events are identical before and after. This is the assertion that an ack drives no transition, and it belongs here because this is where it is cheapest to state.
- A following `send` proceeds after the ack.
- An `ack` naming a non-pending effect fails the case and the message lists the pending effects.
- A rejected `send` is recorded with its rejection and the script **continues**, and the run reports every later divergence too.
- Enabled events and pending effects are recorded after each step, not only at the end.
- Determinism: the same case run twice produces byte-identical observations, and the run reads no clock — asserted by `crates/fsm-cli/tests/policy.rs` covering the new module.
- A machine whose reaction exceeds 64 microsteps reports the engine's own error rather than a case-runner error.
- A case whose evaluation exceeds the budget reports the engine's budget error unchanged.
- A run over a machine with parallel regions records the full configuration, not a single leaf.
- `simulate`'s own tests still pass unchanged, proving the public entry point did not move.

- **Done when:** `cargo test -p fsm-core --test case_runner` passes every case above, the runner builds on the production stepper with no second interpreter, `simulate` and its types are unchanged, every engine bound is inherited and asserted, the whole script runs before reporting, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
