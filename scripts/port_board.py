#!/usr/bin/env python3
"""Living port board: append-only checklist bound to H2 evidence.

The board is the single TODO surface for the 1:1 Warcraft III port. It is
append-only: `sync` adds rows discovered in the gap map / criteria / operator
reports and never deletes or reopens a done item. `done` requires an evidence
string, because the board is H2-bound: an item without evidence is not done.

Usage:
  port_board.py status
  port_board.py sync
  port_board.py add ID --section S --title T [--priority P0] [--gate G] [--lane L]
  port_board.py done ID --evidence "..."
  port_board.py doing ID [--evidence "..."]
  port_board.py block ID --evidence "reason"
  port_board.py todo ID
  port_board.py md
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BOARD = ROOT / "analysis" / "tech-lead" / "PORT_BOARD.json"
MD = ROOT / "analysis" / "tech-lead" / "PORT_BOARD.md"
GAP = ROOT / "analysis" / "tech-lead" / "mechanics_gap_map.json"
CRIT = ROOT / "analysis" / "tech-lead" / "FULL_PORT_CRITERIA.md"
BUGS = ROOT / "analysis" / "agent-qa" / "operator-bugs" / "REPORT.md"

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
            box = {"done": "x", "doing": "~", "todo": " ", "blocked": "!"}[it["status"]]
            gate = f" — `{it['gate']}`" if it.get("gate") else ""
            ev = f" — {it['evidence']}" if it.get("evidence") else ""
            lines.append(f"- [{box}] **{it['id']}** {it['title']} ({it['priority']}){gate}{ev}")
        lines.append("")
    MD.write_text("\n".join(lines) + "\n", encoding="utf-8")


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
