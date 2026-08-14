# Architecture — Plan 0001

> The concrete deltas, by symbol.

## 0001 — Scaffold

Root `Cargo.toml` is a `[workspace]` with `members = ["crates/fsm-core", "crates/fsm-cli"]`, `resolver = "3"`, and a `[workspace.package]` table pinning `edition = "2024"`, `rust-version = "1.89"`, `version = "X.Y.Z"`, `license = "MIT OR Apache-2.0"`. A `[workspace.lints.rust]` table sets `unsafe_code = "forbid"`. `rust-toolchain.toml` pins `channel = "1.89.0"` with `components = ["clippy", "rustfmt"]`.

Member manifests are written once here and never edited by a later task (zero dependencies makes this final):

- `crates/fsm-core/Cargo.toml`: `name = "fsm-core"`, workspace-inherited package fields, `[lints] workspace = true`, **no `[dependencies]` table at all**.
- `crates/fsm-cli/Cargo.toml`: `name = "fsm-cli"`, `[[bin]] name = "fsm", path = "src/main.rs"`, `[dependencies] fsm-core = { path = "../fsm-core" }` (the only dependency edge in the workspace), `[lints] workspace = true`, and `[lints.clippy] print_stdout = "deny"`, `print_stderr = "deny"` (the transport and the stderr logger will carry scoped `#[expect]` attributes in plan 0006; the plain skeleton in workstream 0005 uses `#[expect]` the same way).

`crates/fsm-core/src/lib.rs` opens with `#![forbid(unsafe_code)]` and crate docs stating the purity rule ("this crate performs no I/O, reads no clock, and holds no platform-dependent state; `src/` must not name `std::fs`, `std::net`, `std::time`, `f32`, `f64`, or `HashMap`"), then declares exactly the plan-0001 modules — `pub mod json; pub mod sha256; pub mod decimal; pub mod canon; pub mod ident; pub mod limits; pub mod error;` — with stub files so the workspace compiles: `src/json/{mod,value,parse,write}.rs` (`mod.rs` declares `pub mod value; pub mod parse; pub mod write;`), `src/sha256.rs`, `src/decimal/{mod,u256}.rs` (`mod.rs` declares `pub mod u256;` alongside its own stub), `src/canon.rs`, `src/ident.rs`, `src/limits.rs`, `src/error.rs`. Stubs are empty except module docs; later tasks fill them without touching `lib.rs` again.

`crates/fsm-cli` carries **both a library and the binary target** (cargo auto-detects `src/lib.rs` alongside the `[[bin]]`; no manifest change) because the crate's integration tests — and later the fuzz side-crate — must import its code (`fsm_cli::mcp::serve::serve` runs over in-memory buffers in tests). `src/lib.rs` opens with `#![forbid(unsafe_code)]` and declares `pub mod mcp;`; `src/mcp/mod.rs` declares `pub mod jsonrpc; pub mod serve;` with empty stubs. `src/main.rs` stays thin: `fn main()` reads `std::env::args`, dispatches `Some("serve")` to `fsm_cli::mcp::serve::run()` (a stub returning "not yet implemented" to stderr, exit 2, until workstream 0005 fills it), anything else prints a one-line usage to stderr and exits 2. Later plans add their module declarations to this `lib.rs` (journal_io/store/clock in plan 0004; args/render/cli in plan 0005), never to `main.rs`.

Policy gates (task `0102`) — machine-checked enforcement, so the rules cannot decay into convention:

- `crates/fsm-core/clippy.toml`: `disallowed-types` listing `std::collections::HashMap`, `std::collections::HashSet`, `std::time::SystemTime`, `std::time::Instant` (reason strings say "fsm-core is pure and platform-deterministic; use BTreeMap/BTreeSet; time enters as data"); `disallowed-methods` for `std::mem::transmute` is unnecessary under `forbid(unsafe_code)` and is omitted.
- `crates/fsm-cli/tests/policy.rs`: walks `../fsm-core/src` with `std::fs::read_dir` (test code is not bound by the core purity rule), scans each `.rs` file line-by-line, and fails naming file and line if any line outside a `//` comment contains one of the banned tokens `f32`, `f64`, `SystemTime`, `Instant`, `HashMap`, `HashSet`, `std::fs`, `std::net`, `std::process`, `rand`, `unsafe`. Token scan is a plain substring check on non-comment text — crude by design, with a `POLICY_ALLOW` marker comment as the documented escape hatch (zero uses expected; the test also fails if the marker appears without a justification suffix).
- `crates/fsm-cli/tests/zero_deps.rs`: runs `cargo metadata --format-version 1 --locked` via `std::process::Command`, parses the output with `fsm_core::json::parse`, and asserts the package set is exactly `{"fsm-core", "fsm-cli"}` — any third-party crate anywhere in the graph fails the build. (This test is itself the first consumer of our JSON parser, so it lands after workstream 0002 in the DAG.)

