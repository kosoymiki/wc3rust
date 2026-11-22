"""Offline tests for scripts/port_board.py.

Run: python3 -m pytest scripts/tests -q

No cargo, no device, no repo mutation: every test drives the real CLI against a
board copied into tmp_path through the PORT_BOARD_* path overrides.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "port_board.py"
ROOT = SCRIPT.parents[1]


def _env(tmp_path: Path) -> dict[str, str]:
    env = dict(os.environ)
    env.update({
        "PORT_BOARD_JSON": str(tmp_path / "PORT_BOARD.json"),
        "PORT_BOARD_MD": str(tmp_path / "PORT_BOARD.md"),
        "PORT_BOARD_GAP": str(tmp_path / "no-gap.json"),
        "PORT_BOARD_CRIT": str(tmp_path / "no-crit.md"),
        "PORT_BOARD_BUGS": str(tmp_path / "no-bugs.md"),
    })
    return env


def run(tmp_path: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        capture_output=True, text=True, env=_env(tmp_path), cwd=str(ROOT), timeout=120,
    )


def item(item_id: str, status: str, evidence: str = "", priority: str = "P0",
         section: str = "mechanics") -> dict:
    return {
        "id": item_id, "section": section, "title": f"title {item_id}",
        "status": status, "priority": priority, "gate": "", "evidence": evidence, "lane": "sim",
    }


def write_board(tmp_path: Path, items: list[dict]) -> Path:
    board = {
        "schema": "wc3rust-port-board-v1", "generated": "2026-11-22",
        "source_artefacts": [], "definition_of_done": {}, "items": items,
    }
    path = tmp_path / "PORT_BOARD.json"
    path.write_text(json.dumps(board, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    md = run(tmp_path, "md")
    assert md.returncode == 0, md.stderr
    return path


def test_done_without_evidence_is_refused(tmp_path: Path):
    path = write_board(tmp_path, [item("m-1", "todo")])
    before = path.read_text(encoding="utf-8")
    result = run(tmp_path, "done", "m-1")
    assert result.returncode == 2, result.stdout
    assert "REFUSED" in result.stdout
    assert path.read_text(encoding="utf-8") == before, "a refused done must not mutate the board"

    ok = run(tmp_path, "done", "m-1", "--evidence", "crates/wc3-sim/tests/x.rs green rc=0")
    assert ok.returncode == 0, ok.stdout
    row = json.loads(path.read_text(encoding="utf-8"))["items"][0]
    assert row["status"] == "done"
    assert "rc=0" in row["evidence"]


def test_done_requires_known_id(tmp_path: Path):
    write_board(tmp_path, [item("m-1", "todo")])
    result = run(tmp_path, "done", "nope", "--evidence", "x")
    assert result.returncode == 1
    assert "UNKNOWN" in result.stdout


def test_check_passes_on_consistent_board(tmp_path: Path):
    write_board(tmp_path, [item("m-1", "done", "test rc=0"), item("m-2", "doing")])
    result = run(tmp_path, "check")
    assert result.returncode == 0, result.stdout
    assert "BOARD_CHECK_PASS total=2 done=1 doing=1" in result.stdout


def test_check_fails_on_done_without_evidence(tmp_path: Path):
    write_board(tmp_path, [item("m-1", "done", "")])
    result = run(tmp_path, "check")
    assert result.returncode == 1
    assert "done without evidence" in result.stdout


def test_check_fails_on_blocked_without_reason(tmp_path: Path):
    write_board(tmp_path, [item("m-1", "blocked", "")])
    result = run(tmp_path, "check")
    assert result.returncode == 1
    assert "blocked without a reason" in result.stdout


def test_check_fails_on_stale_md(tmp_path: Path):
    write_board(tmp_path, [item("m-1", "todo")])
    with open(tmp_path / "PORT_BOARD.md", "a", encoding="utf-8") as fh:
        fh.write("- hand-edited line\n")
    result = run(tmp_path, "check")
    assert result.returncode == 1
    assert "stale" in result.stdout

    assert run(tmp_path, "md").returncode == 0
    assert run(tmp_path, "check").returncode == 0


def test_check_fails_on_duplicate_and_bad_status(tmp_path: Path):
    write_board(tmp_path, [item("m-1", "todo"), item("m-1", "todo"), item("m-2", "wip")])
    result = run(tmp_path, "check")
    assert result.returncode == 1
    assert "duplicate id" in result.stdout
    assert "invalid status" in result.stdout


def test_sections_defaults_to_open_p0(tmp_path: Path):
    write_board(tmp_path, [
        item("a-1", "todo", priority="P0"),
        item("a-2", "done", "rc=0", priority="P0"),
        item("a-3", "doing", priority="P0", section="hud"),
        item("a-4", "todo", priority="P1"),
        item("a-5", "blocked", "device unavailable", priority="P0"),
    ])
    result = run(tmp_path, "sections")
    assert result.returncode == 0, result.stderr
    assert "SECTIONS count=3" in result.stdout
    assert "a-1" in result.stdout and "a-3" in result.stdout and "a-5" in result.stdout
    assert "a-2" not in result.stdout and "a-4" not in result.stdout


def test_sections_filters_and_json(tmp_path: Path):
    write_board(tmp_path, [
        item("a-1", "todo", priority="P0", section="hud"),
        item("a-2", "done", "rc=0", priority="P0", section="hud"),
        item("a-3", "todo", priority="P1", section="input"),
    ])
    hud = json.loads(run(tmp_path, "sections", "--json").stdout)
    assert hud["count"] == 1
    assert hud["items"][0]["id"] == "a-1"
    assert hud["filter"]["priority"] == ["P0"]

    only_input = json.loads(run(tmp_path, "sections", "--json", "--section", "input",
                                "--priority", "all", "--all").stdout)
    assert [i["id"] for i in only_input["items"]] == ["a-3"]

    everything = json.loads(run(tmp_path, "sections", "--json", "--all", "--priority", "all").stdout)
    assert everything["count"] == 3


def test_sync_is_idempotent(tmp_path: Path):
    sum_gap = tmp_path / "no-gap.json"
    gap = {"rows": [{"id": "m-1", "title": "t", "status": "partial", "test": "crates/wc3-sim/tests/m1.rs"}]}
    sum_gap.write_text(json.dumps(gap), encoding="utf-8")
    write_board(tmp_path, [])
    env = _env(tmp_path)
    env["PORT_BOARD_GAP"] = str(sum_gap)
    first = subprocess.run([sys.executable, str(SCRIPT), "sync"], capture_output=True, text=True,
                           env=env, cwd=str(ROOT), timeout=120)
    second = subprocess.run([sys.executable, str(SCRIPT), "sync"], capture_output=True, text=True,
                            env=env, cwd=str(ROOT), timeout=120)
    assert first.returncode == 0 and "added=1" in first.stdout, first.stdout + first.stderr
    assert second.returncode == 0 and "added=0" in second.stdout, second.stdout + second.stderr
    row = json.loads((tmp_path / "PORT_BOARD.json").read_text(encoding="utf-8"))["items"][0]
    assert row["id"] == "m-1" and row["status"] == "doing"
    assert run(tmp_path, "check").returncode == 0
