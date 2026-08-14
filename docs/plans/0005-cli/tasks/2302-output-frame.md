---
id: output-frame
title: "Output Frame"
workstream: "0023"
kind: task
depends_on:
  - args-framework
gated: false
touches:
  - crates/fsm-cli/src/render.rs
status: planned
merged_as: ""
---
# Output Frame

All output flows through one frame: a single renderer from structured results to human text (reused verbatim for MCP text blocks in plan 0006), errors always to stderr with their hint, one exit-code table, `--json` emitting exact canonical structured bytes, and flag > env > platform-default configuration.

**Steps:**

1. Implement `render_human(&Value) -> String` in `crates/fsm-cli/src/render.rs` — aligned key-value blocks, compact list tables, indented traces — as the only structured-to-human renderer in the system.
2. Implement `emit_success` (stdout: human, or exact canonical structured bytes under `--json`) and `emit_error` (always stderr: canonical error envelope under `--json`, otherwise code, message, path, caret-marked span excerpt, and hint), with the single exit-code table mapping code namespaces to 0/1/2/3/4/5 per architecture.
3. Implement configuration resolution: flag > env (`FSM_DATA_DIR`, `FSM_LOG`, `NO_COLOR`) > `default_data_dir()` (std-only XDG/macOS/Windows detection), and `default_request_id()` from `clock::now_ms()` printed with every command that used it.
4. Write the inline test module encoding exactly the inventory under **Tests**.

**Tests:**

- Inline in `render.rs` — exit-code table, one asserted row per namespace: `run/not_enabled` → 1, `def/shadowed` → 1, `expr/type_mismatch` → 1, `req/field_scale` → 1, an `args` usage error → 2, `req/machine_not_found` → 3, a `store/*` chain-integrity code → 4, `internal/budget` → 5, `io/*` → 5 — a new namespace slipping through the map is a named test failure, not a silent 1.
- Error rendering (fixed sample error with a span): stderr text contains the code, the message, the path, the source excerpt with a caret line whose `^^^` columns align under the span (asserted against a pinned multi-line expected string), and the hint on its own labeled line.
- `--json` byte-exactness: `emit_success` under `--json` writes exactly `canon_bytes(result)` plus one trailing newline — byte-compared; `emit_error` under `--json` writes exactly the canonical error envelope to stderr.
- Stream discipline: `emit_success` writes nothing to stderr; `emit_error` writes nothing to stdout — asserted over capture buffers.
- Color: with `NO_COLOR` set or `Ctx.color = false`, the human rendering contains no ANSI escape bytes; a fixed sample with color on does (and never under `--json`).
- Config precedence (env-scoped test): flag beats `FSM_DATA_DIR`; `FSM_DATA_DIR` beats `default_data_dir()`; `default_data_dir()` ends in `fsm` on every platform branch.
- `default_request_id`: under `FSM_CLOCK_MS`, two calls yield distinct, deterministic ids (`req-<ms>-<counter>`), identical across two runs of the test.

- **Done when:** inline render tests prove the exit-code table, stderr error rendering with hint and span, `--json` byte-exactness, and config precedence, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