## 0002 — JSON

`crates/fsm-core/src/json/value.rs`:

- `pub enum Value { Null, Bool(bool), Num(String), Str(String), Arr(Vec<Value>), Obj(BTreeMap<String, Value>) }` — `Num` holds the raw number token verbatim; no float type exists anywhere in the crate. Accessors: `as_str`, `as_obj`, `as_arr`, `as_bool`, `get(&str)`, plus `is_*` predicates. `PartialEq/Eq/Clone/Debug` derived.

`crates/fsm-core/src/json/parse.rs`:

- `pub struct JsonLimits { pub max_depth: u32, pub max_bytes: usize }` with `pub const DEFAULT: JsonLimits { max_depth: 64, max_bytes: 16 * 1024 * 1024 }`.
- `pub fn parse(input: &[u8], limits: &JsonLimits) -> Result<Value, JsonError>` — recursive descent over bytes with an explicit depth counter. Rules, each with a dedicated `JsonErrorKind`: input must be UTF-8; exactly one top-level value with only whitespace after it; **duplicate object keys are an error** (never last-wins); number tokens must match the RFC 8259 grammar and are captured verbatim into `Value::Num`; string escapes limited to `\" \\ \/ \b \f \n \r \t \uXXXX`; `\uXXXX` surrogate pairs must be correctly paired (lone surrogates are an error); raw control bytes < 0x20 inside strings are an error; depth beyond `max_depth` and input beyond `max_bytes` are errors.
- `pub struct JsonError { pub kind: JsonErrorKind, pub offset: usize, pub message: String }`.

Fixtures land first: `crates/fsm-core/tests/fixtures/json/` with `y_*.json` (must parse) and `n_*.json` (must fail) cases — a curated ~40-case subset in the spirit of JSONTestSuite (deep nesting at the limit and one past it, `1e309`-style extreme numbers preserved as tokens, lone surrogate, unpaired `\uD800`, duplicate keys, trailing garbage, raw control characters, BOM rejection) plus our stricter verdicts written into the filenames. `crates/fsm-core/tests/json_corpus.rs` iterates the directory and asserts each verdict.

Canonical writer (task `0202`):

`crates/fsm-core/src/json/write.rs`:

- `pub fn write_canonical(v: &Value, out: &mut Vec<u8>)` — the **only** JSON serializer in the system (FSM-CJSON): single line, no insignificant whitespace, object keys in byte-lexicographic order (free via `BTreeMap` iteration), strings escaped minimally (`\" \\ \n \r \t \b \f`, other C0 as lowercase `\u00xx`, everything else raw UTF-8), `Num` tokens written verbatim.

`crates/fsm-core/src/canon.rs`:

- `pub fn canon_bytes(v: &Value) -> Vec<u8>` — wrapper over `write_canonical`.
- `pub fn is_canonical(bytes: &[u8], limits: &JsonLimits) -> Result<bool, JsonError>` — parse, re-serialize, byte-compare; the storage verifier in plan 0004 is built on this.

`crates/fsm-core/tests/canon_golden.rs` + `tests/fixtures/canon/*.txt` fixtures: input-JSON → expected-canonical-bytes pairs covering key reordering, escape normalization (the writer re-escapes the *decoded* string minimally: a backslash-u sequence for a printable character such as `A` collapses to the raw character, an escaped forward slash becomes a bare `/`, while quote, backslash, and control characters stay escaped — the fixture set pins this exactly), Unicode passthrough, nested empties. A round-trip property test: for every `y_*` corpus fixture, `parse ∘ write_canonical ∘ parse` is identity and a second canonicalization is byte-identical to the first.

## 0003 — Hashing

`crates/fsm-core/src/sha256.rs`:

