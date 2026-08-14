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
4. Add inline unit tests: exit-code mapping per namespace, span caret rendering, `--json` byte-exactness for a fixed result, and data-dir precedence with scoped env overrides.

- **Done when:** inline render tests prove the exit-code table, stderr error rendering with hint and span, `--json` byte-exactness, and config precedence, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
