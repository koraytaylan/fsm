# Architecture — Plan 0002

> The concrete deltas, by symbol.

## 0006 — Lexing

Task `0601` is the plan's first task, so it also wires the module: `crates/fsm-core/src/lib.rs` gains `pub mod expr;`, and `src/expr/mod.rs` is created declaring `pub mod ast; pub mod lexer; pub mod parser; pub mod typeck; pub mod eval; pub mod partial;` with empty stub files for each, so later tasks fill files without touching `lib.rs` or `mod.rs` again. `src/expr/mod.rs` also owns the shared error type:

- `pub struct ExprError { pub code: &'static str, pub span: Span, pub message: String, pub hint: String, pub details: Vec<(String, String)> }` — `code` values are the `expr/*` and `run/*` constants listed per workstream below; `hint` is mandatory and generated from the failure data (legal token sets, declared identifier lists, both operand types), never free prose.

`crates/fsm-core/src/expr/lexer.rs`:

- `pub struct Span { pub start: u32, pub end: u32 }` — byte offsets into the verbatim source, half-open.
- `pub enum Tok { Int(String), Dec(String), Str(String), Ident(String), TypeIdent(String), KwIf, KwThen, KwElse, KwAnd, KwOr, KwNot, KwTrue, KwFalse, KwCtx, KwEvt, Dot, Comma, LParen, RParen, EqEq, BangEq, Le, Lt, Ge, Gt, Plus, Minus, Star }`.
- `pub fn lex(src: &str) -> Result<Vec<(Tok, Span)>, ExprError>` — identifiers `[a-z_][a-z0-9_]{0,63}` (`Ident`), type identifiers `[A-Z][A-Za-z0-9_]{0,63}` (`TypeIdent`); the ten keywords are reserved (`if then else and or not true false ctx evt`); mode/unit words (`half_even`, `ms`, …) are ordinary `Ident`s — position, not reservation, disambiguates them. Number tokens capture digits verbatim (`Int` without a dot, `Dec` with exactly one dot and at least one digit on each side); string literals use JSON-style escapes reusing the escape logic conventions of `json::parse`. No comments exist (specs are machine-written; prose lives in description fields). Source over 4,096 bytes → `expr/too_long`; an unexpected byte → `expr/lex` with span.

There is no external source of truth for our token set, so lexing is pinned by inline unit tests (every token kind, adjacency cases like `>=` vs `>` `=`, keyword-vs-ident boundaries, span exactness) rather than a fixtures directory.

## 0007 — Parsing

`crates/fsm-core/src/expr/ast.rs` (task `0701`):

- `pub enum CmpOp { Eq, Ne, Lt, Le, Gt, Ge }`, `pub enum BinOp { Add, Sub, Mul }`, `pub enum DurUnit { Ms, S, Min, H, D }`.
- `pub enum Expr { IntLit { digits: String, span: Span }, DecLit { digits: String, scale: u8, span: Span }, StrLit { value: String, span: Span }, BoolLit { value: bool, span: Span }, CtxRef { name: String, span: Span }, EvtRef { name: String, span: Span }, EnumLit { ty: String, variant: String, span: Span }, Not { inner: Box<Expr>, span: Span }, Neg { inner: Box<Expr>, span: Span }, And { lhs: Box<Expr>, rhs: Box<Expr>, span: Span }, Or { … }, Cmp { op: CmpOp, lhs: Box<Expr>, rhs: Box<Expr>, span: Span }, Bin { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>, span: Span }, If { cond: Box<Expr>, then_branch: Box<Expr>, else_branch: Box<Expr>, span: Span }, Call { name: String, name_span: Span, args: Vec<Arg>, span: Span } }`.
- `pub enum Arg { Expr(Expr), Word { name: String, span: Span } }` — a bare lowercase identifier in argument position is a `Word` (a mode or unit; bare identifiers are never expressions, since variables are reachable only via `ctx.`/`evt.`); the typechecker resolves `Word`s per builtin signature.
- `pub fn render_ast(e: &Expr) -> String` — a stable, span-free S-expression rendering (e.g. `(and (cmp <= (evt amount) (ctx limit)) (not (ctx flag)))`) used by the parse goldens.
- `pub fn node_count(e: &Expr) -> u32` and `pub fn depth(e: &Expr) -> u32` for limit checks.

