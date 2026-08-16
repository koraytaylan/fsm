# fsm-fuzz

Out-of-workspace cargo-fuzz crate. `libfuzzer-sys` lives only here.

```
cargo +nightly fuzz run json_parse
cargo +nightly fuzz run expr_parse
cargo +nightly fuzz run decimal_parse
cargo +nightly fuzz run canon_roundtrip
cargo +nightly fuzz run record_line
cargo +nightly fuzz run jsonrpc_loop
```

Seed each target from the owning module's committed fixtures:

| Target | Seeds |
|---|---|
| json_parse | `crates/fsm-core/tests/fixtures/json/` |
| expr_parse | `crates/fsm-core/tests/fixtures/expr/` |
| decimal_parse | `crates/fsm-core/tests/fixtures/decimal/` |
| canon_roundtrip | `crates/fsm-core/tests/fixtures/canon/` |
| record_line | `crates/fsm-core/tests/fixtures/records/` |
| jsonrpc_loop | `crates/fsm-cli/tests/fixtures/transcripts/` |

A crashing input is minimized and committed as a regression fixture in the owning module's corpus.
