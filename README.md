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

## Running it unattended

The 60-second demo advances the workflow by hand. `fsm execute` does it for
you: it watches the outbox, runs an operator-configured table of handlers, and
acks each outcome into the journal, so a workflow triggered in a chat proceeds
gate to gate with nobody watching.

```
fsm execute --check --handlers examples/order_lifecycle.handlers.json
fsm execute --data-dir ./data --handlers examples/order_lifecycle.handlers.json &
fsm serve --read-only --data-dir ./data
```

The pairing is the point: only the executor writes, and `fsm serve --read-only`
lets the model watch its acks and transitions arrive live. See
[docs/EMBEDDING.md](docs/EMBEDDING.md#executing-workflows) for the handler-table
format and the three run modes.

## Install

```
cargo install --path crates/fsm-cli --locked
```

## Embedding it

`fsm` is four crates. Depend on the one you need — the CLI binary is not a
library:

| Crate | Use it for |
|---|---|
| `fsm-core` | the pure engine: parse, compile, analyse, step, and poll caller-timed deadlines. No I/O, no clock. |
| `fsm-store` | the durable journal-backed store, if you want ours rather than yours. |
| `fsm-execute` | the effect executor loop, if you are hosting it yourself rather than running `fsm execute`. |
| `fsm-cli` | the `fsm` binary and MCP server. Not a supported library dependency. |

```toml
fsm-core = { git = "https://github.com/koraytaylan/fsm", tag = "<release-tag>" }
```

Replace `<release-tag>` with an exact annotated tag from the repository's
Releases page; never substitute a branch.

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
| one-event-one-macrostep | at most one transition fires for the event you sent; the machine may then react to itself to quiescence, bounded, in the same atomic record |
| deterministic regions | parallel regions share one document-ordered global winner; no broadcast |
| explicit deadlines | caller-supplied time plus an explicit poll applies at most one due deadline |
| pure core | `fsm-core` has no I/O, clock, or HashMap |
| no floats | no `f32`/`f64` in the engine |
| explicit rounding | decimal scale and mode are always stated |
| deterministic choice | document order plus innermost-first |
| atomic transitions | core operations are pure; the shell mutates logical instance state only for Applied and journals other state-dependent outcomes |
| content-addressed definitions | `machine_id` is a hash of the spec |
| deterministic identifiers | ids derive from content and the injected clock |
| exact idempotency | the same `request_id` never applies twice; reusing it for different content is refused, not replayed |
| tamper-evident history | hash-chained, fsynced records |
| time as data | the clock is injected; event stamps and deadline polls are explicit inputs |
| bounded computation | a shared eval budget per create, event, deadline poll, or enabled-event scan; accepted definitions fit it by construction |
| platform independence | no OS-specific semantics in core |
| crash safety | torn tails are classified and repaired |
| auditable implementation | zero dependencies, no unsafe |
| unattended execution | `fsm execute` is a separate one-node process with **at-least-once** execution at the process boundary and exactly-once journaling: one ack per effect, whatever it survived |
| explicit composition | a child instance exists because a record says so, and its id is derived from its parent and slot — never allocated, never guessed |

Honest non-claims: this is a **single-node** single-writer engine. There is no
HA/replication or autonomous real-time scheduler; a deadline fires only when a
caller polls. The throughput ceiling is a feature. The executor inherits that
ceiling, and a handler killed mid-run is re-run by the next executor rather
than rolled back — model the undo as a compensating effect in the machine.
Reaction is bounded at 64 microsteps per event: a machine that needs more is
refused at run time, not truncated. Composition is single-store and
single-writer too — a parent and its children live in one journal — and a
signal reaches **exactly one** instance by design, because a query-targeted
delivery would match a different set on replay.

See [docs/SPEC.md](docs/SPEC.md), [docs/EXAMPLES.md](docs/EXAMPLES.md), [docs/EMBEDDING.md](docs/EMBEDDING.md), [docs/API-POLICY.md](docs/API-POLICY.md), and [docs/RELEASE.md](docs/RELEASE.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
