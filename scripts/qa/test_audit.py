#!/usr/bin/env python3
"""TEST_POLICY audit: which `#[test]` functions map to a gap-map mechanic row.

Policy (analysis/tech-lead/TEST_POLICY.md): one test -> one mechanic id from
mechanics_gap_map.json; the row's `test` field must name the test file or the
test function; a PORTED row must have a runner target. This script reports the
violations instead of silently ignoring them.

Output: analysis/agent-qa/test_audit.json
  {schema, generated_unix, total_tests, mapped, unmapped, unmapped_tests:[...],
   ported_without_entry:[...]}

Usage: python3 scripts/qa/test_audit.py [--out PATH]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GAP = ROOT / "analysis" / "tech-lead" / "mechanics_gap_map.json"
TARGETS = ROOT / "analysis" / "tech-lead" / "mechanic_targets.tsv"
TEST_RE = re.compile(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([A-Za-z0-9_]+)", re.S)


def test_files() -> list[Path]:
    files: list[Path] = []
    crates = ROOT / "crates"
    if not crates.is_dir():
        return files
    for crate in sorted(crates.iterdir()):
        tests_dir = crate / "tests"
        if tests_dir.is_dir():
            files.extend(sorted(path for path in tests_dir.glob("*.rs")))
        src_dir = crate / "src"
        if src_dir.is_dir():
            files.extend(sorted(src_dir.rglob("*.rs")))
    return files


def row_names_test(row: dict, path: Path, name: str) -> bool:
    text = (row.get("test") or "").lower()
    if not text or text.strip() in ("none", "none dedicated"):
        return False
    stem = path.name.removesuffix(".rs").lower()
    return name.lower() in text or stem in text


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=ROOT / "analysis" / "agent-qa" / "test_audit.json")
    args = parser.parse_args(argv)

    gap = json.loads(GAP.read_text()) if GAP.is_file() else {"rows": []}
    gap_rows = gap.get("rows", [])
    mapped: list[dict] = []
    unmapped: list[dict] = []

    for path in test_files():
        try:
            body = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        relative = str(path.relative_to(ROOT))
        for match in TEST_RE.finditer(body):
            name = match.group(1)
            prefix = name.split("_")[0]
            candidates = [row for row in gap_rows if (row.get("id") or "").split(".")[0] == prefix]
            naming = [row for row in candidates if row_names_test(row, path, name)]
            entry = {"test": name, "file": relative, "prefix": prefix}
            if naming:
                entry["rows"] = [row["id"] for row in naming]
                mapped.append(entry)
            else:
                entry["reason"] = (
                    "no gap-map row with this prefix"
                    if not candidates
                    else "gap row(s) do not name this test: " + ", ".join(r["id"] for r in candidates[:3])
                )
                unmapped.append(entry)

    targets: set[str] = set()
    if TARGETS.is_file():
        for line in TARGETS.read_text().splitlines():
            if line.strip():
                targets.add(line.split("\t")[0].strip())
    ported = [row["id"] for row in gap_rows if row.get("status") == "ported"]
    ported_without_entry = [mechanic for mechanic in ported if mechanic not in targets]

    payload = {
        "schema": "wc3-test-audit-v1",
        "generated_unix": int(time.time()),
        "policy": "analysis/tech-lead/TEST_POLICY.md",
        "total_tests": len(mapped) + len(unmapped),
        "mapped": len(mapped),
        "unmapped": len(unmapped),
        "unmapped_tests": unmapped,
        "ported_without_entry": ported_without_entry,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n")
    print(
        f"tests={payload['total_tests']} mapped={payload['mapped']} "
        f"unmapped={payload['unmapped']} ported_without_entry={ported_without_entry}"
    )
    for entry in unmapped[:20]:
        print(f"UNMAPPED {entry['file']}::{entry['test']} ({entry['reason']})")
    if len(unmapped) > 20:
        print(f"... {len(unmapped) - 20} more in {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
