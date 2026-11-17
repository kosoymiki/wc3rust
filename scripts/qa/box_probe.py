#!/usr/bin/env python3
"""Focused box-select proof on the installed build (one short session).

The full cycle proved: injected drag reaches /dev/input (getevent), the game
draws the selection rectangle, and an empty rectangle clears selection. This
probe centres the camera on the selected army (ARMY card), drags a rectangle
that contains the units, and records the resulting selection.
"""

from __future__ import annotations

import importlib.util
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from PIL import Image
import numpy as np

from scripts.qa import analyze
from scripts.qa.common import COMPONENT, PKG
from scripts.qa.device import Device

ACCEPT = ROOT / "analysis" / "agent-qa" / "acceptance"
sys.path.insert(0, str(ACCEPT))
import console_layout  # noqa: E402

spec = importlib.util.spec_from_file_location("acceptance_helpers", ACCEPT / "run_acceptance.py")
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)
state_samples = helpers.state_samples
parse_game_hud = helpers.parse_game_hud
world_rect = helpers.world_rect

OUT = ROOT / "analysis" / "agent-qa" / "parity" / "box-probe"
REMOTE = "/data/local/tmp/qa-box-getevent.log"


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    shots = OUT / "screens"
    shots.mkdir(parents=True, exist_ok=True)
    dev = Device()
    payload: dict = {"schema": "wc3rust-box-probe-v1", "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}
    try:
        launch = dev.launch(apk=ROOT / "build" / "android-p0" / "release" / "wc3rust-p0.apk", install=False)
        display = launch["display"]
        w, h = display["w"], display["h"]
        safe = launch.get("safe_insets") or launch["insets"]
        lay = {**console_layout.layout(w, h, safe), **(parse_game_hud(dev.logs()) or {})}
        slots = lay.get("card_slots") or []
        if len(slots) < 8:
            raise RuntimeError(f"card slots={len(slots)}")
        dev.screenshot(shots / "start.png", display)

        def tap_slot(index: int) -> None:
            box = slots[index]
            dev.tap((box[0] + box[2]) / 2, (box[1] + box[3]) / 2)

        # ARMY selects every player non-building unit and centres the camera on
        # the first one, so the unit cluster is at the screen centre.
        before_army = dev.logs().count("BUTTON ARMY")
        tap_slot(1)
        time.sleep(1.2)
        selected = int((state_samples(dev.logs()) or [{}])[-1].get("selected", 0) or 0)
        payload["army"] = {
            "button": dev.logs().count("BUTTON ARMY") > before_army,
            "selected": selected,
            "screenshot": "screens/army.png",
        }
        dev.screenshot(shots / "army.png", display)

        image = np.asarray(Image.open(shots / "army.png").convert("RGB"))
        rect = world_rect(lay, w, h)
        units = analyze.detect_units(image, rect)
        payload["detected_blue_clusters"] = units.get("blue_clusters", [])[:10]

        # Drag a box around the screen centre (camera centre = first unit).
        cx, cy = w / 2, h * 0.42
        x1, y1 = cx - 420, cy - 300
        x2, y2 = cx + 420, cy + 300
        dev.su(f"sh -c ': > {REMOTE}'", timeout=15)
        dev.su(f"sh -c 'timeout 10 getevent -lt > {REMOTE} 2>&1 &'", timeout=15)
        time.sleep(0.5)
        before = int((state_samples(dev.logs()) or [{}])[-1].get("selected", 0) or 0)
        drag = dev.timed_drag(x1, y1, x2, y2, hold_ms=450, steps=10, step_ms=25)
        time.sleep(1.6)
        getevent = dev.su(f"cat {REMOTE}", timeout=20).stdout or ""
        lines = [line for line in getevent.splitlines() if line.strip()]
        ok = bool(lines) and "Usage: getevent" not in getevent
        injected = ok and any(marker in getevent for marker in ("EV_ABS", "EV_KEY", "SYN_REPORT"))
        after = int((state_samples(dev.logs()) or [{}])[-1].get("selected", 0) or 0)
        dev.screenshot(shots / "box-after.png", display)
        payload["box"] = {
            "rect": [x1, y1, x2, y2],
            "drag": drag,
            "selected_before": before,
            "selected_after": after,
            "getevent_lines": len(lines),
            "getevent_ok": ok,
            "getevent_injected_seen": injected,
            "screenshot_before": "screens/army.png",
            "screenshot_after": "screens/box-after.png",
        }
        # BASE control: tap mapping still works after the drag.
        before_base = dev.logs().count("BUTTON BASE")
        tap_slot(2)
        time.sleep(0.6)
        payload["tap_proof"] = {"ok": dev.logs().count("BUTTON BASE") > before_base}
        payload["status"] = "PASS" if (payload["army"]["button"] and after >= 1 and injected) else "FAIL"
    except Exception as error:
        payload["error"] = f"{type(error).__name__}: {error}"
        payload["status"] = "FAIL"
    finally:
        dev.su(f"am force-stop {PKG}", timeout=20)
        dev.su(f"am start -W -n com.termux/com.termux.app.TermuxActivity", timeout=30)
        deadline = time.time() + 8
        while time.time() < deadline:
            if "com.termux" in dev.focus():
                break
            time.sleep(0.5)
        payload["cleanup"] = {"focus": dev.focus()}
    (OUT / "probe.json").write_text(json.dumps(payload, indent=2) + "\n")
    (OUT / "getevent.log").write_text("\n".join(payload.get("box", {}).get("lines", [])) + "\n")
    print(json.dumps({"status": payload["status"], "box": payload.get("box"), "army": payload.get("army"), "error": payload.get("error")}))
    return 0 if payload["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
