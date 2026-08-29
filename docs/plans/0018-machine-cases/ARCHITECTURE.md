# Architecture — Plan 0018

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers.
2. Fixtures first: commit the case files and goldens your task names before writing implementation code.
3. Your task's **Tests:** block is the complete acceptance inventory.
4. Stay inside your task's `touches` list.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt`.
6. Write the obvious version.
7. When a golden fails, fix the code to match the fixture.
8. **`fsm-core` purity is the constraint that shapes this whole plan.** The runner lives in the pure crate: no `std::fs`, no `std::net`, no `std::time`, no `f32`/`f64`, no `HashMap`. `crates/fsm-cli/tests/policy.rs` scans for exactly those tokens and will fail you. Reading the case file is the CLI's job; running it is the core's.

## 0000 — Orientation: the four facts that shape this plan

- **`simulate` is almost the runner, and deliberately less.** `crates/fsm-core/src/simulate.rs` already creates an in-memory instance and delivers a list of events, returning per-step `outcome`, `configuration_after`, `ctx_after`, and `effects` — which is most of what an expectation needs. It stops short in two ways that matter: it polls **no** deadlines ("no deadline is implicitly polled") and it never acknowledges an effect, so `pending` starts empty and stays that way. A workflow that waits on a gate cannot be expressed as a `simulate` call.
- **`simulate` is public API and does not move.** `docs/API-POLICY.md` covers `fsm-core`'s surface. `SimReport`, `SimStep`, and `OnReject` keep their exact shapes; the scripted runner is a **new** entry point beside them, and where the two overlap the new one is written in terms of the same step primitives rather than by copying them.
- **Determinism is inherited, not added.** `simulate` uses timestamp zero for creation and `i` milliseconds for event `i`. The scripted runner needs an explicit time for a deadline poll, so the script carries it and the runner never invents one. There is no clock to inject in `fsm-core` and there must not be one.
- **The engine has an oracle, and cases are not it.** `crates/fsm-core/tests/oracle.rs` duplicates production semantics on purpose to catch engine bugs. Cases test *machines*, not the engine, and must be built on the production stepper — a second interpreter here would report a machine as broken when the interpreter was.

## 0084 — The format and the runner

### The file (task `8401`)

`fsm.cases/1`, a JSON document beside a machine:

```json
{
  "format": "fsm.cases/1",
  "machine": "expense_approval",
  "cases": [
    {
      "name": "under_threshold_auto_approves",
      "context": {"amount": "10.00"},
      "script": [
        {"send": "submit", "payload": {"amount": "10.00"}},
        {"poll": 60000},
        {"ack": "notify", "outcome": "ok"}
      ],
      "expect": {
        "configuration": ["approved"],
        "context": {"amount": "10.00", "decision": "auto"},
        "enabled": [],
        "effects": [],
        "terminal": true
      }
    }
  ]
}
```

- Key sets are **closed** at every level, as every other format in this workspace is. An unknown key is a refusal naming the key, never a silently ignored field — a case file with a typo'd `expects` that reports success is worse than no case file.
- `machine` is a name or content hash for reporting only. The definition under test comes from the command line, so one case file can be run against two definitions, which is exactly what `0086` needs.
- A script step is exactly one of `send`, `poll`, or `ack`. Three closed variants, discriminated by which key is present; two keys present is a refusal.
- `expect` fields are individually optional and each asserts only itself. A case that names only `configuration` asserts only configuration. This is what keeps a case file readable when the author cares about one thing.
- Limits get their own `def/limit_*`-style ceilings in the existing register: cases per file, script steps per case, and total file bytes, each with a limit-plus-one regression, per `CONTRIBUTING.md`'s testing standards.

### The runner (task `8402`)

New module `crates/fsm-core/src/cases/` — pure, no I/O, taking a `CompiledMachine`, a `Tree`, and a parsed case.

The three step kinds map onto primitives that already exist, and the point of the task is to use them rather than re-derive them:

