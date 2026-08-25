---
id: handler-table-config
title: "Handler Table Config"
workstream: "0036"
kind: task
depends_on:
  - crate-scaffold-and-skeleton
gated: false
touches:
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/tests/config.rs
  - crates/fsm-execute/tests/fixtures/handlers/valid_min.json
  - crates/fsm-execute/tests/fixtures/handlers/dup_effect.json
  - crates/fsm-execute/tests/fixtures/handlers/bad_placeholder.json
  - crates/fsm-execute/tests/fixtures/handlers/empty_argv.json
  - crates/fsm-execute/tests/fixtures/handlers/bad_timeout.json
status: planned
merged_as: ""
---
# Handler Table Config

The handler table is the plan's security boundary: a single operator-owned `fsm.handlers/1` JSON file that closes the set of commands the executor can ever run, parsed with the workspace's own JSON (never a third-party deserializer) and fully validated at startup before any store is opened.

**Steps:**

1. Author fixtures first: `valid_min.json` (one well-formed handler with `{instance}` and `{project}` placeholders), `dup_effect.json`, `bad_placeholder.json` (unbalanced `{` and a `{bad name}`), `empty_argv.json`, `bad_timeout.json` (zero and missing).
2. Implement `HandlerSpec { effect, argv, timeout_ms, success_event: Option<String>, failure_event: Option<String> }` and `HandlerTable { handlers: BTreeMap<String, HandlerSpec> }` in `config.rs`, plus `HandlerTable::parse(src: &str) -> Result<HandlerTable, ExecError>` using `fsm_core::json::parse`.
3. Enforce structural validation per architecture §0036: exact `format: "fsm.handlers/1"`; `handlers` a non-empty array; each entry required-string `effect`, non-empty `argv` of strings, positive-integer `timeout_ms`; unique `effect` names; every `{placeholder}` well-formed (`[a-z_][a-z0-9_]*`, balanced braces, by scan not regex); `success_event`/`failure_event` non-empty strings when present. Each violation → `exec/config` with the offending handler index and field in `details`.
4. Implement `fn substitute(argv: &[String], args: &BTreeMap<String, Val>) -> Result<Vec<String>, ExecError>` replacing each `{name}` with the canonical string form of that arg (ints/decimals/bools/timestamps via `Val` rendering; strings verbatim); a placeholder naming an absent arg → run-level failure (return `exec/config` here with the arg name; the *runtime* absence is acked `failed` by workstream 0038).

**Tests:**

- Accept: `valid_min.json` parses; the handler's `argv`, `timeout_ms`, and absent `success_event`/`failure_event` round-trip.
- Reject each bad fixture with exactly `exec/config`, the offending handler index named in `details`, and the specific field (`effect` / `argv` / `timeout_ms` / placeholder) identified.
- Duplicate `effect` names → `exec/config` naming the duplicated name.
- Placeholder validation: `{ok_name_1}` accepted; `{Bad}`, `{with space}`, `{}`, `{unclosed`, and `stray}` each rejected by scan with the character offset in `details`.
- `substitute`: `{instance}` → the supplied string value; an int arg renders exactly (e.g. `42`); a missing arg name returns `exec/config` naming it; no shell metacharacter in a substituted value alters the argv length (a value with spaces stays one argv element).

- **Done when:** `cargo test -p fsm-execute --test config` passes every accept/reject row, substitution is shell-free and exact, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
