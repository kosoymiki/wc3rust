#!/usr/bin/env python3
"""Living port board: append-only checklist bound to H2 evidence.

The board is the single TODO surface for the 1:1 Warcraft III port. It is
append-only: `sync` adds rows discovered in the gap map / criteria / operator
reports and never deletes or reopens a done item. `done` requires an evidence
string, because the board is H2-bound: an item without evidence is not done.

Usage:
  port_board.py status
  port_board.py sync
  port_board.py check            # read-only gate: schema, evidence, md drift
  port_board.py sections         # open P0 items (filters: --section/--priority/--status/--all)
  port_board.py add ID --section S --title T [--priority P0] [--gate G] [--lane L]
  port_board.py done ID --evidence "..."
  port_board.py doing ID [--evidence "..."]
  port_board.py block ID --evidence "reason"
  port_board.py todo ID
  port_board.py md

Paths are overridable for tests/dry runs: PORT_BOARD_JSON, PORT_BOARD_MD,
PORT_BOARD_GAP, PORT_BOARD_CRIT, PORT_BOARD_BUGS.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _env_path(name: str, default: Path) -> Path:
    raw = os.environ.get(name)
    return Path(raw).expanduser().resolve() if raw else default


BOARD = _env_path("PORT_BOARD_JSON", ROOT / "analysis" / "tech-lead" / "PORT_BOARD.json")
MD = _env_path("PORT_BOARD_MD", ROOT / "analysis" / "tech-lead" / "PORT_BOARD.md")
GAP = _env_path("PORT_BOARD_GAP", ROOT / "analysis" / "tech-lead" / "mechanics_gap_map.json")
CRIT = _env_path("PORT_BOARD_CRIT", ROOT / "analysis" / "tech-lead" / "FULL_PORT_CRITERIA.md")
BUGS = _env_path("PORT_BOARD_BUGS", ROOT / "analysis" / "agent-qa" / "operator-bugs" / "REPORT.md")

SECTIONS_ORDER = [
    "acceptance", "mechanics", "operator-bugs", "hud", "rendering", "input",
    "audio", "campaign", "reference-runtime", "h2-binding", "infra", "tooling", "parity",
]


def load() -> dict:
    return json.loads(BOARD.read_text(encoding="utf-8"))


def save(data: dict) -> None:
    BOARD.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    write_md(data)


def find(data: dict, item_id: str) -> dict | None:
    return next((i for i in data["items"] if i["id"] == item_id), None)


def add_item(data: dict, item: dict) -> bool:
    if find(data, item["id"]):
        return False
    data["items"].append(item)
    return True


def write_md(data: dict) -> None:
    MD.write_text(render_md(data), encoding="utf-8")


def render_md(data: dict) -> str:
    sections: dict[str, list[dict]] = {}
    for it in data["items"]:
        sections.setdefault(it["section"], []).append(it)
    counts = {k: sum(1 for i in data["items"] if i["status"] == k) for k in ("done", "doing", "todo", "blocked")}
    lines = [
        "# PORT BOARD — living checklist (1:1 Warcraft III port)",
        "",
        "Append-only. Bound to H2: a `done` row needs evidence (device screenshot, rc, trace).",
        "`python3 scripts/port_board.py sync` grows the board from gap map / criteria / operator bugs.",
        "",
        f"**Total {len(data['items'])} · done {counts['done']} · doing {counts['doing']} · todo {counts['todo']} · blocked {counts['blocked']}**",
        "",
        "## Counts",
        "",
        "| section | done | doing | todo | blocked | total |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    order = [s for s in SECTIONS_ORDER if s in sections] + sorted(s for s in sections if s not in SECTIONS_ORDER)
    for sec in order:
        rows = sections[sec]
        c = {k: sum(1 for r in rows if r["status"] == k) for k in ("done", "doing", "todo", "blocked")}
        lines.append(f"| {sec} | {c['done']} | {c['doing']} | {c['todo']} | {c['blocked']} | {len(rows)} |")
    lines += ["", "## Board", ""]
    for sec in order:
        lines += [f"### {sec}", ""]
        dod = data.get("definition_of_done", {}).get(sec)
        if dod:
            lines += [f"_Done when:_ {dod}", ""]
        for it in sorted(sections[sec], key=lambda i: (i["priority"], i["id"])):
            box = {"done": "x", "doing": "~", "todo": " ", "blocked": "!"}.get(it["status"], "?")
            gate = f" — `{it['gate']}`" if it.get("gate") else ""
            ev = f" — {it['evidence']}" if it.get("evidence") else ""
            lines.append(f"- [{box}] **{it['id']}** {it['title']} ({it['priority']}){gate}{ev}")
        lines.append("")
    return "\n".join(lines) + "\n"


def cmd_status(data: dict, _args) -> int:
    counts: dict[str, dict[str, int]] = {}
    for it in data["items"]:
        row = counts.setdefault(it["section"], {"done": 0, "doing": 0, "todo": 0, "blocked": 0})
        row[it["status"]] = row.get(it["status"], 0) + 1
    total = {k: sum(1 for i in data["items"] if i["status"] == k) for k in ("done", "doing", "todo", "blocked")}
    print(f"TOTAL {len(data['items'])} done={total['done']} doing={total['doing']} todo={total['todo']} blocked={total['blocked']}")
    for sec in sorted(counts):
        c = counts[sec]
        print(f"  {sec:<18} done={c['done']:<3} doing={c['doing']:<3} todo={c['todo']:<3} blocked={c['blocked']:<3}")
    return 0


REQUIRED_ITEM_KEYS = ("id", "section", "title", "status", "priority")
VALID_STATUSES = ("done", "doing", "todo", "blocked")


def cmd_check(data: dict, _args) -> int:
    """Read-only gate: the board must be self-consistent and H2-honest.

    Fails (rc=1) on: a malformed item, a duplicate id, an unknown status, a
    `done` row without evidence, a `blocked` row without a reason, or a
    PORT_BOARD.md that has drifted from what `sync`/`md` would render. The gate
    never mutates the board: drift is reported, not repaired.
    """
    problems: list[str] = []
    seen: dict[str, int] = {}
    if not isinstance(data, dict) or data.get("schema") != "wc3rust-port-board-v1":
        problems.append(f"schema: expected wc3rust-port-board-v1, got {data.get('schema')!r}")
    items = data.get("items") if isinstance(data, dict) else None
    if not isinstance(items, list):
        print("BOARD_CHECK_FAIL schema: items is not a list")
        return 1
    for pos, it in enumerate(items):
        if not isinstance(it, dict):
            problems.append(f"item[{pos}]: not an object")
            continue
        missing = [k for k in REQUIRED_ITEM_KEYS if not str(it.get(k, "")).strip()]
        if missing:
            problems.append(f"item[{pos}] {it.get('id', '?')}: missing {','.join(missing)}")
            continue
        seen[it["id"]] = seen.get(it["id"], 0) + 1
        if it["status"] not in VALID_STATUSES:
            problems.append(f"{it['id']}: invalid status {it['status']!r}")
        evidence = str(it.get("evidence") or "").strip()
        if it["status"] == "done" and not evidence:
            problems.append(f"{it['id']}: done without evidence (H2: no evidence => not done)")
        if it["status"] == "blocked" and not evidence:
            problems.append(f"{it['id']}: blocked without a reason in evidence")
    for item_id, n in seen.items():
        if n > 1:
            problems.append(f"{item_id}: duplicate id x{n}")
    if MD.is_file():
        expected = render_md(data)
        actual = MD.read_text(encoding="utf-8")
        if actual != expected:
            problems.append("PORT_BOARD.md is stale: run `python3 scripts/port_board.py md` (or sync) and commit")
    else:
        problems.append(f"{MD}: missing (run `port_board.py md`)")
    counts = {k: sum(1 for i in items if i.get("status") == k) for k in VALID_STATUSES}
    if problems:
        print(f"BOARD_CHECK_FAIL problems={len(problems)} total={len(items)} "
              f"done={counts['done']} doing={counts['doing']} todo={counts['todo']} blocked={counts['blocked']}")
        for p in problems[:40]:
            print(f"  FAIL {p}")
        return 1
    print(f"BOARD_CHECK_PASS total={len(items)} done={counts['done']} doing={counts['doing']} "
          f"todo={counts['todo']} blocked={counts['blocked']}")
    return 0


def _csv(value: str) -> set[str] | None:
    parts = {p.strip() for p in (value or "").split(",") if p.strip()}
    if not parts or parts & {"all", "any", "*"}:
        return None
    return parts


def cmd_sections(data: dict, args) -> int:
    """List board items by section/priority/status (default: open P0 rows)."""
    statuses = None if args.all else _csv(args.status)
    priorities = _csv(args.priority)
    sections = _csv(args.section)
    sel = [
        it for it in data["items"]
        if (statuses is None or it["status"] in statuses)
        and (priorities is None or it["priority"] in priorities)
        and (sections is None or it["section"] in sections)
    ]
    order = [s for s in SECTIONS_ORDER if any(i["section"] == s for i in sel)]
    order += sorted({i["section"] for i in sel} - set(order))
    sel.sort(key=lambda i: (i["priority"], i["section"], i["id"]))
    by_status = {k: sum(1 for i in sel if i["status"] == k) for k in VALID_STATUSES}
    if args.json:
        print(json.dumps({
            "count": len(sel),
            "filter": {"status": sorted(statuses) if statuses else "all",
                       "priority": sorted(priorities) if priorities else "all",
                       "section": sorted(sections) if sections else "all"},
            "by_status": by_status, "items": sel,
        }, ensure_ascii=False, indent=2))
        return 0
    print(f"SECTIONS count={len(sel)} todo={by_status['todo']} doing={by_status['doing']} "
          f"blocked={by_status['blocked']} done={by_status['done']}")
    for sec in order:
        print(f"{sec}")
        for it in (i for i in sel if i["section"] == sec):
            print(f"  {it['status']:<8}{it['priority']:<4}{it['id']:<12} {it['title'][:90]}")
    return 0


def cmd_sync(data: dict, _args) -> int:
    added = 0
    gap = json.loads(GAP.read_text(encoding="utf-8"))
    rows = gap if isinstance(gap, list) else gap.get("rows", gap.get("mechanics", []))
    for row in rows:
        mid = row.get("id") or row.get("mechanic")
        if not mid:
            continue
        status = {"ported": "done", "partial": "doing", "missing": "todo", "data-only": "done"}.get(row.get("status", ""), "todo")
        ev = row.get("evidence") or row.get("test") or ""
        if isinstance(ev, (list, dict)):
            ev = json.dumps(ev, ensure_ascii=False)
        added += add_item(data, {
            "id": mid, "section": "mechanics", "title": row.get("title") or row.get("name") or mid,
            "status": status, "priority": row.get("priority", "P1"),
            "gate": "gate.port.mechanic-test-coverage", "evidence": str(ev)[:400], "lane": "sim",
        })
    if CRIT.is_file():
        crit = CRIT.read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r"^\|\s*(P0-\d+)\s*\|\s*([^|]{5,120})\|", crit, re.M):
            added += add_item(data, {
                "id": m.group(1), "section": "acceptance", "title": m.group(2).strip(),
                "status": "todo", "priority": "P0", "gate": "gate.port.full-parity-acceptance",
                "evidence": "", "lane": "port",
            })
    if BUGS.is_file():
        bugs = BUGS.read_text(encoding="utf-8", errors="replace")
        for m in re.finditer(r"^\|\s*(\d+)\s*\|\s*([^|]{4,120})\|.*\|\s*\*\*(fixed|partial|open)\*\*\s*\|", bugs, re.M):
            added += add_item(data, {
                "id": f"opbug-{m.group(1)}", "section": "operator-bugs", "title": m.group(2).strip(),
                "status": {"fixed": "done", "partial": "doing", "open": "todo"}[m.group(3)],
                "priority": "P0",
                "gate": "gate.ui.craft-floor" if "hud" in m.group(2).lower() else "gate.port.full-parity-acceptance",
                "evidence": "", "lane": "qa",
            })
    save(data)
    print(f"SYNC added={added} total={len(data['items'])}")
    return 0


def cmd_add(data: dict, args) -> int:
    ok = add_item(data, {
        "id": args.id, "section": args.section, "title": args.title, "status": "todo",
        "priority": args.priority, "gate": args.gate or "", "evidence": "", "lane": args.lane or "",
    })
    save(data)
    print(("ADDED " if ok else "EXISTS ") + args.id)
    return 0 if ok else 1


def set_status(data: dict, item_id: str, status: str, evidence: str) -> int:
    it = find(data, item_id)
    if not it:
        print(f"UNKNOWN {item_id}")
        return 1
    if status == "done" and not evidence.strip():
        print("REFUSED: done requires --evidence (H2: no evidence => not done)")
        return 2
    it["status"] = status
    if evidence:
        it["evidence"] = evidence
    save(data)
    print(f"{item_id} -> {status}" + (f" ({evidence[:80]})" if evidence else ""))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("status", "sync", "md"):
        p = sub.add_parser(name)
        p.set_defaults(func=(cmd_status if name == "status" else cmd_sync if name == "sync" else (lambda d, a: (write_md(d), 0)[1])))
    p = sub.add_parser("check", help="read-only consistency/evidence gate")
    p.set_defaults(func=cmd_check)
    p = sub.add_parser("sections", help="list items (default: open P0 rows)")
    p.add_argument("--section", default="", help="comma list, e.g. hud,input (default all)")
    p.add_argument("--priority", default="P0", help="comma list; 'all' disables (default P0)")
    p.add_argument("--status", default="todo,doing,blocked", help="comma list; ignored with --all")
    p.add_argument("--all", action="store_true", help="any status (overrides --status)")
    p.add_argument("--json", action="store_true", help="machine-readable output")
    p.set_defaults(func=cmd_sections)
    p = sub.add_parser("add")
    p.add_argument("id")
    p.add_argument("--section", required=True)
    p.add_argument("--title", required=True)
    p.add_argument("--priority", default="P1")
    p.add_argument("--gate", default="")
    p.add_argument("--lane", default="")
    p.set_defaults(func=cmd_add)
    for name in ("done", "doing", "block", "todo"):
        p = sub.add_parser(name)
        p.add_argument("id")
        p.add_argument("--evidence", default="")
        p.set_defaults(func=lambda d, a, n=name: set_status(d, a.id, {"done": "done", "doing": "doing", "block": "blocked", "todo": "todo"}[n], a.evidence))
    args = parser.parse_args()
    return args.func(load(), args)


if __name__ == "__main__":
    raise SystemExit(main())
