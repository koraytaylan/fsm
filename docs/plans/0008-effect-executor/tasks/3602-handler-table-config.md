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
  - crates/fsm-execute/tests/fixtures/handlers/valid_advance.json
  - crates/fsm-execute/tests/fixtures/handlers/dup_effect.json
  - crates/fsm-execute/tests/fixtures/handlers/bad_placeholder.json
  - crates/fsm-execute/tests/fixtures/handlers/empty_argv.json
  - crates/fsm-execute/tests/fixtures/handlers/bad_timeout.json
  - crates/fsm-execute/tests/fixtures/handlers/bad_advance.json
status: planned
merged_as: ""
---
# Handler Table Config

The handler table is the plan's security boundary: a single operator-owned `fsm.handlers/1` JSON file that closes the set of commands the executor can ever run, parsed with the workspace's own JSON (never a third-party deserializer) and fully validated at startup before any store is opened.

**Steps:**

1. Author fixtures first, in the repo's neutral business-process vocabulary — no cloud-vendor or product names: `valid_min.json` (one well-formed handler with `{order_id}` and `{reviewer}` placeholders, no advance), `valid_advance.json` (the same handler plus `on_ok` with `event`/`payload`/`stamps` and `on_failed` with `event` only), `dup_effect.json`, `bad_placeholder.json` (unbalanced `{` and a `{bad name}`), `empty_argv.json`, `bad_timeout.json` (zero and missing), `bad_advance.json` (`on_ok` with an empty `event`, and an `on_ok` whose `payload` is an array rather than an object).
2. Implement `HandlerSpec { effect, argv, timeout_ms, on_ok: Option<Advance>, on_failed: Option<Advance> }`, `Advance { event: String, payload: Value, stamps: Vec<String> }`, and `HandlerTable { handlers: BTreeMap<String, HandlerSpec> }` in `config.rs`, plus `HandlerTable::parse(src: &str) -> Result<HandlerTable, ExecError>` calling `fsm_core::json::parse(src.as_bytes(), &JsonLimits::DEFAULT)`.
3. Enforce structural validation per architecture §0036: exact `format: "fsm.handlers/1"`; `handlers` a non-empty array; each entry required-string `effect`, non-empty `argv` of strings, positive-integer `timeout_ms`; unique `effect` names; every `{placeholder}` well-formed (`[a-z_][a-z0-9_]*`, balanced braces, by scan not regex); `on_ok`/`on_failed` when present are objects with a non-empty string `event`, an optional `payload` **object** defaulting to `{}`, and an optional `stamps` array of non-empty strings defaulting to `[]`. Each violation → `exec/config` with the offending handler index and field in `details`.
4. Implement `fn substitute(argv: &[String], args: &BTreeMap<String, Val>) -> Result<Vec<String>, ExecError>` replacing each `{name}` with `fsm_core::replay::ctx_val_string` of that arg — the workspace's canonical `Val` rendering, so ints, decimals, bools, and timestamps are exact and strings are verbatim; a placeholder naming an absent arg → `exec/config` naming it (the *runtime* absence is acked `failed` by workstream 0038).

**Tests:**

- Accept: `valid_min.json` parses; the handler's `argv`, `timeout_ms`, and absent `on_ok`/`on_failed` round-trip. `valid_advance.json` round-trips `on_ok.event`, `on_ok.payload`, `on_ok.stamps`, and an `on_failed` whose payload and stamps take their defaults.
- Reject each bad fixture with exactly `exec/config`, the offending handler index named in `details`, and the specific field (`effect` / `argv` / `timeout_ms` / `on_ok` / placeholder) identified.
- Duplicate `effect` names → `exec/config` naming the duplicated name.
- Placeholder validation: `{ok_name_1}` accepted; `{Bad}`, `{with space}`, `{}`, `{unclosed`, and `stray}` each rejected by scan with the character offset in `details`.
- `substitute`: `{order_id}` → the supplied string value; an int arg renders exactly (e.g. `42`) and a decimal keeps its scale; a missing arg name returns `exec/config` naming it; no shell metacharacter in a substituted value alters the argv length (a value containing spaces, `;`, and `$(…)` stays exactly one argv element).

- **Done when:** `cargo test -p fsm-execute --test config` passes every accept/reject row including the advance-block rows, substitution is shell-free and exact, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
