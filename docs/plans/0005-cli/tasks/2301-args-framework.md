---
id: args-framework
title: "Args Framework"
workstream: "0023"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/lib.rs
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/src/render.rs
  - "crates/fsm-cli/src/cli/**"
status: planned
merged_as: ""
---
# Args Framework

One static table declares every command's path, positionals, flags, and help — dispatch and help text both render from it, so documentation cannot drift from behavior, and each command module fills only its own `SPECS` constant so later tasks never edit `args.rs`.

**Steps:**

1. Implement `CmdSpec`, `Args`, `Ctx`, `read_input` (`-` stdin, `@file` contents, inline otherwise), and `dispatch` (longest-prefix match, `--flag=v`/`--flag v`/switches, unknown-command/flag usage errors with nearest-match suggestions, table-rendered `-h`/`--help` through the single help printer) in `crates/fsm-cli/src/args.rs`, assembling `all_specs()` from the five `cli::*::SPECS` constants plus the inline `serve` spec.
2. Create `crates/fsm-cli/src/cli/mod.rs` declaring `offline`, `machine`, `instance`, `ops`, and `diagram`, each stub exporting an empty `pub const SPECS: &[CmdSpec]`, and create `crates/fsm-cli/src/render.rs` as a stub.
3. Add `pub mod args; pub mod render; pub mod cli;` to `crates/fsm-cli/src/lib.rs` and rework `crates/fsm-cli/src/main.rs` to the thin `fsm_cli::args::dispatch(std::env::args().collect())` with `serve` routed through the table.
4. Write the inline test module encoding exactly the inventory under **Tests** (driving `dispatch` and the parsers directly over a scratch spec table — no store, no binary spawn).

**Tests:**

- Inline in `args.rs` — longest-prefix dispatch: with specs for `machine add` and `machine analyze`, argv `machine analyze x` selects the analyze spec; bare `machine` is a usage error (exit 2) listing its subcommands.
- Flag forms: `--format=dot` and `--format dot` both populate `flags["format"] = "dot"`; a bare declared switch (`--json`) lands in `switches`; a flag missing its value is a usage error naming the flag.
- Suggestions: an unknown command (`machin add`) exits 2 suggesting `machine`; an unknown flag (`--jsno`) exits 2 suggesting `--json` — both through the shared nearest-match helper.
- Help-from-the-table (the docs-can't-drift assertion): the `-h` rendering contains every `path` registered in `all_specs()`, asserted by iterating the live table, not a hard-coded list — a spec added later without help coverage cannot pass.
- `read_input` three forms: `-` returns stdin contents (fed via the test harness), `@<tmpfile>` returns the file contents, a bare string returns itself; `@missing-file` returns the io `ErrorObj` naming the path.
- Positional arity: too few positionals for a spec → usage error naming the missing one; extras → usage error.
- Wiring: `serve` resolves through the table to the inline spec (asserted by dispatching to a stub `run` that records its call).

- **Done when:** inline `args` tests prove dispatch, flag parsing, suggestions, and table-rendered help listing every command, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