| Step | Primitive | Time |
|---|---|---|
| `send` | the same stepper call `simulate` makes | step index in milliseconds, as `simulate` does |
| `poll` | `fsm_core::step::poll_deadline` | the script's explicit `now_ms` |
| `ack` | none — see below | step index |

**There is no pure acknowledgement primitive, and the plan does not add one.** Acking lives entirely in `fsm-store`: `store/instance/ack.rs` removes the effect id from `instance.pending` and journals a record. In pure terms an ack *is* that removal — no configuration change, no event, no transition. The runner implements the removal directly, which makes the case runner the clearest statement in the codebase of the engine rule that an ack never drives a transition.

The consequence is worth documenting rather than discovering: a case does not inherit the executor's `on_ok` / `on_failed` behaviour, because that mapping lives in the handler table and not in the machine. A case that wants the follow-up event writes the `send` itself.

An `ack` names a pending effect by its emitted name; naming one that is not pending fails the case with a message saying which effects *were* pending, because that is the mistake an author makes and the list is the fix.

The runner returns a per-case observation with the same fields `SimStep` carries plus the pending effects and enabled events after each step. It never stops at the first mismatch: it runs the whole script and reports every divergence, since an author fixing one expectation wants to see the other two.

Reaction is bounded at 64 microsteps per event by the engine, and a case that trips it reports `run/` the way any caller does. Cases inherit every engine bound; none is relaxed for them.

### The matcher (task `8403`)

Comparison is field by field, and the failure output names the field. `expected approved, found under_review` beside `expected {amount: 10.00, decision: auto}, found {amount: 10.00}` is a diff an author can act on; two pretty-printed states are a diff they have to read twice.

Ordering rules follow the engine's own: effects compare in emission order because that order is deterministic and load-bearing; `configuration` and `enabled` compare as sets because a configuration is a set and an enabled-event list is derived from a scan whose order is an implementation detail the spec does not fix. Say which is which at the site, because a reader will assume all four are the same.

## 0085 — The surface

### The command (task `8501`)

```
fsm machine test <machine.json> --cases <cases.json> [--case <name>] [--json]
```

Reading the two files and reporting is the CLI's job; everything between is `fsm-core`. Exit code is zero when every case passes and non-zero when any fails, with a summary line that counts.

`--case <name>` runs one case, which is what an author does while fixing one. Output goes through the same `--json` structured shape as every other command.

The command opens **no store**. It takes a definition file and a case file, and it works in a directory that has never held a store — which is what lets it run in a machine author's editor loop and in CI over a repository of definitions.

### Regeneration (task `8502`)

`FSM_REGEN_FIXTURES=1` is this repository's established idiom and cases join it rather than inventing a flag. Regenerating rewrites `expect` blocks from observed behaviour and prints what it changed.

The safeguard is the one that makes regeneration honest: **regeneration refuses to run unless the case file is committed and clean**, so the diff is reviewable in version control rather than silently overwritten. A regeneration that cannot be reviewed is a case file that agrees with the code by construction and proves nothing.

## 0086 — Migration pairing and docs

### The delta (task `8601`)

```
fsm machine test <new.json> --cases <old-cases.json> --against <old.json>
```

The new definition must declare `supersedes` naming the old one's `machine_id`, or the command refuses — the mapping is what makes the comparison meaningful, and without it this is two unrelated machines.

Each case is run twice and reported as one of three outcomes: **unchanged**, **changed** with the fields that moved, or **refused** where the new definition rejects a script the old one accepted. Expected configurations are translated through the `supersedes` mapping before comparison, using the same mapping code plan 0011's migration uses — not a second copy, so the report cannot disagree with what an actual migration would do.

This is a **report**, and its exit code is zero when the run completes. A corrected machine usually changes behaviour on purpose; making the delta a gate would be wrong, and a gate with an override is a gate everyone overrides. The author reads it, and the migration is reviewed rather than hoped for.

### The documentation (task `8602`)

`docs/EXAMPLES.md` gains a worked case file for a committed example machine, in the repository's neutral vocabulary. `docs/EMBEDDING.md` gains the format and the library entry point. The `supersedes` delta gets its own short section, because it is the reason to keep case files rather than write them once.
