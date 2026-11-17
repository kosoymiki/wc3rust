#!/usr/bin/env python3
"""Build analysis/tech-lead/test_matrix.json from mechanic_tests.json rows.

The mechanic runner is the single source of test execution counts; this script
only aggregates its rows per crate (no test is executed here, no test file is
created). A crate with any red row keeps the failing mechanic ids and its log
paths so the exact broken test is visible.

Usage: python3 scripts/qa/test_matrix.py [--rows PATH] [--out PATH]
"""

from __future__ import annotations

import argparse
import json
import time
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--rows", type=Path, default=ROOT / "analysis" / "tech-lead" / "mechanic_tests.json")
    parser.add_argument("--out", type=Path, default=ROOT / "analysis" / "tech-lead" / "test_matrix.json")
    args = parser.parse_args(argv)

    try:
        data = json.loads(args.rows.read_text())
    except (OSError, ValueError) as error:
        print(f"TEST_MATRIX_FAIL cannot read {args.rows}: {error}")
        return 1
    rows = data.get("rows") or data.get("results") or []
    if not rows:
        print(f"TEST_MATRIX_FAIL no rows in {args.rows}")
        return 1

    per_crate: dict[str, dict] = defaultdict(
        lambda: {"tests": 0, "passed": 0, "failed": 0, "mechanic_rows": 0,
                 "red_mechanics": [], "logs": [], "commands": []}
    )
    for row in rows:
        crate = row.get("crate") or "unknown"
        entry = per_crate[crate]
        entry["mechanic_rows"] += 1
        entry["passed"] += int(row.get("passed") or 0)
        entry["failed"] += int(row.get("failed") or 0)
        entry["logs"].append(row.get("log"))
        if row.get("command") and row["command"] not in entry["commands"]:
            entry["commands"].append(row["command"])
        if int(row.get("rc") or 0) != 0:
            entry["red_mechanics"].append(row.get("mechanic"))

    matrix_rows = []
    for crate, entry in sorted(per_crate.items()):
        entry["tests"] = entry["passed"] + entry["failed"]
        entry["crate"] = crate
        entry["rc"] = 0 if not entry["red_mechanics"] else 1
        matrix_rows.append(entry)

    payload = {
        "schema": "wc3-test-matrix-v1",
        "generated_unix": int(time.time()),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "source": str(args.rows),
        "source_mode": data.get("mode"),
        "source_generated_unix": data.get("generated_unix"),
        "rows": matrix_rows,
        "summary": {
            "crates": len(matrix_rows),
            "mechanic_rows": sum(r["mechanic_rows"] for r in matrix_rows),
            "tests": sum(r["tests"] for r in matrix_rows),
            "passed": sum(r["passed"] for r in matrix_rows),
            "failed": sum(r["failed"] for r in matrix_rows),
            "red_crates": {r["crate"]: r["red_mechanics"] for r in matrix_rows if r["rc"] != 0},
        },
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n")
    print(json.dumps(payload["summary"]))
    for row in matrix_rows:
        if row["rc"] != 0:
            print(f"RED {row['crate']}: {row['red_mechanics']}")
    print(f"wrote {args.out}")
    return 0 if not payload["summary"]["red_crates"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
