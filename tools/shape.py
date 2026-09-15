#!/usr/bin/env python3
"""The shape of the code: how long every file and every function is.

A file thousands of lines long is a file nobody can hold in their head, and a
function hundreds of lines long is a function nobody can test. This measures
both, lists the worst, and with `--check` fails on anything over the limits
CLAUDE.md sets, so the limit is a check and not a promise.

    python3 tools/shape.py            # the report
    python3 tools/shape.py --check    # exit 1 on anything over a limit

A function's length is counted by matching braces from its `fn` line, so the
doc comment above it is not charged to it and neither is the whitespace
between two functions. Lines are lines: comments count, because a two hundred
line function with a hundred lines of comment is still two hundred lines to
scroll past.
"""
import re
import sys
from pathlib import Path

FILE_LIMIT = 900
FN_LIMIT = 100
ROOTS = ["crates/freeport_core/src", "crates/freeport_app/src", "crates/hex_planet/src"]
FN = re.compile(r"^\s*(pub(\([^)]*\))? )?(const |async |unsafe )*fn ([A-Za-z_][A-Za-z_0-9]*)")


def functions(lines):
    """Every function in a file as (length, name, first line), by brace matching."""
    out = []
    i = 0
    while i < len(lines):
        m = FN.match(lines[i])
        if not m:
            i += 1
            continue
        depth, started, j = 0, False, i
        while j < len(lines):
            depth += lines[j].count("{") - lines[j].count("}")
            if "{" in lines[j]:
                started = True
            if started and depth <= 0:
                break
            # A declaration with no body (`fn x();` in a trait) ends at the semicolon.
            if not started and lines[j].rstrip().endswith(";"):
                break
            j += 1
        out.append((j - i + 1, m.group(4), i + 1))
        i = j + 1
    return out


def main():
    check = "--check" in sys.argv
    root = Path(__file__).resolve().parent.parent
    files = []
    fns = []
    for r in ROOTS:
        for p in sorted((root / r).rglob("*.rs")):
            lines = p.read_text().split("\n")
            rel = p.relative_to(root)
            files.append((len(lines), str(rel)))
            for n, name, at in functions(lines):
                fns.append((n, f"{rel}:{at}", name))
    files.sort(reverse=True)
    fns.sort(reverse=True)

    print(f"{'lines':>6}  file")
    for n, f in files:
        flag = "  OVER" if n > FILE_LIMIT else ""
        print(f"{n:6d}  {f}{flag}")
    print()
    print(f"{'lines':>6}  function")
    for n, at, name in fns[:20]:
        flag = "  OVER" if n > FN_LIMIT else ""
        print(f"{n:6d}  {name} ({at}){flag}")
    over_files = [f for n, f in files if n > FILE_LIMIT]
    over_fns = [(n, name, at) for n, at, name in fns if n > FN_LIMIT]
    print()
    print(f"{len(files)} files, {len(fns)} functions; over {FILE_LIMIT} lines: {len(over_files)} files; over {FN_LIMIT} lines: {len(over_fns)} functions")
    if check and (over_files or over_fns):
        print("shape: FAILED")
        sys.exit(1)
    if check:
        print("shape: ok")


if __name__ == "__main__":
    main()
