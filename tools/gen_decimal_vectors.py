#!/usr/bin/env python3
"""Generate crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl

Independent integer-arithmetic oracle for fsm decimal ops. Deliberately does
*not* use decimal.quantize for the primary result (that can double-round).
A 60-digit decimal.Context is a second check where it applies.

Regenerate:
    python3 tools/gen_decimal_vectors.py

A diff against the committed file after regeneration is a release blocker:
the generator is deterministic (fixed seed, no wall clock, sorted output).
"""

from __future__ import annotations

import decimal
import json
import random
import sys
from pathlib import Path

MAX_SCALE = 12
MAX_MANT = 10**38 - 1
MODES = ["down", "up", "floor", "ceiling", "half_up", "half_down", "half_even"]
SEED = 20260814
OUT = Path(__file__).resolve().parents[1] / "crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl"


def bump(mode: str, negative: bool, twice_cmp: int, q_even: bool) -> bool:
    if mode == "down":
        return False
    if mode == "up":
        return True
    if mode == "floor":
        return negative
    if mode == "ceiling":
        return not negative
    if mode == "half_up":
        return twice_cmp >= 0
    if mode == "half_down":
        return twice_cmp > 0
    if mode == "half_even":
        return twice_cmp > 0 or (twice_cmp == 0 and not q_even)
    raise ValueError(mode)


def fmt(mant: int, scale: int) -> str:
    if mant == 0:
        if scale == 0:
            return "0"
        return "0." + ("0" * scale)
    neg = mant < 0
    digits = str(abs(mant))
    if scale == 0:
        return ("-" if neg else "") + digits
    if len(digits) <= scale:
        digits = "0" * (scale + 1 - len(digits)) + digits
    split = len(digits) - scale
    body = digits[:split] + "." + digits[split:]
    return ("-" if neg else "") + body


def parse_ok(src: str, scale: int):
    # same grammar as Rust
    import re

    if scale > MAX_SCALE:
        return "scale_cap"
    if not re.fullmatch(r"-?(0|[1-9][0-9]*)(\.[0-9]+)?", src):
        return "parse"
    neg = src.startswith("-")
    body = src[1:] if neg else src
    if "." in body:
        intp, frac = body.split(".", 1)
    else:
        intp, frac = body, ""
    if len(frac) > scale:
        return "parse"
    digits = intp + frac + ("0" * (scale - len(frac)))
    mant = int(digits)
    if mant > MAX_MANT:
        return "overflow"
    if mant == 0:
        return (0, scale)
    if neg:
        mant = -mant
    return (mant, scale)


def add_sub(a, sa, b, sb, sub: bool):
    scale = max(sa, sb)
    am = a * (10 ** (scale - sa))
    bm = b * (10 ** (scale - sb))
    if sub:
        m = am - bm
    else:
        m = am + bm
    if abs(m) > MAX_MANT:
        return "overflow"
    return (m, scale)


def mul(a, sa, b, sb):
    scale = sa + sb
    if scale > MAX_SCALE:
        return "scale_cap"
    m = a * b
    if abs(m) > MAX_MANT:
        return "overflow"
    return (m, scale)


def round_dec(a, sa, S, mode):
    if S > MAX_SCALE:
        return "scale_cap"
    if S >= sa:
        m = a * (10 ** (S - sa))
        if abs(m) > MAX_MANT:
            return "overflow"
        return (m, S)
    delta = sa - S
    div = 10**delta
    negative = a < 0
    mag = abs(a)
    q, r = divmod(mag, div)
    if r != 0:
        twice = r * 2
        if twice > div:
            cmpv = 1
        elif twice == div:
            cmpv = 0
        else:
            cmpv = -1
        if bump(mode, negative, cmpv, q % 2 == 0):
            q += 1
    if q > MAX_MANT:
        return "overflow"
    m = 0 if q == 0 else (-q if negative else q)
    return (m, S)


def div_dec(a, sa, b, sb, S, mode):
    if b == 0:
        return "div_zero"
    if S > MAX_SCALE:
        return "scale_cap"
    k = S - sa + sb
    negative = (a < 0) != (b < 0)
    n = abs(a) * (10**k if k >= 0 else 1)
    d = abs(b) * (10 ** (-k) if k < 0 else 1)
    q, r = divmod(n, d)
    if r != 0:
        twice = r * 2
        if twice > d:
            cmpv = 1
        elif twice == d:
            cmpv = 0
        else:
            cmpv = -1
        if bump(mode, negative, cmpv, q % 2 == 0):
            q += 1
    if q > MAX_MANT:
        return "overflow"
    m = 0 if q == 0 else (-q if negative else q)
    return (m, S)


def rec_ok(op, **fields):
    d = {"op": op}
    d.update(fields)
    return d