- `pub struct Sha256 { ... }` with `new() / update(&[u8]) / finalize() -> [u8; 32]`, plus `pub fn sha256(bytes: &[u8]) -> [u8; 32]` and `pub fn to_hex(&[u8]) -> String` / `pub fn from_hex(&str) -> Option<Vec<u8>>` (lowercase hex only). Pure FIPS 180-4: message schedule, 64 rounds, length padding; no unsafe, no SIMD.

Fixtures first: `crates/fsm-core/tests/fixtures/sha256/vectors.txt` — NIST byte-oriented vectors (empty string, `"abc"`, the two-block `"abcdbcde..."` case, 448/896-bit boundary messages, a 1,000,000 × `'a'` case exercised via `update` in chunks) with expected digests. `crates/fsm-core/tests/sha256_golden.rs` asserts one-shot and incremental (varied chunk sizes) agree with every vector.

## 0004 — Decimal

`crates/fsm-core/src/decimal/mod.rs`:

- `pub struct Dec { mant: i128, scale: u8 }` — value = mant·10⁻ˢᶜᵃˡᵉ, **not normalized** (scale is semantic: `1.50` stays `{150, 2}`). Constants `MAX_SCALE: u8 = 12`, `MAX_MANT: i128 = 10i128.pow(38) - 1`.
- `pub enum RoundMode { Down, Up, Floor, Ceiling, HalfUp, HalfDown, HalfEven }` — exactly Python `decimal`'s set minus `ROUND_05UP`.
- `pub enum DecError { Overflow, DivZero, ScaleCap, Parse(String) }`.
- Operations: `checked_add/checked_sub` (rescale the smaller-scale operand by checked ×10^Δ to `max(s1, s2)`, then checked add; any overflow → `Overflow` — the true result is unrepresentable); `checked_mul` (result scale `s1+s2`; caller enforces the ≤12 cap statically in plan 0002's typechecker, this module returns `ScaleCap` dynamically); `cmp(&self, other) -> Ordering` — total, **via u256 widening** (naive rescale of a 38-digit mantissa by 10¹² overflows i128; sign-compare first, then align magnitudes in u256); `round(self, scale: u8, mode: RoundMode)` (upscale = exact checked widen; downscale = divide mantissa by 10^Δ rounding by (quotient, remainder, half-divisor) per mode, `HalfEven` on quotient parity); `div(a, b, scale, mode)` — **the correctly-rounded value of the exact rational a/b at the target scale, never double-rounded**: compute `n = a.mant · 10^k` with `k = scale − a.scale + b.scale`; `k ≥ 0` widens `n` in u256 and long-divides by `|b.mant|`; `k < 0` folds `10^|k|` into the divisor; round the integer quotient by remainder-vs-divisor per mode with the combined sign; bound-check the result.
- `parse(&str, scale: u8) -> Result<Dec, DecError>` — grammar `-?(0|[1-9][0-9]*)(\.[0-9]+)?`; fewer fraction digits than `scale` widen exactly; more are `Parse` (never rounded); `-0` normalizes to sign-dropped zero. `format(&self) -> String` — canonical form with exactly `scale` fraction digits, no exponent, no `+`.

`crates/fsm-core/src/decimal/u256.rs`:

- `pub struct U256 { hi: u128, lo: u128 }` with `from_u128`, `checked_mul_pow10(u32)`, `cmp`, and `div_rem_u128(self, d: u128) -> (U256, u128)` — schoolbook long division, division by a u128 divisor only (all our divisors are `|mant| ≤ MAX_MANT` or a power of ten times that, folded before widening). ~150 lines, exhaustively unit-tested against u128-representable cases.

Fixtures first: `crates/fsm-core/tests/fixtures/decimal/starter_vectors.jsonl` — ~100 hand-authored lines `{"op":"add|sub|mul|cmp|round|div|parse|format", "a":"...", "b":"...", "scale":N, "mode":"half_even", "expect":"..."}` or `"expect_err":"overflow|div_zero|scale_cap|parse"`, covering every mode at exact ties (`.5` remainders both signs), mantissa at ±(10³⁸−1), add-alignment overflow at the boundary, cmp pairs whose naive alignment would overflow i128, `1/3` and `1/7` at scales 0 and 12, exact divisions, the `k < 0` division path, and `-0.00` normalization. `crates/fsm-core/tests/decimal_golden.rs` runs every line of every `*.jsonl` file in that directory (so the generated file from task 0402 is picked up without edits).

Differential harness (task `0402`):

`tools/gen_decimal_vectors.py` — Python 3 stdlib only, run manually and in CI, never a build dependency:

- Implements the **independent oracle**: exact rational arithmetic over Python ints (`a·10^k // b` with explicit remainder-based rounding per mode implemented in integer space — deliberately *not* `decimal.quantize`, which could itself double-round), plus `decimal` with a 60-digit context as a second cross-check where applicable.
- Deterministic case generation (fixed seed, no wall clock): boundary mantissas, all mode×tie combinations, ~5,000 seeded random op cases across scales 0..12.
- Writes `crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl` sorted and byte-stable: running the generator twice must produce identical bytes (asserted in the task's done-when).

## 0005 — MCP Skeleton

`crates/fsm-cli/src/mcp/jsonrpc.rs`:

- `pub enum Incoming { Request { id: Value, method: String, params: Option<Value> }, Notification { method: String, params: Option<Value> } }`.
- `pub fn parse_line(line: &str) -> Result<Incoming, WireError>` — parses via `fsm_core::json::parse`; a JSON **array** at top level is `WireError::Batch` (JSON-RPC batching was removed in MCP 2025-06-18; we reject it under every negotiated revision); missing `jsonrpc: "2.0"` or `method` is `WireError::Invalid`.
- Response builders: `result_response(id, Value) -> Value`, `error_response(id, code, message) -> Value`; error code constants `PARSE_ERROR = -32700`, `INVALID_REQUEST = -32600`, `METHOD_NOT_FOUND = -32601`, `INVALID_PARAMS = -32602`, `INTERNAL_ERROR = -32603`, `NOT_INITIALIZED = -32002`.

`crates/fsm-cli/src/mcp/serve.rs`:

- `pub fn run() -> std::io::Result<()>` locks stdin/stdout and delegates to `pub fn serve(input: impl BufRead, output: impl Write) -> std::io::Result<()>` — fully testable over in-memory buffers.
- Loop: read one line (cap 16 MiB, over-cap → `-32700` with a message naming the cap); parse; dispatch. State machine: before `initialize`, only `initialize` and `ping` are served, anything else → `-32002`. `initialize` result: `protocolVersion` from the negotiation table (`2025-06-18` and `2025-03-26` and `2024-11-05` echo the client's version; anything else → `"2025-06-18"`), `capabilities: {"tools": {"listChanged": false}}`, `serverInfo: {"name": "fsm", "version": env!("CARGO_PKG_VERSION")}`. `notifications/initialized` and all unknown notifications are ignored. `ping` → `{}` at any stage. `tools/list` → one stub tool `{"name": "fsm_ping", "description": "Health check; returns pong.", "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}}`. `tools/call` with `name == "fsm_ping"` → `{"content": [{"type": "text", "text": "pong"}]}`; unknown tool → `-32602` with the valid tool names in the message. Unknown method → `-32601`. EOF → flush and `Ok(())`.
- `fn send_line(out: &mut impl Write, v: &Value) -> std::io::Result<()>` — the single stdout chokepoint: serializes via `fsm_core::canon::canon_bytes`, `debug_assert!` no `\n` byte in the payload, writes bytes + `\n`, flushes. All diagnostics go to stderr behind `FSM_LOG` (values `error|info|debug`, default `error`), via one `fn log(level, msg)` helper carrying the scoped `#[expect(clippy::print_stderr)]`.
- `main.rs`'s `serve` arm switches from the 0001 stub to `fsm_cli::mcp::serve::run()`.

Fixtures first: `crates/fsm-cli/tests/fixtures/transcripts/skeleton.in.jsonl` / `skeleton.out.jsonl` — a recorded session authored from the MCP 2025-06-18 spec: `ping` before init, a request before init (expects `-32002`), `initialize` (client offers `2025-11-25`, expects `2025-06-18` back), `notifications/initialized`, `tools/list`, `tools/call fsm_ping`, a batch array (expects `-32600`), an unknown method (expects `-32601`), malformed JSON (expects `-32700`). `crates/fsm-cli/tests/mcp_skeleton.rs` feeds the `.in` file through `serve()` over buffers and byte-compares the full output stream to `.out`.
