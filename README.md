# fsm

`fsm` is a deterministic, auditable statechart engine that gives LLMs a workflow substrate: the model translates intent into machines, the engine guarantees the semantics — one event, one transition, a tamper-evident journal, and errors that teach the fix.

## 60-second demo

```
cargo install --path crates/fsm-cli --locked
fsm validate examples/expense_approval.json
fsm machine add examples/expense_approval.json
fsm instance new expense_approval --request-id demo
fsm instance send inst-demo submit --payload '{"amount":"10.00"}' --request-id demo-submit
fsm instance history inst-demo
```

## Install

```
cargo install --path crates/fsm-cli --locked
```

## Embedding it

`fsm` is three crates. Depend on the one you need — the CLI binary is not a
library:

| Crate | Use it for |
|---|---|
| `fsm-core` | the pure engine: parse, compile, analyse, step. No I/O, no clock. |
| `fsm-store` | the durable journal-backed store, if you want ours rather than yours. |
| `fsm-cli` | the `fsm` binary and MCP server. Not a supported library dependency. |

```toml
fsm-core = { git = "<repository url>", tag = "vX.Y.Z" }
```

See [docs/EMBEDDING.md](docs/EMBEDDING.md) for the library loop, the `Store`
concurrency contract with measured latencies, and the guarantees an embedder
should know; [docs/API-POLICY.md](docs/API-POLICY.md) for semver and formats.

## MCP setup

Claude Code:

```
claude mcp add fsm -- fsm serve
```

Claude Desktop `mcpServers` JSON:

```
{"mcpServers":{"fsm":{"command":"fsm","args":["serve"]}}}
```

## Guarantees

| Guarantee | What it means |
|---|---|
| total order | journal records are a single sequence |
| one-event-one-transition | at most one transition fires per send |
| pure core | `fsm-core` has no I/O, clock, or HashMap |
| no floats | no `f32`/`f64` in the engine |
| explicit rounding | decimal scale and mode are always stated |
| deterministic choice | document order plus innermost-first |
| atomic transitions | step is pure; the shell commits Applied only |
| content-addressed definitions | `machine_id` is a hash of the spec |
| deterministic identifiers | ids derive from content and the injected clock |
| exact idempotency | the same `request_id` never applies twice; reusing it for different content is refused, not replayed |
| tamper-evident history | hash-chained, fsynced records |
| time as data | the clock is injected; time is a payload field |
| bounded computation | a shared eval budget per event |
| platform independence | no OS-specific semantics in core |
| crash safety | torn tails are classified and repaired |
| auditable implementation | zero dependencies, no unsafe |

Honest non-claims: this is a **single-node** single-writer engine. There is no HA/replication, no real-time deadline, and the throughput ceiling is a feature.

See [docs/SPEC.md](docs/SPEC.md), [docs/EXAMPLES.md](docs/EXAMPLES.md), [docs/EMBEDDING.md](docs/EMBEDDING.md), [docs/API-POLICY.md](docs/API-POLICY.md), and [docs/RELEASE.md](docs/RELEASE.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
