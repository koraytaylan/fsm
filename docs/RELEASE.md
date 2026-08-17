# Release checklist

Every line is a runnable command or is tagged `manual:`.

## Version stamping

- `grep '^version' Cargo.toml` prints the workspace version (must match `fsm version` and `serverInfo.version`).
- `fsm version`
- `manual:` add a CHANGELOG line for the stamped version.

## Install verification

- `cargo install --path crates/fsm-cli --locked`
- `fsm version`
- `fsm docs spec`

## Host matrix

- `manual:` Claude Code: connect, list all 13 tools, run the golden loop end-to-end.
- `manual:` Claude Desktop: connect, list all 13 tools, run the golden loop end-to-end.
- `manual:` MCP Inspector: connect, list all 13 tools, run the golden loop end-to-end.

## Live-model acceptance

- `manual:` an LLM authors and drives the case-review machine from a natural-language brief, unaided, in a bounded number of tool calls.

## Regeneration checks

- `python3 tools/gen_decimal_vectors.py && python3 tools/gen_decimal_vectors.py && git diff --exit-code crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl`
- `cargo test -p fsm-cli --test mcp_full`
- `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1`
- `manual:` replay `docs/EXAMPLES.md` transcripts under `FSM_CLOCK_MS` and compare output.

## initial release definition of done

initial release is done when version stamping, install verification, the host matrix, live-model acceptance, and regeneration checks are all complete, and `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt --check` is green.