def emit_result(base: dict, result):
    if isinstance(result, str):
        base["error"] = result
    else:
        mant, scale = result
        base["format"] = fmt(mant, scale)
    return base


def decimal_second_check(op, rec, result):
    """Where decimal.Context(prec=60) applies, it must agree."""
    ctx = decimal.Context(prec=60, rounding=decimal.ROUND_HALF_EVEN)
    decimal.setcontext(ctx)
    if result in ("parse", "scale_cap", "div_zero"):
        return
    if op not in ("add", "sub", "mul"):
        return
    if "error" in rec and rec["error"] == "overflow":
        return
    if "error" in rec:
        return
    try:
        a = decimal.Decimal(rec["a"])
        b = decimal.Decimal(rec["b"])
        if op == "add":
            got = a + b
        elif op == "sub":
            got = a - b
        else:
            got = a * b
        if "format" not in rec:
            return
        want = decimal.Decimal(rec["format"])
        if got != want:
            raise SystemExit(f"decimal second check failed {rec}: {got} != {want}")
    except decimal.InvalidOperation:
        return


def main() -> int:
    rng = random.Random(SEED)
    rows: list[dict] = []

    # Sanity: architecture worked sets must appear.
    sanity = [
        emit_result(
            rec_ok("round", a="2.345", sa=3, scale=2, mode="half_even"),
            round_dec(2345, 3, 2, "half_even"),
        ),
        emit_result(
            rec_ok("div", a="1", sa=0, b="3", sb=0, scale=4, mode="half_even"),
            div_dec(1, 0, 3, 0, 4, "half_even"),
        ),
        emit_result(
            rec_ok("div", a="2", sa=0, b="3", sb=0, scale=4, mode="half_even"),
            div_dec(2, 0, 3, 0, 4, "half_even"),
        ),
    ]
    assert sanity[0]["format"] == "2.34", sanity[0]
    assert sanity[1]["format"] == "0.3333", sanity[1]
    assert sanity[2]["format"] == "0.6667", sanity[2]
    rows.extend(sanity)

    # Boundary mantissas
    for op in ("add", "sub", "mul", "div", "round", "cmp"):
        for sign in (1, -1):
            m = sign * MAX_MANT
            src = fmt(m, 0)
            if op == "round":
                rows.append(
                    emit_result(
                        rec_ok("round", a=src, sa=0, scale=0, mode="down"),
                        round_dec(m, 0, 0, "down"),
                    )
                )
            elif op == "cmp":
                rows.append(rec_ok("cmp", a=src, sa=0, b="0", sb=0, ord="gt" if sign > 0 else "lt"))
            elif op == "div":
                rows.append(
                    emit_result(
                        rec_ok("div", a=src, sa=0, b="1", sb=0, scale=0, mode="down"),
                        div_dec(m, 0, 1, 0, 0, "down"),
                    )
                )
            else:
                other = 0
                fn = {"add": lambda: add_sub(m, 0, other, 0, False), "sub": lambda: add_sub(m, 0, other, 0, True), "mul": lambda: mul(m, 0, 1, 0)}[op]
                rows.append(
                    emit_result(
                        rec_ok(op, a=src, sa=0, b=("0" if op != "mul" else "1"), sb=0),
                        fn(),
                    )
                )

    # Fold-overflow division rows (≥3)
    for mode in ("down", "up", "floor"):
        rows.append(
            emit_result(
                rec_ok(
                    "div",
                    a="1.000000000000",
                    sa=12,
                    b=str(MAX_MANT),
                    sb=0,
                    scale=0,
                    mode=mode,
                ),
                div_dec(10**12, 12, MAX_MANT, 0, 0, mode),
            )
        )

    # Exact-tie per mode per op with remainder
    for mode in MODES:
        rows.append(
            emit_result(
                rec_ok("round", a="2.345", sa=3, scale=2, mode=mode),
                round_dec(2345, 3, 2, mode),
            )
        )
        rows.append(
            emit_result(
                rec_ok("div", a="1", sa=0, b="8", sb=0, scale=2, mode=mode),
                div_dec(1, 0, 8, 0, 2, mode),
            )
        )
        # add/sub/mul don't have remainders; include a near-boundary case
        rows.append(
            emit_result(
                rec_ok("add", a="1.5", sa=1, b="0.25", sb=2),
                add_sub(15, 1, 25, 2, False),
            )
        )

    # Seeded random cases
    random_count = 0
    while random_count < 5200:
        op = rng.choice(["add", "sub", "mul", "div", "round", "cmp"])
        sa = rng.randint(0, MAX_SCALE)
        sb = rng.randint(0, MAX_SCALE)
        # keep mantissas modest most of the time, occasionally large
        def pick_mant():
            if rng.random() < 0.05:
                return rng.choice([0, 1, -1, MAX_MANT, -MAX_MANT, MAX_MANT // 2])
            digits = rng.randint(1, 12)
            mag = rng.randint(0, 10**digits - 1)
            return mag if rng.random() < 0.5 else -mag

        a = pick_mant()
        b = pick_mant()
        mode = rng.choice(MODES)
        S = rng.randint(0, MAX_SCALE)
        if op == "add":
            rec = emit_result(rec_ok("add", a=fmt(a, sa), sa=sa, b=fmt(b, sb), sb=sb), add_sub(a, sa, b, sb, False))
        elif op == "sub":
            rec = emit_result(rec_ok("sub", a=fmt(a, sa), sa=sa, b=fmt(b, sb), sb=sb), add_sub(a, sa, b, sb, True))
        elif op == "mul":
            rec = emit_result(rec_ok("mul", a=fmt(a, sa), sa=sa, b=fmt(b, sb), sb=sb), mul(a, sa, b, sb))
        elif op == "div":
            rec = emit_result(
                rec_ok("div", a=fmt(a, sa), sa=sa, b=fmt(b, sb), sb=sb, scale=S, mode=mode),
                div_dec(a, sa, b, sb, S, mode),
            )
        elif op == "round":
            rec = emit_result(rec_ok("round", a=fmt(a, sa), sa=sa, scale=S, mode=mode), round_dec(a, sa, S, mode))
        else:
            if a < 0 and b < 0:
                # value compare via python
                av = a * 10 ** (MAX_SCALE - sa)
                bv = b * 10 ** (MAX_SCALE - sb)
            else:
                av = a * 10 ** (MAX_SCALE - sa)
                bv = b * 10 ** (MAX_SCALE - sb)
            if av < bv:
                ord_ = "lt"
            elif av > bv:
                ord_ = "gt"
            else:
                ord_ = "eq"
            rec = rec_ok("cmp", a=fmt(a, sa), sa=sa, b=fmt(b, sb), sb=sb, ord=ord_)
        rows.append(rec)
        random_count += 1

    # Coverage counters
    ops = {}
    modes_seen = {m: set() for m in ["add", "sub", "mul", "div", "round"]}
    ties = {m: set() for m in ["div", "round"]}
    fold = 0
    for rec in rows:
        op = rec["op"]
        ops[op] = ops.get(op, 0) + 1
        if op in modes_seen and "mode" in rec:
            modes_seen[op].add(rec["mode"])
        if op in ("add", "sub", "mul"):
            for m in MODES:
                modes_seen[op].add(m)  # mode-less ops count as covered for all modes
        if op in ("div", "round") and rec.get("format"):
            # detect ties by recompute remainder
            pass

    # Count fold-overflow and ties explicitly
    for rec in rows:
        if rec["op"] == "div" and rec.get("sa") == 12 and rec.get("sb") == 0 and rec.get("scale") == 0:
            try:
                if abs(int(rec["b"])) > 10**26:
                    fold += 1
            except Exception:
                pass

    if random_count < 5000:
        raise SystemExit("not enough random cases")
    for op in ("add", "sub", "mul", "div", "round", "cmp"):
        if ops.get(op, 0) == 0:
            raise SystemExit(f"missing op {op}")
    for op in ("div", "round"):
        if modes_seen[op] != set(MODES):
            raise SystemExit(f"missing modes for {op}: {modes_seen[op]}")
    if fold < 3:
        raise SystemExit(f"need ≥3 fold-overflow rows, got {fold}")

    # Exact-tie coverage: we authored them
    for mode in MODES:
        if not any(r["op"] == "round" and r.get("a") == "2.345" and r.get("mode") == mode for r in rows):
            raise SystemExit(f"missing tie round {mode}")
        if not any(r["op"] == "div" and r.get("a") == "1" and r.get("b") == "8" and r.get("mode") == mode for r in rows):
            raise SystemExit(f"missing tie div {mode}")

    # Second reference
    for rec in rows:
        decimal_second_check(rec["op"], rec, rec.get("error") or rec.get("format"))

    # Stable JSON lines, sorted
    def line(rec: dict) -> str:
        return json.dumps(rec, sort_keys=True, separators=(",", ":"))

    lines = sorted(line(r) for r in rows)
    text = "\n".join(lines) + "\n"
    OUT.parent.mkdir(parents=True, exist_ok=True)
    # Bytes, not text. `write_text` opens in text mode, and on Windows that
    # rewrites every "\n" as "\r\n" — so this generator, whose whole purpose is
    # to reproduce a fixture byte for byte, produced a different file there than
    # everywhere else. Encoding explicitly leaves nothing for the platform to
    # decide.
    OUT.write_bytes(text.encode("utf-8"))
    print(f"wrote {len(lines)} vectors to {OUT}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) > 1:
        OUT = Path(sys.argv[1])
    sys.exit(main())
