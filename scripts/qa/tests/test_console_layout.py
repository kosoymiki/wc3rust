"""Offline layer tests for the canonical console QA projection.

Run: python3 -m pytest scripts/qa/tests -q
No device, build, or cargo invocation.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

import pytest

from scripts.qa import analyze, layout


def load_facade():
    path = Path("analysis/agent-qa/acceptance/console_layout.py")
    spec = importlib.util.spec_from_file_location("qa_console_facade", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


console_layout = load_facade()


def test_console_geometry_is_1to1(tmp_path):
    lay = layout.layout(2414, 1080, {"left": 210, "top": 120, "right": 90, "bottom": 96})
    assert lay["console"][3] - lay["console"][1] == pytest.approx(288.0)
    assert lay["console_target_height"] == pytest.approx(288.0)
    assert lay["console"][3] == pytest.approx(1080.0)
    assert lay["minimap"][2] - lay["minimap"][0] == pytest.approx(216.0)
    assert lay["portrait"][2] - lay["portrait"][0] == pytest.approx(128.0)
    assert lay["cell"] == pytest.approx(64.0)
    assert lay["gap"] == pytest.approx(4.0)
    assert lay["top_bar"][3] - lay["top_bar"][1] == pytest.approx(32.0)
    assert lay["joystick"] is None


def test_card_is_four_by_three_with_card_slots():
    lay = layout.layout(2414, 1080, {"left": 0, "top": 0, "right": 0, "bottom": 0})
    slots = lay["card_slots"]
    assert len(slots) == 12
    for index, slot in enumerate(slots):
        assert slot[2] - slot[0] == pytest.approx(64.0)
        assert slot[3] - slot[1] == pytest.approx(64.0)
        row, col = divmod(index, 4)
        assert slot[0] == pytest.approx(lay["card"][0] + col * 68.0)
        assert slot[1] == pytest.approx(lay["card"][1] + row * 68.0)


def test_text_bands_use_console_token_sizes():
    lay = layout.layout(2414, 1080, {"left": 210, "top": 120, "right": 90, "bottom": 96})
    bands = {band["name"]: band for band in layout.text_bands(lay, 2414, 1080)}
    assert bands["map_name"]["expected_px"] == pytest.approx(22.0)
    assert bands["fps"]["expected_px"] == pytest.approx(11.0)
    assert bands["resource_0"]["expected_px"] == pytest.approx(20.0)
    assert bands["selection_name"]["expected_px"] == pytest.approx(22.0)
    assert bands["card_0"]["expected_px"] == pytest.approx(13.0)
    assert "joystick_label" not in bands


def test_console_layout_module_is_only_a_facade():
    facade = Path("analysis/agent-qa/acceptance/console_layout.py").read_text()
    assert "from scripts.qa.layout import" in facade
    assert "CONSOLE_TARGET =" not in facade
    assert "def layout(" not in facade
    assert console_layout.layout is layout.layout
    assert console_layout.text_bands is layout.text_bands


def test_analyze_has_one_console_path():
    source = Path("scripts/qa/analyze.py").read_text()
    assert "def build_console(" in source
    assert "def build_legacy(" not in source
    assert "return build_console(" in source
    assert "build_legacy(" not in source


def test_gameplay_uses_card_slots_not_old_rings():
    source = Path("scripts/qa/gameplay.py").read_text()
    assert "card_slots" in source
    assert "chip_slots" not in source
    assert 'lay["buttons"]' not in source
    assert "context_slots" not in source


def test_spec_is_machine_readable_and_sources_exist():
    spec = json.loads(Path("analysis/agent-qa/acceptance/spec.json").read_text())
    ids = [gate["id"] for gate in spec["gates"]]
    assert len(ids) == len(set(ids))
    categories = {gate["category"] for gate in spec["gates"]}
    for required in ("launch", "map_load", "hud_grid", "terrain_layers", "model_animation", "input", "audio", "performance", "process"):
        assert required in categories
    assert spec["expected"]["console_height"] == 288.0
    assert spec["expected"]["models_unit_building"] == 542
    assert spec["expected"]["scan"]["blp_total"] == 3935
    for gate in spec["gates"]:
        assert gate["source"], gate["id"]
        for reference in gate["source"]:
            path_part = reference.split(":", 1)[0]
            candidate = Path(path_part)
            if candidate.exists():
                assert candidate.is_file(), reference
            else:
                # Symbol references such as scripts/qa/analyze.py:terrain_metrics
                # keep the path before the colon; a missing path is a hard error.
                raise AssertionError(f"spec source path missing: {reference}")


def test_acceptance_plan_is_freeze_gated():
    proc = subprocess.run(
        [sys.executable, "analysis/agent-qa/acceptance/run_acceptance.py", "--plan"],
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert proc.returncode == 3, proc.stderr
    assert "ACCEPTANCE PLAN" in proc.stdout
    assert "--execute" not in proc.stdout.splitlines()[0]
