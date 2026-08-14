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
4. Add inline unit tests: longest-prefix dispatch, both flag forms, unknown-flag suggestion, help rendering containing every registered path, and `read_input` for all three forms.

- **Done when:** inline `args` tests prove dispatch, flag parsing, suggestions, and table-rendered help listing every command, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