`crates/fsm-core/src/expr/parser.rs`:

- `pub fn parse(src: &str) -> Result<Expr, ExprError>` — recursive descent over the token stream implementing exactly this grammar (versioned `expr/1` in SPEC.md):

```ebnf
expr        = if_expr ;
if_expr     = "if" , or_expr , "then" , if_expr , "else" , if_expr | or_expr ;
or_expr     = and_expr , { "or" , and_expr } ;
and_expr    = not_expr , { "and" , not_expr } ;
not_expr    = "not" , not_expr | cmp_expr ;
cmp_expr    = add_expr , [ cmp_op , add_expr ] ;          (* non-associative *)
cmp_op      = "==" | "!=" | "<=" | "<" | ">=" | ">" ;
add_expr    = mul_expr , { ( "+" | "-" ) , mul_expr } ;
mul_expr    = unary_expr , { "*" , unary_expr } ;
unary_expr  = "-" , unary_expr | primary ;
primary     = int_lit | dec_lit | str_lit | "true" | "false"
            | ( "ctx" | "evt" ) , "." , ident
            | type_ident , "." , ident
            | ident , "(" , [ arg , { "," , arg } ] , ")"
            | "(" , expr , ")" ;
arg         = expr | ident ;                               (* bare ident = Word *)
```

- Dedicated errors, all with spans: a second comparison operator in one `cmp_expr` → `expr/chained_cmp` with hint exactly `use `and` to combine comparisons`; `Int` digits that do not fit `i64` → `expr/int_range`; `Dec` with more than 38 digits or more than 12 fraction digits → `expr/dec_range`; more than 512 AST nodes → `expr/too_long`; nesting beyond depth 32 → `expr/too_deep`; any other mismatch → `expr/parse` with the expected-token set rendered into `hint`. `/` and `%` are not tokens; `a / b` fails at the lexer with a hint naming `div(a, b, scale, mode)`.

Fixtures land first: `crates/fsm-core/tests/fixtures/expr/parse.jsonl` — lines `{"src": "...", "ast": "<render_ast form>"}` or `{"src": "...", "err": "expr/chained_cmp", "span": [9, 10]}` covering precedence, associativity, laziness shape, every parse-error code; `crates/fsm-core/tests/expr_golden.rs` asserts every line.

## 0008 — Typing

`crates/fsm-core/src/expr/typeck.rs` (task `0801`):

