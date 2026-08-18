# Release checklist

Every line is a runnable command or is tagged `manual:`.

## Version stamping

- `grep '^version' Cargo.toml` prints the workspace version (must match `fsm version` and `serverInfo.version`).
- `fsm version`
- `manual:` add a CHANGELOG line for the stamped version.
- `manual:` tag the release commit `vMAJOR.MINOR.PATCH` and push the tag — it is
  the artifact library consumers pin. Tags are never moved or deleted; see
  `docs/API-POLICY.md`.

## Install verification

- `cargo install --path crates/fsm-cli --locked`
- `fsm version`
- `fsm docs spec`

## Library embedding target

- `cargo test -p fsm-embed-acceptance` — the downstream loop: `parse → compile →
  step → completeness_matrix` plus a persistence round-trip, driven from a crate
  that depends on `fsm-core` alone.
- `cargo tree -p fsm-embed-acceptance` shows `fsm-core` and nothing else — an
  embedder must never need `fsm-store` or `fsm-cli` for the core loop.
- `manual:` if the `fsm-core` or `fsm-store` public API changed, check the change
  against the semver rules in `docs/API-POLICY.md` and pick the version bump
  accordingly.
- `manual:` re-run `cargo test --release -p fsm-store --test append_latency --
  --ignored --nocapture` and update the measured table in `docs/EMBEDDING.md` if
  the numbers have moved materially.

## Host matrix

- `manual:` Claude Code: connect, list all 13 tools, run the golden loop end-to-end.
- `manual:` Claude Desktop: connect, list all 13 tools, run the golden loop end-to-end.
- `manual:` MCP Inspector: connect, list all 13 tools, run the golden loop end-to-end.

## Live-model acceptance

- `manual:` an LLM authors and drives the case-review machine from a natural-language brief, unaided, in a bounded number of tool calls.

## Regeneration checks

- `python3 tools/gen_decimal_vectors.py /tmp/dec-a.jsonl && python3 tools/gen_decimal_vectors.py /tmp/dec-b.jsonl && cmp /tmp/dec-a.jsonl /tmp/dec-b.jsonl && cmp /tmp/dec-a.jsonl crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl`
- `cargo test -p fsm-cli --test mcp_full`
- `cargo metadata --manifest-path fuzz/Cargo.toml --format-version 1`
- `manual:` replay `docs/EXAMPLES.md` transcripts under `FSM_CLOCK_MS` and compare output.

## initial release definition of done

initial release is done when version stamping, install verification, the library embedding target, the host matrix, live-model acceptance, and regeneration checks are all complete, and `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt --check` is green.

There are three supported consumers, and all three are in that list: the CLI, the
MCP hosts, and a Rust program embedding `fsm-core` (optionally `fsm-store`). A
release that satisfies only the first two is not done.
