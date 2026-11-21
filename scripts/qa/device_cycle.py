#!/usr/bin/env python3
"""One coordinated device cycle for a green build (top-6 parity).

Exactly one launch per build. The cycle proves the UI wiring that headless
gates cannot: worker selection, hall build, one trained unit, tap mapping, and
the box-select boundary with a real `getevent` capture. No retries: every step
records its observed result, and a failed step is evidence, not a rerun.

Usage:
  python3 scripts/qa/device_cycle.py --apk build/android-p0/release/wc3rust-p0.apk \
      --out analysis/agent-qa/parity/device-cycle
"""

from __future__ import annotations

import os
if os.environ.get("H2_DEVICE_SLOT") != "1":
    print("DEVICE_SLOT: not granted (set H2_DEVICE_SLOT=1 explicitly)")
    raise SystemExit(3)

import argparse
import hashlib
import importlib.util
import json
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

import numpy as np
from PIL import Image

from scripts.qa import analyze
from scripts.qa.common import COMPONENT, PKG, QaError
from scripts.qa.device import Device

ACCEPT_DIR = ROOT / "analysis" / "agent-qa" / "acceptance"
if str(ACCEPT_DIR) not in sys.path:
    sys.path.insert(0, str(ACCEPT_DIR))
import console_layout  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "acceptance_helpers", ACCEPT_DIR / "run_acceptance.py"
)
helpers = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(helpers)
state_samples = helpers.state_samples
parse_game_hud = helpers.parse_game_hud
world_rect = helpers.world_rect

GETEVENT_REMOTE = "/data/local/tmp/qa-getevent.log"
PLACEMENT_CANDIDATES = [
    (0.68, 0.27),
    (0.72, 0.29),
    (0.70, 0.31),
    (0.75, 0.32),
    (0.62, 0.55),
    (0.55, 0.65),
]


def now_utc() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def last_state(logs: str) -> dict:
    rows = state_samples(logs)
    return rows[-1] if rows else {}


