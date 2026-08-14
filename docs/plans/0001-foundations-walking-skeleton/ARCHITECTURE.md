# Architecture — Plan 0001

> The concrete deltas, by symbol.

## Implementer orientation

Read this before your first task. The workflow is identical for every task in this plan:

1. Read your task file top to bottom, then only the parts of this document your workstream covers. Everything is decided here — if you find yourself making a design choice, you have missed a sentence; re-read before improvising.
2. Fixtures first, always: commit the vectors/goldens/corpus your task names before writing implementation code. They are the executable definition of done — when they pass, you are done; do not "improve" beyond them.
3. Your task's **Tests:** block is the complete acceptance inventory — implement every listed case; add more if you find a gap, never fewer. The command named in the Done-when is what runs them.
4. Stay inside your task's `touches` list. Needing another file is a signal you misread the design, not a reason to edit it.
5. Run the gates locally before every commit: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`. A red gate is never someone else's flake — this workspace has zero dependencies and deterministic tests.
6. Write the obvious version. Determinism and reviewability beat cleverness everywhere here; where a trick is genuinely needed, this document names it — and if it doesn't, don't use one.
7. When a golden or byte-comparison test fails, fix the code to match the fixture — never the fixture to match the code — unless the fixture demonstrably contradicts this document; then say so in your commit message.
8. The purity gates are mechanical allies: when clippy rejects `HashMap` or the policy test flags `f64`, switch to `BTreeMap`/decimals — never fight or silence a gate.

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

Three tasks, ordered so the fiddly scalar work is finished and vector-proven before any structure parsing exists.

Value model and scalar helpers (task `0201`), `crates/fsm-core/src/json/value.rs` and `parse.rs`:

- `pub enum Value { Null, Bool(bool), Num(String), Str(String), Arr(Vec<Value>), Obj(BTreeMap<String, Value>) }` — `Num` holds the raw number token verbatim; no float type exists anywhere in the crate. Accessors: `as_str`, `as_obj`, `as_arr`, `as_bool`, `get(&str)`, plus `is_*` predicates. `PartialEq/Eq/Clone/Debug` derived.
- `pub(crate) fn unescape_string(raw: &str) -> Result<String, ScalarError>` — input is the escaped contents *between* the quotes (the parser scans to the closing quote with a trivial backslash-parity walk and hands over the slice). Algorithm: copy bytes until `\`; then match the next byte against the eight simple escapes; on `u`, read 4 hex digits → `cp`. If `cp` is in `[0xD800, 0xDBFF]` (high surrogate), the next input MUST be another backslash-u escape decoding to `lo` in `[0xDC00, 0xDFFF]`; combine as `0x10000 + ((cp − 0xD800) << 10) + (lo − 0xDC00)` and push that character. A high surrogate not followed this way, or a bare low surrogate, is the lone-surrogate error. Any other byte after `\` is an invalid-escape error.
- `pub(crate) fn check_number_token(tok: &str) -> bool` — the RFC 8259 grammar as a four-phase scan: (1) optional `-`; (2) integer part: a single `0`, or a nonzero digit followed by digits; (3) optional fraction: `.` followed by one or more digits; (4) optional exponent: `e|E`, optional sign, one or more digits; end of input required. No other forms (`+1`, `.5`, `1.`, `01`, hex, `NaN`, `Infinity`).
- Fixtures first: `crates/fsm-core/tests/fixtures/json-scalars/{strings.txt, numbers.txt}` verdict files + `crates/fsm-core/tests/json_scalars.rs` asserting every line — every escape, correct and broken surrogate sequences, truncated `\u`, and the number-grammar accept/reject set.

Structural parser (task `0202`), `crates/fsm-core/src/json/parse.rs`:

- `pub struct JsonLimits { pub max_depth: u32, pub max_bytes: usize }` with `pub const DEFAULT: JsonLimits { max_depth: 64, max_bytes: 16 * 1024 * 1024 }`.
- `pub fn parse(input: &[u8], limits: &JsonLimits) -> Result<Value, JsonError>` — with scalars already solved, this is a plain recursive descent: validate UTF-8; skip whitespace; `parse_value` dispatches on the first byte (`{` object, `[` array, `"` string via the quote-scan + `unescape_string`, `-`/digit number via longest-token scan + `check_number_token` capturing the token verbatim, `t`/`f`/`n` literals); objects build `BTreeMap` and **reject duplicate keys**; an explicit depth counter enforces `max_depth`; exactly one top-level value with only whitespace after it; every error carries a byte offset and a dedicated `JsonErrorKind`.
- `pub struct JsonError { pub kind: JsonErrorKind, pub offset: usize, pub message: String }`.
- Fixtures first: `crates/fsm-core/tests/fixtures/json/` with `y_*.json` / `n_*.json` verdict cases (~40, JSONTestSuite-spirited: depth at the limit and past it, extreme number tokens preserved as strings, duplicate keys, trailing garbage, BOM rejection, raw control characters, full-document surrogate cases) + `crates/fsm-core/tests/json_corpus.rs` iterating the directory.

Canonical writer (task `0203`):

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

Five tasks, ordered so every hard step is a transcription of an algorithm given here verbatim, landing against vectors committed first. Shared bounds: `scale ≤ 12`, `|mant| ≤ 10³⁸−1`; `Dec { mant: i128, scale: u8 }`, value = mant·10⁻ˢᶜᵃˡᵉ, **not normalized** (scale is semantic: `1.50` stays `{150, 2}`).

u256 primitives (task `0401`), `crates/fsm-core/src/decimal/u256.rs` — `pub struct U256 { hi: u128, lo: u128 }` with exactly four operations:

- `from_u128(x)` and `cmp` (compare `hi`, then `lo`).
- `checked_mul_pow10(self, k: u32) -> Option<U256>` — a loop of `k` single ×10 steps; each step, verbatim:

  ```text
  lo_lo = lo & 0xFFFF_FFFF_FFFF_FFFF          # low 64 bits, as u128
  lo_hi = lo >> 64
  p0 = lo_lo * 10                              # fits u128
  p1 = lo_hi * 10 + (p0 >> 64)                 # fits u128
  new_lo = (p1 << 64) | (p0 & 0xFFFF_FFFF_FFFF_FFFF)
  carry  = p1 >> 64
  new_hi = hi.checked_mul(10)? .checked_add(carry)?   # None ⇒ overflow ⇒ whole call returns None
  ```

- `div_rem_u128(self, d: u128) -> (U256, u128)` (`d != 0`, caller-guaranteed) — restoring division, one bit per iteration, high bit first, verbatim:

  ```text
  q = U256::ZERO; rem: u128 = 0
  for i in (0..256).rev():
      bit    = (self >> i) & 1                 # via hi/lo indexing
      hi_bit = rem >> 127
      r2     = (rem << 1) | bit                # may wrap; see rule
      if hi_bit == 1 or r2 >= d:
          r2 = r2.wrapping_sub(d)              # exact: see invariant
          q.set_bit(i)
      rem = r2
  return (q, rem)
  ```

  Invariant argument (goes in the module docs, and is why `wrapping_sub` is exact): before each step `rem < d`, so the true shifted value `2·rem + bit < 2d`; when `hi_bit == 1` the true value is ≥ 2¹²⁸ > `u128::MAX` ≥ `d`, so the subtract branch is always correct there, and `true_value − d < d ≤ u128::MAX` always fits — the wrap-around arithmetic lands on exactly that difference.
- Tests are inline (the type is crate-internal): a seeded sweep cross-checking every operation against native u128 arithmetic where operands and results fit, both 2¹²⁸-crossing directions, and the worked division example below asserted digit for digit.

Representation and alignment (task `0402`), `crates/fsm-core/src/decimal/mod.rs`:

- `parse(&str, scale)` — grammar `-?(0|[1-9][0-9]*)(\.[0-9]+)?`; fewer fraction digits than `scale` widen exactly, more are an error (never rounded); `-0` normalizes to sign-dropped zero. `format` — exactly `scale` fraction digits, no exponent, no `+`.
- `checked_add/checked_sub`: rescale the smaller-scale operand by checked ×10^Δ to `max(s1, s2)`, then checked add — any overflow is `DecError::Overflow` (the true result is unrepresentable). `checked_mul`: result scale `s1+s2`, `ScaleCap` beyond 12, checked i128 multiply plus the mantissa bound.
- `cmp`: total value comparison across scales, **through u256** — comparing mant = 10³⁸−1 at scale 0 against any scale-12 value means aligning by ×10¹², whose product exceeds i128; sign-compare first, then align magnitudes in u256.
- Vectors first in `align_vectors.jsonl`; `tests/decimal_golden.rs` (authored here) runs every line of every `*.jsonl` in the fixtures directory, so later tasks add files without touching the test.

Rounding (task `0403`), same module — one shared decision function used by `round` here and `div` in the next task:

- `fn bump(mode, negative: bool, twice_rem_vs_divisor: Ordering, q_is_even: bool) -> bool`, the complete table (`r == 0` never reaches `bump`):

  | mode | bump? |
  |---|---|
  | `down` | never |
  | `up` | always |
  | `floor` | iff `negative` |
  | `ceiling` | iff not `negative` |
  | `half_up` | iff `2r ≥ d` (Greater or Equal) |
  | `half_down` | iff `2r > d` (Greater only) |
  | `half_even` | iff `2r > d`, or `2r == d` and `q` is odd |

- `round(x, S, mode)`: `S ≥ x.scale` → exact checked upscale (mode irrelevant, still total). `S < x.scale` → divide the magnitude by `10^(x.scale − S)` to get `(q, r)`, call `bump` with `(2r) cmp 10^Δ` and `q`'s parity, re-apply the sign.
- Worked set (encoded in `round_vectors.jsonl`): `2.345` (mant 2345, scale 3) to scale 2 → q=234, r=5, an exact tie: `down`→2.34, `up`→2.35, `floor`→2.34, `ceiling`→2.35, `half_up`→2.35, `half_down`→2.34, `half_even`→2.34 (q even). Negated: `floor`→−2.35, `ceiling`→−2.34, `half_even`→−2.34 — round the magnitude then re-apply the sign, except `floor`/`ceiling`, which follow the number line.

Division (task `0404`), same module — **the correctly-rounded value of the exact rational a/b at scale S, never double-rounded**:

- `div(a, b, S, mode)`: `b` zero → `DivZero`. Compute `k = S − a.scale + b.scale`.
  - `k ≥ 0`: `n = U256::from(|a.mant|).checked_mul_pow10(k)` (cannot fail: n ≤ 10³⁸·10²⁴ = 10⁶² < 2²⁵⁶), then `(q, r) = n.div_rem_u128(|b.mant|)` (divisor ≤ 10³⁸−1 < 2¹²⁷, always fits).
  - `k < 0`: fold the power into the divisor, `d = |b.mant| · 10^|k|`. **If that multiply overflows u128, the answer is already decided**: `d > u128::MAX ≈ 3.4·10³⁸` while `r = |a.mant| ≤ 10³⁸−1`, so `q = 0` and `2r ≤ 2·(10³⁸−1) < 3.4·10³⁸ < d` — a tie is impossible, and `bump` (with `Less`) yields 0 for the half modes and ±1 ulp only for `up` and the matching `floor`/`ceiling` direction (when `r > 0`). Otherwise long-divide as above.
  - Decide the final digit with the shared `bump` using `(2r) cmp d`; re-apply the combined sign; the quotient must satisfy the mantissa bound or the call is `Overflow`.
- Worked cases (encoded in `div_vectors.jsonl`): `div(1, 3, 4, half_even)`: k=4, n=10000, 10000÷3 → q=3333, r=1, 2r=2 < 3 → `"0.3333"`. `div(2, 3, 4, half_even)`: q=6666, r=2, 2r=4 > 3 → `"0.6667"`. No intermediate rounded value ever exists — that is precisely what "never double-rounded" means.

Differential harness (task `0405`):

`tools/gen_decimal_vectors.py` — Python 3 stdlib only, run manually and in CI, never a build dependency:

- Implements the **independent oracle**: exact rational arithmetic over Python ints (`a·10^k // b` with explicit remainder-based rounding per mode implemented in integer space — deliberately *not* `decimal.quantize`, which could itself double-round; the negative-`k` fold-overflow rule needs no special case in unbounded integers, making it a true cross-check), plus `decimal` with a 60-digit context as a second check where applicable.
- Deterministic case generation (fixed seed, no wall clock): boundary mantissas, all mode-by-tie combinations, ~5,000 seeded random op cases across scales 0..12.
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
