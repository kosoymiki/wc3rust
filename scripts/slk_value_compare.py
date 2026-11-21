#!/usr/bin/env python3
"""Byte-level cell check: shipped SLK tables vs our parser's value dump.

`crates/wc3-formats --example slk_dump` prints `id|col=value|…` per data row.
This script re-parses the same file with a standalone reader (stateful
`C;X<x>;Y<y>;K<value>` cells, `""` unescape, header row at Y1) and requires
identical line sets: same rows, same ids, same non-empty values.

Usage: python3 scripts/slk_value_compare.py [--json]
Exit:  0 identical, 1 any difference.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# original_parity builds every tool once and exports the directory: executing the
# built binary keeps this check off the shared cargo queue. Standalone runs fall
# back to `cargo run`.
BIN_DIR = os.environ.get("WC3RUST_BIN_DIR")
TABLES = [
    ROOT / "analysis" / "war3_data" / "TerrainArt" / "Water.slk",
    ROOT / "analysis" / "war3_data" / "TerrainArt" / "CliffTypes.slk",
    ROOT / "analysis" / "war3_data" / "TerrainArt" / "Terrain.slk",
]


def unquote(text: str) -> str:
    text = text.strip()
    if len(text) >= 2 and text[0] == '"' and text[-1] == '"':
        return text[1:-1].replace('""', '"')
    return text


def parse(path: Path) -> list[str]:
    text = path.read_bytes().decode("utf-8", "replace")
    cells: dict[tuple[int, int], str] = {}
    cx = cy = 0
    for line in text.splitlines():
        line = line.rstrip("\r")
        if len(line) < 2:
            continue
        parts = line.split(";")
        if parts[0] not in ("C", "F"):
            continue
        x, y, value, has = cx, cy, "", False
        for field in parts[1:]:
            if field.startswith("X"):
                try:
                    x = int(field[1:])
                except ValueError:
                    x = 0
            elif field.startswith("Y"):
                try:
                    y = int(field[1:])
                except ValueError:
                    y = 0
            elif field.startswith("K"):
                value = unquote(field[1:])
                has = True
        if has and x > 0 and y > 0:
            cells[(x - 1, y - 1)] = value
        cx, cy = x, y
    if not cells:
        return []
    max_x = max(x for x, _ in cells) + 1
    max_y = max(y for _, y in cells) + 1
    columns = [cells.get((x, 0), "") for x in range(max_x)]
    lines: list[str] = []
    for y in range(1, max_y):
        row = [cells.get((x, y), "") for x in range(max_x)]
        ident = row[0] if row else ""
        fields = sorted((columns[x], row[x]) for x in range(1, max_x) if row[x])
        line = ident
        for column, value in fields:
            line += f"|{column}={value}"
        lines.append(line)
    return lines


def main() -> int:
    as_json = "--json" in sys.argv[1:]
    existing = [p for p in TABLES if p.is_file()]
    if not existing:
        print("SLK_VALUE_SUMMARY tables=0 (no reference tables)")
        return 0

    exe = Path(BIN_DIR) / "examples" / "slk_dump" if BIN_DIR else None
    if exe is not None and exe.is_file():
        command = [str(exe)] + [str(p) for p in existing]
    else:
        command = (["scripts/h2-cargo", "run", "-q", "-p", "wc3-formats",
                    "--example", "slk_dump", "--offline", "--"] + [str(p) for p in existing])
    proc = subprocess.run(
        command,
        cwd=str(ROOT), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=600,
    )
    ours = proc.stdout.decode("utf-8", "replace").splitlines()

    expected: list[str] = []
    for path in existing:
        expected.extend(parse(path))

    if len(ours) != len(expected):
        diff_kind = f"line-count ours={len(ours)} independent={len(expected)}"
        first = next((i for i, (a, b) in enumerate(zip(ours, expected)) if a != b), None)
        detail = ""
        if first is not None:
            detail = f" first-diff-line={first} ours={ours[first][:160]!r} independent={expected[first][:160]!r}"
        if as_json:
            print(json.dumps({"ok": False, "reason": diff_kind, "detail": detail}, ensure_ascii=False))
        else:
            print(f"SLK_VALUE_MISMATCH {diff_kind}{detail}")
            print(f"SLK_VALUE_SUMMARY tables={len(existing)} failures=1 rc={proc.returncode}")
        return 1

    mismatches = [(i, a, b) for i, (a, b) in enumerate(zip(ours, expected)) if a != b]
    if as_json:
        print(json.dumps({"ok": not mismatches, "tables": len(existing),
                          "lines": len(ours), "mismatches": len(mismatches),
                          "sample": mismatches[:3]}, ensure_ascii=False, indent=2))
    else:
        for i, a, b in mismatches[:5]:
            print(f"SLK_VALUE_MISMATCH line={i} ours={a[:120]!r} independent={b[:120]!r}")
        print(f"SLK_VALUE_SUMMARY tables={len(existing)} lines={len(ours)} mismatches={len(mismatches)} rc={proc.returncode}")
    return 0 if not mismatches else 1


if __name__ == "__main__":
    sys.exit(main())
