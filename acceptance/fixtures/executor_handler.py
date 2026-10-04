"""Portable, independent handler fixture; it imports no engine implementation.

Use an explicit interpreter path and this file rather than platform commands
or PATH lookup. Exit modes intentionally do no work besides reporting status.
"""
from __future__ import annotations

import argparse


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("exit-ok", "exit-failed"))
    args = parser.parse_args()
    return 0 if args.mode == "exit-ok" else 3


if __name__ == "__main__":
    raise SystemExit(main())
