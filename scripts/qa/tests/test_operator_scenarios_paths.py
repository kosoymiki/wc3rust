"""Offline regression tests for the operator-bug scenario runner paths.

Covers the 2026-11-21 11:56 device-run failure: a relative ``--out`` leaked a
relative Path into ``relative_to(ROOT)`` and crashed at the first screenshot
with ``ValueError: ... is not in the subpath of ...``. No device, build or
cargo invocation is needed.

Run: python3 -m pytest scripts/qa/tests/test_operator_scenarios_paths.py -q
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from scripts.qa import operator_scenarios as scenarios
from scripts.qa.common import rel

SCREEN_REL = "analysis/agent-qa/operator-bugs/screens/01-ready.png"


def test_canonical_out_is_always_absolute():
    assert scenarios.canonical_out(Path("analysis/agent-qa/operator-bugs")) == scenarios.OUT
    assert scenarios.canonical_out(scenarios.OUT) == scenarios.OUT
    assert scenarios.canonical_out(Path("/some/other/place")) == scenarios.OUT
    assert scenarios.OUT.is_absolute()


def test_rel_accepts_the_relative_path_that_crashed_the_device_run():
    # The failing run passed a cwd-relative screenshot path; this used to raise
    # ValueError inside Path.relative_to(ROOT).
    assert rel(Path(SCREEN_REL)) == SCREEN_REL


def test_rel_accepts_absolute_and_symlinked_spellings():
    absolute = ROOT / SCREEN_REL
    assert rel(absolute) == SCREEN_REL

    symlink = ROOT.parent / "herofolends-rs"
    if symlink.is_symlink():
        assert rel(symlink / SCREEN_REL) == SCREEN_REL


def test_rel_keeps_foreign_paths_verbatim():
    assert rel(Path("/data/local/tmp/x.png")) == "/data/local/tmp/x.png"


def test_scenario_runner_refuses_without_the_device_slot(monkeypatch, capsys):
    monkeypatch.delenv("H2_DEVICE_SLOT", raising=False)
    rc = scenarios.main([])
    assert rc == 3
    assert "DEVICE_SLOT_ABSENT" in capsys.readouterr().err