def blue_boxes(image: np.ndarray, lay: dict, w: int, h: int) -> list[list[int]]:
    rect = world_rect(lay, w, h)
    units = analyze.detect_units(image, rect)
    boxes = []
    for cluster in units.get("blue_clusters", []):
        bw = cluster[2] - cluster[0] + 1
        bh = cluster[3] - cluster[1] + 1
        if 4 <= min(bw, bh) and max(bw, bh) <= 140 and cluster[4] >= 8:
            boxes.append(
                [
                    rect[0] + cluster[0],
                    rect[1] + cluster[1],
                    rect[0] + cluster[2],
                    rect[1] + cluster[3],
                ]
            )
    return boxes


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", type=Path, default=ROOT / "build" / "android-p0" / "release" / "wc3rust-p0.apk")
    parser.add_argument("--out", type=Path, default=ROOT / "analysis" / "agent-qa" / "parity" / "device-cycle")
    args = parser.parse_args(argv)
    out = args.out.resolve()
    shots = out / "screens"
    shots.mkdir(parents=True, exist_ok=True)

    device = Device()
    cycle: dict = {
        "schema": "wc3rust-device-cycle-v1",
        "timestamp": now_utc(),
        "apk": {"path": str(args.apk), "sha256": sha256(args.apk)},
        "phase": "top-6-one-cycle",
        "steps": {},
        "status": "FAIL",
    }
    display = None
    try:
        # K-01: this script never installs, so the package on the device must
        # already be byte-identical to the artifact named in this report.
        installed_sha = device.installed_apk_sha256()
        cycle["apk"]["installed_sha256"] = installed_sha
        cycle["apk"]["install_matches_built"] = (
            installed_sha is not None and installed_sha == cycle["apk"]["sha256"]
        )
        if not cycle["apk"]["install_matches_built"]:
            raise QaError(
                "installed base.apk does not match the built APK: "
                f"installed={installed_sha} built={cycle['apk']['sha256']} "
                "(install the built artifact before a device cycle)"
            )
        launch = device.launch(apk=args.apk, install=False)
        display = launch["display"]
        w, h = display["w"], display["h"]
        safe = launch.get("safe_insets") or launch["insets"]
        cycle["package"] = device.package_version()
        cycle["display"] = display
        cycle["launch"] = launch

        def shot(name: str) -> str:
            device.screenshot(shots / name, display)
            return str((shots / name).relative_to(ROOT))

        cycle["steps"]["start"] = {"screenshot": shot("start.png")}
        lay = {**console_layout.layout(w, h, safe), **(parse_game_hud(device.logs()) or {})}
        slots = lay.get("card_slots") or []
        if len(slots) < 8:
            raise QaError(f"card slots={len(slots)} (need >=8)")

        def tap_slot(index: int) -> None:
            box = slots[index]
            device.tap((box[0] + box[2]) / 2, (box[1] + box[3]) / 2)

        # Centre on the base; the opening camera can leave the workers off-screen.
        tap_slot(2)
        time.sleep(0.8)
        cycle["steps"]["base"] = {"screenshot": shot("base.png")}

        # ARMY (card slot 1) selects every player non-building unit, including
        # peasants; the worker card then exposes BUILD at slot 7. This is the
        # source-verified mapping in build_buttons()/card_button_indices().
        tap_slot(1)
        deadline = time.time() + 4.0
        selected = 0
        while time.time() < deadline:
            selected = int(last_state(device.logs()).get("selected", 0) or 0)
            if selected >= 1:
                break
            time.sleep(0.3)
        image = np.asarray(Image.open(shots / "base.png").convert("RGB"))
        boxes = blue_boxes(image, lay, w, h)
        cycle["steps"]["select"] = {
            "blue_boxes": len(boxes),
            "selected": selected,
            "button": "ARMY",
            "screenshot": shot("selected.png"),
        }
        if selected < 1:
            raise QaError("ARMY selected no units")

        # Build: worker -> BUILD card -> a free ground point.
        before_build = device.logs().count("PLACE_BUILT htow")
        tap_slot(7)
        time.sleep(0.7)
        attempts = []
        built = False
        placed_at = None
        for px, py in PLACEMENT_CANDIDATES:
            device.tap(w * px, h * py)
            time.sleep(0.7)
            logs = device.logs()
            attempts.append(
                {
                    "point": [px, py],
                    "built": "PLACE_BUILT htow" in logs,
                    "blocked": "PLACE_BLOCKED" in logs,
                }
            )
            if "PLACE_BUILT htow cost=" in logs:
                built = True
                placed_at = (px, py)
                break
        cycle["steps"]["build"] = {
            "attempts": attempts,
            "place_built": next(
                (line for line in device.logs().splitlines() if "PLACE_BUILT htow" in line),
                None,
            ),
            "screenshot": shot("after-build.png"),
            "built": built,
            "placed_at": placed_at,
        }

        # Train: select the new hall at the placement point, then card slot 3.
        train_ok = False
        train_logs: list[str] = []
        train_requested = False
        if built:
            # The new hall is selected by place_building; card slot 3 is the
            # first data-driven unit entry (Rifleman on this install).
            baseline_units = int(last_state(device.logs()).get("units", 0) or 0)
            before_train = device.logs().count("TRAIN ")
            tap_slot(3)
            time.sleep(1.2)
            logs = device.logs()
            train_logs = [line for line in logs.splitlines() if "TRAIN " in line][-4:]
            train_requested = logs.count("TRAIN ") > before_train
            if train_requested:
                deadline = time.time() + 32
                while time.time() < deadline:
                    state = last_state(device.logs())
                    if int(state.get("units", 0) or 0) > baseline_units:
                        train_ok = True
                        break
                    time.sleep(1.5)
        cycle["steps"]["train"] = {
            "ok": train_ok,
            "requested": train_requested,
            "logs": train_logs,
            "screenshot": shot("after-train.png"),
        }

        # Box-select boundary with a real getevent capture.
        device.su(f"sh -c ': > {GETEVENT_REMOTE}'", timeout=15)
        device.su(f"sh -c 'timeout 10 getevent -lt > {GETEVENT_REMOTE} 2>&1 &'", timeout=15)
        time.sleep(0.5)
        sel_before = int(last_state(device.logs()).get("selected", 0) or 0)
        x1, y1 = w * 0.42, h * 0.28
        x2, y2 = w * 0.72, h * 0.48
        drag = device.timed_drag(x1, y1, x2, y2, hold_ms=450, steps=10, step_ms=25)
        time.sleep(2.0)
        getevent = device.su(f"cat {GETEVENT_REMOTE}", timeout=20).stdout or ""
        getevent_lines = [line for line in getevent.splitlines() if line.strip()]
        getevent_ok = bool(getevent_lines) and "Usage: getevent" not in getevent
        injected_seen = getevent_ok and any(
            marker in getevent
            for marker in ("EV_ABS", "EV_KEY", "0003", "0001", "SYN_REPORT")
        )
        time.sleep(0.5)
        sel_after = int(last_state(device.logs()).get("selected", 0) or 0)
        cycle["steps"]["box"] = {
            "drag": {"x1": x1, "y1": y1, "x2": x2, "y2": y2, "hold_ms": 450, "steps": 10},
            "helper": drag,
            "selected_before": sel_before,
            "selected_after": sel_after,
            "getevent_lines": len(getevent_lines),
            "getevent_ok": getevent_ok,
            "getevent_injected_seen": injected_seen,
            "getevent_head": getevent_lines[:40],
            "screenshot_before": shot("box-before.png"),
            "screenshot_after": shot("box-after.png"),
            "classification": "harness_boundary" if sel_before == sel_after else "box_select_observed",
        }

        # Tap mapping control: a known card must log BUTTON BASE.
        before_base = device.logs().count("BUTTON BASE")
        tap_slot(2)
        time.sleep(0.7)
        tap_ok = device.logs().count("BUTTON BASE") > before_base
        cycle["steps"]["tap_proof"] = {"ok": tap_ok, "button": "BASE"}

        # Memory reading while the app is alive.
        mem = device.su(f"dumpsys meminfo {PKG}", timeout=30).stdout or ""
        total = None
        for line in mem.splitlines():
            if line.strip().startswith("TOTAL"):
                parts = line.split()
                if len(parts) >= 2 and parts[1].isdigit():
                    total = int(parts[1])
                    break
        cycle["steps"]["memory"] = {"pss_kb": total}

        cycle["status"] = "PASS" if (built and train_ok and tap_ok) else "FAIL"
    except Exception as error:  # one cycle: record, never retry
        cycle["error"] = f"{type(error).__name__}: {error}"
        cycle["status"] = "FAIL"
    finally:
        cleanup: dict = {}
        try:
            proc = device.su(f"am force-stop {PKG}", timeout=20)
            cleanup["force_stop_rc"] = proc.returncode
            # Return the shared device to Termux (the game component is not it).
            proc = device.su(
                "am start -W -n com.termux/com.termux.app.TermuxActivity", timeout=30
            )
            cleanup["termux_rc"] = proc.returncode
            deadline = time.time() + 8.0
            while time.time() < deadline:
                focus = device.focus()
                if "com.termux" in focus:
                    break
                time.sleep(0.6)
            cleanup["focus"] = device.focus()
        except Exception as error:
            cleanup["error"] = f"{type(error).__name__}: {error}"
        cycle["cleanup"] = cleanup

    (out).mkdir(parents=True, exist_ok=True)
    (out / "cycle.json").write_text(json.dumps(cycle, indent=2) + "\n")
    (out / "getevent.log").write_text(
        "\n".join(cycle.get("steps", {}).get("box", {}).get("getevent_head", [])) + "\n"
    )
    lines = [
        "# Device cycle — top-6 parity",
        "",
        f"Timestamp: **{cycle['timestamp']}**",
        f"APK sha256: `{cycle['apk']['sha256']}`",
        f"Installed base.apk sha256: `{cycle['apk'].get('installed_sha256')}` "
        f"(match={cycle['apk'].get('install_matches_built')})",
        f"Status: **{cycle['status']}**",
        "",
        f"- Build: {cycle.get('steps', {}).get('build', {}).get('built')} "
        f"{cycle.get('steps', {}).get('build', {}).get('place_built')}",
        f"- Train: {cycle.get('steps', {}).get('train', {}).get('ok')}",
        f"- Tap proof: {cycle.get('steps', {}).get('tap_proof', {}).get('ok')}",
        f"- Box: {cycle.get('steps', {}).get('box', {}).get('classification')} "
        f"selected {cycle.get('steps', {}).get('box', {}).get('selected_before')} -> "
        f"{cycle.get('steps', {}).get('box', {}).get('selected_after')}, "
        f"getevent lines {cycle.get('steps', {}).get('box', {}).get('getevent_lines')}, "
        f"injected seen {cycle.get('steps', {}).get('box', {}).get('getevent_injected_seen')}",
        f"- PSS: {cycle.get('steps', {}).get('memory', {}).get('pss_kb')} kB",
        "",
        f"Cleanup: {cycle.get('cleanup')}",
        "",
    ]
    (out / "REPORT.md").write_text("\n".join(lines) + "\n")
    print(json.dumps({"status": cycle["status"], "out": str(out), "error": cycle.get("error")}))
    return 0 if cycle["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