- `pub enum Ty { Bool, Int, Dec(u8), Str, Enum(String), Ts, Dur }` with `Display` rendering (`decimal(2)`, `enum Risk`, …) used in messages.
- `pub enum ScopeKind { Guard, TransitionAction, Invariant, Block }` — `Invariant` and `Block` have no event; an `EvtRef` there is `expr/evt_in_invariant` / `expr/evt_in_block`.
- `pub struct Scope<'a> { pub kind: ScopeKind, pub ctx: &'a BTreeMap<String, Ty>, pub evt: Option<&'a BTreeMap<String, Ty>>, pub enums: &'a BTreeMap<String, Vec<String>> }`.
- `pub struct TypeWarning { pub code: &'static str, pub span: Span, pub message: String }` — one code in this plan: `expr/round_widens` (a `round` whose target scale is ≥ the operand's; the mode is dead — use `dec`). Emitted by task `0902`.
- `pub fn typecheck(e: &Expr, scope: &Scope) -> Result<(Ty, Vec<TypeWarning>), ExprError>`.

The typing rules, exactly (each violation names its code):

| Construct | Rule |
|---|---|
| `IntLit` | `Int` (digits already i64-checked by the parser) |
| `DecLit` | `Dec(s)` where `s` = fraction-digit count |
| `+ -` | `Int×Int→Int` · `Dec(s1)×Dec(s2)→Dec(max(s1,s2))` · `Ts+Dur→Ts`, `Dur+Ts→Ts`, `Ts−Ts→Dur`, `Ts−Dur→Ts`, `Dur±Dur→Dur` · everything else `expr/type_mismatch`; `Dec` with `Int` specifically → `expr/mixed_class` with hint naming both fixes (`write 0.00-style literals` or `dec(x, s)`) |
| `*` | `Int×Int→Int` · `Dec(s1)×Dec(s2)→Dec(s1+s2)`, statically `expr/scale_cap` when `s1+s2 > 12` · `Dec(s)×Int→Dec(s)` and `Int×Dec(s)→Dec(s)` (exact) · `Dur×Int→Dur`, `Int×Dur→Dur` |
| unary `-` | `Int`, `Dec`, `Dur` only |
| `cmp` | both sides same class; `Dec` compares by value across scales; full order on `Int`, `Dec`, `Ts`, `Dur`; `Str`, `Enum`, `Bool` allow `==`/`!=` only, an ordering operator → `expr/cmp_unordered` |
| `and or not` | `Bool` operands |
| `if c then a else b` | `c: Bool`; branches unify in the same class; two `Dec` branches widen exactly to `Dec(max scale)` |
| `CtxRef`/`EvtRef` | declared name, else `expr/unknown_var`/`expr/unknown_field` with Levenshtein suggestion plus the full legal list in the hint |
| `EnumLit T.v` | `T` declared (`expr/unknown_enum`), `v` a variant (`expr/unknown_variant`); result `Enum(T)` |
| `Call` | until task `0902` lands signatures: every name → `expr/unknown_builtin` listing the legal names |

`crates/fsm-core/src/ident.rs` (a plan-0001 stub; the cross-plan touch is safe because plans land sequentially) gains `pub fn suggest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str>` — classic dynamic-programming Levenshtein, best candidate at distance ≤ 2.

Fixtures first: `crates/fsm-core/tests/fixtures/expr/typeck.jsonl` — lines declaring a scope inline (`{"ctx": {"limit": "decimal(2)"}, "evt": {"amount": "decimal(2)"}, "enums": {"Risk": ["low", "high"]}, "src": "...", "ty": "decimal(2)"}` or `"err": "expr/mixed_class"`), one line minimum per error code; `crates/fsm-core/tests/expr_typeck.rs` asserts every line.

## 0009 — Evaluation

`crates/fsm-core/src/expr/eval.rs` (task `0901`):

- `pub enum Val { Bool(bool), Int(i64), Dec(crate::decimal::Dec), Str(String), Enum { ty: String, variant: String }, Ts(i64), Dur(i64) }` with `pub fn canonical_string(&self) -> String` (decimals via `Dec::format`, timestamps/durations as decimal integer strings) used by traces and, later, by hashing.
- `pub struct Bindings<'a> { pub ctx: &'a BTreeMap<String, Val>, pub evt: Option<&'a BTreeMap<String, Val>> }`.
- `pub struct Budget { remaining: u32 }` with `pub fn new(limit: u32)` — one `Budget` is shared across every evaluation of a single event (guards, all blocks, invariants); each AST-node visit decrements; exhaustion → `internal/budget` (an engine-invariant breach, never a user error — the statically-enforced size limits make the legal worst case fit with 4× headroom).
- `pub enum TraceOutcome { Value(String), Skipped, Error { code: &'static str, operands: Vec<String> } }`, `pub struct TraceNode { pub span: Span, pub outcome: TraceOutcome, pub children: Vec<TraceNode> }`, `pub fn trace_to_value(t: &TraceNode) -> crate::json::Value`.
- `pub fn eval(e: &Expr, b: &Bindings, budget: &mut Budget, trace: bool) -> (Result<Val, ExprError>, Option<TraceNode>)` — strict left-to-right; `and`/`or` short-circuit and `if` evaluates only the taken branch, with unevaluated subtrees recorded as `Skipped`; all `Int`/`Ts`/`Dur` arithmetic uses `checked_*` (including `-(i64::MIN)`) → `run/overflow` with both operand canonical strings in `details` and in the trace; `Dec` arithmetic delegates to `crate::decimal` (`DecError::Overflow` → `run/overflow`). Evaluation is total: no recursion into user code exists, and the budget bounds pathological trees.

Builtins (task `0902`) extend `typeck.rs` and `eval.rs` together — the complete set, with the rule that scale arguments must be integer literals `0..=12` and mode/unit arguments must be literal `Word`s (otherwise result *types* would depend on runtime values → `expr/scale_not_literal` / `expr/mode_invalid`):

| Signature | Typing | Evaluation |
|---|---|---|
| `min(a, b)`, `max(a, b)` | both `Int` → `Int`; both `Dec` → `Dec(max scale)`; both `Ts` or both `Dur` | value comparison (Dec via `Dec::cmp`) |
| `abs(x)` | type-preserving on `Int`/`Dec`/`Dur` | checked (`abs(i64::MIN)` → `run/overflow`) |
| `dec(x, S)` | `Int → Dec(S)`; `Dec(s0) → Dec(S)` requires `s0 ≤ S` else `expr/scale_narrow` (hint: use `round`) | exact widen, total |
| `round(x, S, M)` | `Dec(s0) → Dec(S)`, `M` mandatory; warns `expr/round_widens` when `S ≥ s0` | `Dec::round` |
| `div(a, b, S, M)` | `a`, `b` each `Int` or `Dec` → `Dec(S)` | `Dec::div` (correctly rounded exact rational); `b = 0` → `run/div_zero` |
| `dur(n, U)` | `n: Int`, `U ∈ ms s min h d` → `Dur` | checked multiply to milliseconds |

`M ∈ {down, up, floor, ceiling, half_up, half_down, half_even}` maps to `crate::decimal::RoundMode`; wrong arity → `expr/arity`; unknown name → `expr/unknown_builtin`.

Fixtures first for both tasks: `crates/fsm-core/tests/fixtures/expr/eval.jsonl` (bindings + src → value or `run/*` error, including short-circuit proofs where the skipped side would error) asserted by `crates/fsm-core/tests/expr_eval.rs`, and `crates/fsm-core/tests/fixtures/expr/builtins.jsonl` (every builtin × edge cases: ties per mode, `dec` narrowing rejection, `div` by zero, `dur` overflow) asserted by `crates/fsm-core/tests/expr_builtins.rs`.

## 0010 — Partial Evaluation

`crates/fsm-core/src/expr/partial.rs` (task `1001`):

- `pub enum Truth { True, False, Unknown }` (Kleene).
- `pub fn partial_eval_bool(e: &Expr, ctx: &BTreeMap<String, Val>, budget: &mut Budget) -> Truth` — `EvtRef` is `Unknown`; `and`/`or`/`not` follow Kleene tables (`False and _ = False`, `True or _ = True`); comparisons and arithmetic containing an `Unknown` operand are `Unknown`; fully-`ctx` subtrees evaluate concretely via `eval`. A concrete sub-evaluation error yields `Unknown` — deliberately conservative: for the enabled-events report an erroring guard is neither definitely enabled nor definitely disabled, and the authoritative loud failure (`run/guard_error`) happens at send time. This choice is documented in SPEC.md.

Inline unit tests pin the Kleene tables and the conservative-error rule; `crates/fsm-core/tests/expr_partial.rs` runs mixed ctx/evt fixtures from `crates/fsm-core/tests/fixtures/expr/partial.jsonl` (authored first).

## 0011 — Docs

Task `1101` creates `docs/SPEC.md` — the normative specification the whole project builds against, written to the bar "an independent team could reimplement from this document":

- Skeleton: title, a normative-language note (MUST/NEVER usage), format-version registry (`fsm.machine/1`, `fsm.journal/1`, `fsm.state/1`, expression grammar `expr/1`), and placeholder sections `## Machine definitions`, `## Semantics`, `## Journal`, `## Error code appendix`, each with a one-line "landed by plan NNNN" marker.
- The complete `## Expressions` section: the EBNF exactly as in workstream 0007, keyword and contextual-word rules, the typing tables from workstream 0008, builtin signatures and literal-argument rules from workstream 0009, evaluation-order rules (strict left-to-right, short-circuit, lazy `if`, shared budget), the three-valued partial-evaluation semantics and its conservative-error rule, and the `expr/*` + expression-raised `run/*` error catalogue (code, trigger, hint policy).
- The standing rule, stated in the document: **golden fixtures derive from this prose, never from observed implementation behavior** — a golden that disagrees with SPEC.md is a bug in the implementation or in the golden, never a reason to edit SPEC.md silently.
