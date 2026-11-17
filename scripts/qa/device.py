#!/usr/bin/env python3
"""Root device runner for wc3rust QA (Termux/Android, no adb required).

Why `su -c` instead of adb: adb is not attached to this device; KernelSU root
can drive the local Android shell directly. Every privileged action is quoted
and bounded; nothing here deletes data or changes system settings.

Commands:
    device.py check                          # root + display + package + APK facts
    device.py info --json                    # display/rotation/density/insets/focus
    device.py install [--apk path]           # pm install -r
    device.py launch [--apk path] [--install] [--json]
    device.py screenshot OUT.png             # exact screencap, retried
    device.py tap X Y
    device.py swipe X1 Y1 X2 Y2 [--ms 300]
    device.py pinch CX CY START END [--ms 400]
    device.py logs --out FILE [--grep TAG]
    device.py state --json                   # GAME_STATE + focus + screen

Coordinates are always screenshot/display pixels in the current rotation; the
screen fact helper rejects a screenshot whose size disagrees with the display
so a stale portrait buffer can never annotate a landscape run.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

from . import common
from .common import COMPONENT, PKG, QaError, rel, su, su_ok

DEFAULT_APK = common.ROOT / "build" / "android-p0" / "release" / "wc3rust-p0.apk"
TMP_PINCH = "/data/local/tmp/h2qa-pinch.sh"


class Device:
    def __init__(self, density_fallback: float = 480.0):
        self.root = common.root_identity()
        self.density_fallback = density_fallback

    # ------------------------------------------------------------- basic facts

    def su(self, cmd: str, timeout: float = 60.0):
        return su(cmd, timeout)

    def package_installed(self) -> bool:
        proc = su(f"pm path {PKG}", timeout=20)
        return proc.returncode == 0 and "package:" in proc.stdout

    def package_version(self) -> dict:
        proc = su(f"dumpsys package {PKG}", timeout=20)
        out = proc.stdout
        version = re.search(r"versionName=(\S+)", out)
        updated = re.search(r"lastUpdateTime=([^\n]+)", out)
        return {
            "installed": "package:" in su(f"pm path {PKG}", timeout=20).stdout,
            "version": version.group(1) if version else None,
            "last_update": updated.group(1).strip() if updated else None,
        }

    def display(self) -> dict:
        """Return current display size/rotation/density.

        `dumpsys window displays` prints `DisplayFrames w=2414 h=1080 r=1`
        while an app is landscape; `wm size` is the natural override.
        """
        frames = None
        proc = su("dumpsys window displays", timeout=25)
        match = re.search(r"DisplayFrames w=(\d+) h=(\d+) r=(\d+)", proc.stdout)
        if match:
            frames = {
                "w": int(match.group(1)),
                "h": int(match.group(2)),
                "rotation": int(match.group(3)),
            }
        size = su("wm size", timeout=15).stdout
        override = re.search(r"Override size: (\d+)x(\d+)", size)
        physical = re.search(r"Physical size: (\d+)x(\d+)", size)
        density_out = su("wm density", timeout=15).stdout
        density = re.search(r"Override density: (\d+)", density_out) or re.search(
            r"Physical density: (\d+)", density_out
        )
        base = frames or {}
        if override:
            base["natural_w"] = int(override.group(1))
            base["natural_h"] = int(override.group(2))
        elif physical:
            base["natural_w"] = int(physical.group(1))
            base["natural_h"] = int(physical.group(2))
        base["density"] = float(density.group(1)) if density else self.density_fallback
        return base

    def insets(self, w: int, h: int, tries: int = 6) -> dict:
        """System-bar insets in screenshot coordinates.

        Parses `InsetsSource` frames from `dumpsys window`. The frames are in
        the current rotation's coordinate space; if they do not fit the
        screenshot size we retry (rotation settling) instead of guessing.
        """
        last = None
        for attempt in range(tries):
            proc = su("dumpsys window", timeout=25)
            out = proc.stdout
            status = self._inset_rect(out, "statusBars", w, h)
            nav = self._inset_rect(out, "navigationBars", w, h)
            cutout = self._inset_rect(out, "displayCutout", w, h)
            last = {"status": status, "nav": nav, "cutout": cutout}
            if status or nav or cutout:
                # statusBars/nav are always present on this device; empty means
                # the frame did not fit the screenshot yet.
                if (status is not None) and (nav is not None):
                    return self._merge_insets(status, nav, cutout, w, h, out)
            time.sleep(0.35 * (attempt + 1))
        # Fallback: the app task snapshot carries content insets even when the
        # live InsetsSource frames are stale (task switch / rotation race).
        proc = su("dumpsys window", timeout=25)
        match = re.search(
            r"mTopActivityComponent=" + re.escape(PKG) + r"[^\n]*mContentInsets=\[(\d+),(\d+)\]\[(\d+),(\d+)\]",
            proc.stdout,
        )
        if match:
            left, top, right, bottom = map(int, match.groups())
            if top < h and bottom < h and left < w and right < w:
                frame = {"visible": True, "side": "top"}
                return {
                    "left": left,
                    "top": top,
                    "right": right,
                    "bottom": bottom,
                    "status_visible": top > 0,
                    "nav_visible": bottom > 0,
                    "cutout_visible": left > 0,
                    "status_frame": frame if top else None,
                    "nav_frame": frame if bottom else None,
                    "cutout_frame": frame if left else None,
                    "density": None,
                }
        return self._merge_insets(last["status"], last["nav"], last["cutout"], w, h, "")

    @staticmethod
    def _inset_rect(out: str, kind: str, w: int, h: int) -> dict | None:
        pattern = re.compile(
            r"InsetsSource id=\S+ type=" + kind + r" frame=\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\] visible=(\w+)"
        )
        for match in pattern.finditer(out):
            x0, y0, x1, y1 = map(int, match.groups()[:4])
            visible = match.group(5) == "true"
            if x0 < 0 or y0 < 0 or x1 > w or y1 > h:
                return None
            rect = {
                "x0": x0,
                "y0": y0,
                "x1": x1,
                "y1": y1,
                "visible": visible,
                "side": Device._side(x0, y0, x1, y1, w, h),
            }
            # A frame that spans the full display in both axes is not an inset.
            if x1 - x0 >= w and y1 - y0 >= h:
                return None
            return rect
        return None

    @staticmethod
    def _side(x0: int, y0: int, x1: int, y1: int, w: int, h: int) -> str:
        if y0 == 0 and x1 <= w and y1 < h / 2:
            return "top"
        if y1 == h and x0 == 0 and x1 <= w and y0 > h / 2:
            return "bottom"
        if x0 == 0 and y1 <= h and x1 < w / 2:
            return "left"
        if x1 == w and y1 <= h and x0 > w / 2:
            return "right"
        return "other"

    @staticmethod
    def _merge_insets(status, nav, cutout, w: int, h: int, raw: str) -> dict:
        def edges(rect):
            if not rect:
                return {"left": 0, "top": 0, "right": 0, "bottom": 0}
            side = rect["side"]
            return {
                "left": rect["x1"] if side == "left" else 0,
                "top": rect["y1"] if side == "top" else 0,
                "right": w - rect["x0"] if side == "right" else 0,
                "bottom": h - rect["y0"] if side == "bottom" else 0,
            }

        st = edges(status)
        nv = edges(nav)
        co = edges(cutout)
        merged = {
            "left": max(st["left"], nv["left"], co["left"]),
            "top": max(st["top"], nv["top"], co["top"]),
            "right": max(st["right"], nv["right"], co["right"]),
            "bottom": max(st["bottom"], nv["bottom"], co["bottom"]),
        }
        return {
            **merged,
            "status_visible": bool(status and status["visible"]),
            "nav_visible": bool(nav and nav["visible"]),
            "cutout_visible": bool(cutout and cutout["visible"]),
            "status_frame": status,
            "nav_frame": nav,
            "cutout_frame": cutout,
            "density": None,
        }

    def focus(self) -> str:
        out = su("dumpsys window", timeout=25).stdout
        match = re.search(r"mCurrentFocus=Window\{\S+ \S+ ([^}]+)\}", out)
        if match:
            return match.group(1).strip()
        match = re.search(r"mCurrentFocus=(\S+)", out)
        return match.group(1).strip() if match else ""

    def app_focused(self) -> bool:
        focus = self.focus()
        return PKG in focus

    def ensure_foreground(self, timeout: float = 8.0) -> bool:
        """Bring the game back without resetting it when another app is on top."""
        if self.app_focused():
            return True
        focus = self.focus()
        if "NotificationShade" in focus or "StatusBar" in focus:
            su("cmd statusbar collapse", timeout=15)
            time.sleep(0.3)
            if "NotificationShade" in self.focus():
                su("input keyevent KEYCODE_BACK", timeout=15)
                time.sleep(0.4)
        su(f"am start -n {COMPONENT}", timeout=30)
        return self.wait_focus(timeout)

    def clean_screenshot(self, path: Path, tries: int = 5) -> tuple[dict, list[list[int]]]:
        """Capture the game without a foreign window overlay (call banner, shade)."""
        from . import analyze

        overlays: list[list[int]] = []
        import numpy as np

        for attempt in range(tries):
            self.ensure_foreground()
            info = self.screenshot(path)
            from PIL import Image

            rgb = np.asarray(Image.open(path).convert("RGB"))
            overlays = analyze.detect_overlays(rgb)
            if not overlays:
                return info, []
            if attempt + 1 < tries:
                su("cmd statusbar collapse", timeout=15)
                time.sleep(1.2 * (attempt + 1))
        return info, overlays

    # --------------------------------------------------------------- screens

    def screencap(self, tries: int = 4) -> bytes:
        last = b""
        for attempt in range(tries):
            proc = subprocess.run(
                ["su", "-c", "screencap -p"],
                capture_output=True,
                timeout=35,
                cwd=str(common.ROOT),
            )
            data = proc.stdout
            if common.png_bytes(data):
                return data
            last = data
            time.sleep(0.5 * (attempt + 1))
        raise QaError(f"screencap returned non-PNG bytes ({len(last)} bytes)")

    def screenshot(self, path: Path, device: dict | None = None) -> dict:
        if not self.ensure_foreground():
            raise QaError(f"game is not foreground before screenshot (focus={self.focus()!r})")
        path.parent.mkdir(parents=True, exist_ok=True)
        data = self.screencap()
        path.write_bytes(data)
        from PIL import Image

        image = Image.open(path)
        if device and (image.width, image.height) != (device["w"], device["h"]):
            raise QaError(
                f"screenshot {image.width}x{image.height} does not match display "
                f"{device['w']}x{device['h']} (rotation {device.get('rotation')}); "
                "the app is not foreground in the expected rotation"
            )
        return {"path": rel(path), "width": image.width, "height": image.height}

    def wait_landscape(self, timeout: float = 12.0) -> dict:
        """Wait for a real landscape framebuffer.

        `dumpsys window displays` can be stale across a task switch, so the
        framebuffer itself decides: a screenshot wider than it is tall is the
        only acceptable state for this game.
        """
        from PIL import Image
        import io

        deadline = time.time() + timeout
        last = None
        while time.time() < deadline:
            if not self.app_focused():
                self.ensure_foreground()
                time.sleep(0.3)
                continue
            data = self.screencap()
            image = Image.open(io.BytesIO(data))
            display = self.display()
            last = display
            if image.width > image.height:
                display["w"] = image.width
                display["h"] = image.height
                display["rotation"] = 1 if image.width > image.height else 0
                return display
            time.sleep(0.4)
        raise QaError(f"display did not rotate to landscape: {last}")

    # ---------------------------------------------------------------- launch

    def install(self, apk: Path) -> str:
        if not apk.exists():
            raise QaError(f"APK missing: {apk}")
        out = su_ok(f"pm install -r '{apk}'", timeout=180).strip()
        if "Success" not in out:
            raise QaError(f"pm install failed: {out[-800:]}")
        return out

    @staticmethod
    def _safe_from_first_frame(line: str) -> dict | None:
        match = re.search(
            r"safe=(?:SafeInsets|SafeArea) \{ left: ([0-9.]+), top: ([0-9.]+), right: ([0-9.]+), bottom: ([0-9.]+) \}",
            line,
        )
        if not match:
            return None
        return {
            "left": float(match.group(1)),
            "top": float(match.group(2)),
            "right": float(match.group(3)),
            "bottom": float(match.group(4)),
        }

    def wait_focus(self, timeout: float = 15.0) -> bool:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.app_focused():
                return True
            time.sleep(0.3)
        return False

    def wait_first_frame(self, timeout: float = 55.0) -> dict:
        deadline = time.time() + timeout
        seen: list[str] = []
        while time.time() < deadline:
            logs = self.logs()
            seen = [line for line in logs.splitlines() if "GAME_" in line]
            if any("GAME_FIRST_FRAME" in line for line in seen):
                first = next(line for line in seen if "GAME_FIRST_FRAME" in line)
                surface = next((line for line in seen if "GAME_SURFACE " in line), "")
                ready = next((line for line in seen if "GAME_READY " in line), "")
                fatal = [line for line in seen if "GAME_FATAL" in line]
                return {
                    "first_frame": first.strip(),
                    "surface": surface.strip(),
                    "ready": ready.strip(),
                    "fatal": fatal[-1].strip() if fatal else None,
                }
            fatal = [line for line in seen if "GAME_FATAL" in line]
            if fatal:
                raise QaError(f"native fatal before first frame: {fatal[-1]}")
            time.sleep(0.7)
        tail = "\n".join(seen[-8:])
        raise QaError(f"no GAME_FIRST_FRAME within {timeout:.0f}s; tail:\n{tail}")

    def launch(self, apk: Path | None = None, install: bool = False, attempts: int = 3) -> dict:
        if install:
            if apk is None:
                apk = DEFAULT_APK
            self.install(apk)
        errors: list[str] = []
        for attempt in range(1, attempts + 1):
            su(f"am force-stop {PKG}", timeout=20)
            su("logcat -c", timeout=15)
            su("input keyevent KEYCODE_WAKEUP", timeout=15)
            su("wm dismiss-keyguard", timeout=15)
            su("cmd statusbar collapse", timeout=15)
            su(f"pm enable {PKG}", timeout=15)
            if "NotificationShade" in self.focus():
                su("input keyevent KEYCODE_BACK", timeout=15)
                time.sleep(0.4)
            start = su(f"am start -W -n {COMPONENT}", timeout=45)
            if start.returncode != 0:
                errors.append(f"attempt {attempt}: am start rc={start.returncode} {start.stderr.strip()[:200]}")
                time.sleep(1.0)
                continue
            if not self.wait_focus(12):
                focus = self.focus()
                errors.append(f"attempt {attempt}: focus is {focus!r}")
                if "NotificationShade" in focus:
                    su("cmd statusbar collapse", timeout=15)
                    su("input keyevent KEYCODE_BACK", timeout=15)
                time.sleep(1.0)
                continue
            try:
                first = self.wait_first_frame()
            except QaError as error:
                errors.append(f"attempt {attempt}: {error}")
                continue
            if first.get("fatal"):
                errors.append(f"attempt {attempt}: {first['fatal']}")
                continue
            display = self.wait_landscape()
            # The operator shares this device; hold the game foreground long
            # enough to capture, but do not fail the launch if focus flaps.
            held = False
            for _ in range(6):
                if self.app_focused():
                    held = True
                    break
                self.ensure_foreground()
                time.sleep(0.3)
            time.sleep(0.4)
            info = {
                "ok": True,
                "attempt": attempt,
                "component": COMPONENT,
                "display": display,
                "first_frame": first,
                "focus": self.focus(),
            }
            info["insets"] = self.insets(display["w"], display["h"])
            info["insets"]["density"] = display["density"]
            info["safe_insets"] = self._safe_from_first_frame(first.get("first_frame", ""))
            if info["safe_insets"] is None:
                info["safe_insets"] = {
                    "left": info["insets"].get("left", 0),
                    "top": info["insets"].get("top", 0),
                    "right": info["insets"].get("right", 0),
                    "bottom": info["insets"].get("bottom", 0),
                    "source": "window-manager",
                }
            else:
                info["safe_insets"]["source"] = "GAME_FIRST_FRAME" 
            info["version"] = self.package_version()
            return info
        raise QaError("launch failed after retries:\n" + "\n".join(errors))

    # ---------------------------------------------------------------- input

    def tap(self, x: float, y: float, tries: int = 2) -> None:
        for attempt in range(tries):
            self._input(f"input tap {int(round(x))} {int(round(y))}")
            return

    def swipe(self, x1: float, y1: float, x2: float, y2: float, ms: int = 300, tries: int = 2) -> None:
        for attempt in range(tries):
            self._input(
                f"input swipe {int(round(x1))} {int(round(y1))} "
                f"{int(round(x2))} {int(round(y2))} {int(ms)}"
            )
            return

    def timed_drag(
        self,
        x1: float,
        y1: float,
        x2: float,
        y2: float,
        hold_ms: int = 450,
        steps: int = 8,
        step_ms: int = 20,
    ) -> dict:
        """Held drag via `input motionevent` in display coordinates.

        `input draganddrop` injects its first MOVE immediately, so the game sees
        a pan instead of a held box gesture (`wc3-input/src/lib.rs:262-268`
        requires >=0.30 s hold before the first move). This helper uses the same
        display coordinates that already work for `input tap`.
        """
        self.ensure_foreground()
        lines = ["#!/system/bin/sh", "set -e"]
        lines.append(f"input motionevent DOWN {int(round(x1))} {int(round(y1))}")
        lines.append(f"sleep {hold_ms / 1000.0:.3f}")
        for step in range(1, steps + 1):
            t = step / steps
            x = int(round(x1 + (x2 - x1) * t))
            y = int(round(y1 + (y2 - y1) * t))
            lines.append(f"input motionevent MOVE {x} {y}")
            if step < steps:
                lines.append(f"sleep {step_ms / 1000.0:.3f}")
        lines.append(f"input motionevent UP {int(round(x2))} {int(round(y2))}")
        local = common.ANALYSIS / "timed-drag.sh"
        local.parent.mkdir(parents=True, exist_ok=True)
        local.write_text("\n".join(lines) + "\n")
        su_ok(f"sh '{local}'", timeout=40)
        return {
            "ok": True,
            "method": "input motionevent",
            "start": [x1, y1],
            "end": [x2, y2],
            "hold_ms": hold_ms,
            "steps": steps,
            "script": rel(local),
        }

    def drag_and_drop(self, x1: float, y1: float, x2: float, y2: float, ms: int = 800) -> None:
        """Long-press drag (Android input draganddrop) for box selection."""
        self._input(
            f"input draganddrop {int(round(x1))} {int(round(y1))} "
            f"{int(round(x2))} {int(round(y2))} {int(ms)}"
        )

    def long_press(self, x: float, y: float, ms: int = 650) -> None:
        self._input(f"input swipe {int(round(x))} {int(round(y))} {int(round(x))} {int(round(y))} {int(ms)}")

    def _input(self, cmd: str) -> None:
        # If another app (or the terminal) is in front, bring the game back
        # before injecting touch; otherwise the tap would land on that app.
        if not self.app_focused():
            self.ensure_foreground()
        proc = su(cmd, timeout=25)
        if proc.returncode != 0:
            raise QaError(f"input failed rc={proc.returncode}: {cmd}\n{proc.stderr.strip()[:300]}")

    # ---------------------------------------------------------------- pinch

    def touch_matrix(self) -> dict:
        """Raw touchscreen → display transform from `dumpsys input`.

        The InputReader prints an affine 3x3 `RawToDisplay Transform` for the
        active viewport; it already includes rotation, so the pinch helper does
        not need a device-specific rotation table.
        """
        out = su("dumpsys input", timeout=30).stdout
        device_no = None
        for line in out.splitlines():
            match = re.match(r"\s*Device (\d+): touchpanel\b", line)
            if match:
                device_no = int(match.group(1))
                break
        if device_no is None:
            raise QaError("no touchpanel device in dumpsys input")
        start = out.find(f"Device {device_no}: touchpanel")
        end = out.find(f"Device {device_no + 1}:", start)
        block = out[start : end if end > 0 else len(out)]
        matrix = None
        match = re.search(
            r"RawToDisplay Transform:[^\n]*\n\s*([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\n"
            r"\s*([-\d.]+)\s+([-\d.]+)\s+([-\d.]+)\n",
            block,
        )
        if match:
            vals = [float(v) for v in match.groups()]
            matrix = [vals[0:3], vals[3:6], [0.0, 0.0, 1.0]]
        axes = {}
        for axis in ("X", "Y"):
            am = re.search(rf"Raw Touch Axes:\s*\n\s*{axis}: min=(\d+), max=(\d+)", block)
            if am:
                axes[axis] = (int(am.group(1)), int(am.group(2)))
        slot = 0
        sm = re.search(r"Slot: min=0, max=(\d+)", block)
        if sm:
            slot = int(sm.group(1))
        device_path = None
        getev = su("getevent -pl", timeout=20).stdout
        for line in getev.splitlines():
            if "touchpanel" in line and "add device" in line:
                device_path = line.rsplit(":", 1)[-1].strip()
        if not device_path:
            device_path = "/dev/input/event5"
        display = self.display()
        return {
            "path": device_path,
            "matrix": matrix,
            "axes": axes,
            "slot_max": slot,
            "display": display,
        }

    def pinch(self, cx: float, cy: float, start: float, end: float, ms: int = 420, steps: int = 10) -> dict:
        info = self.touch_matrix()
        matrix = info["matrix"]
        if matrix is None:
            raise QaError("touchscreen RawToDisplay matrix not found")
        det = matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]
        if abs(det) < 1e-9:
            raise QaError("singular touch transform")

        def to_raw(x: float, y: float) -> tuple[float, float]:
            # invert the affine 2x2 + translation
            a, b, tx = matrix[0]
            c, d, ty = matrix[1]
            px, py = x - tx, y - ty
            rx = (px * d - py * b) / det
            ry = (a * py - c * px) / det
            return rx, ry

        ax = info["axes"].get("X", (0, 12399))
        ay = info["axes"].get("Y", (0, 27719))

        def clamp(v, lo, hi):
            return max(lo, min(hi, int(round(v))))

        def points(dist: float):
            half = dist * 0.5
            p1 = to_raw(cx - half, cy)
            p2 = to_raw(cx + half, cy)
            return (
                clamp(p1[0], ax[0], ax[1]),
                clamp(p1[1], ay[0], ay[1]),
                clamp(p2[0], ax[0], ax[1]),
                clamp(p2[1], ay[0], ay[1]),
            )

        lines = ["#!/system/bin/sh", "set -e"]
        lines.append(f"sendevent {info['path']} 3 47 0")
        lines.append(f"sendevent {info['path']} 3 57 1001")
        lines.append(f"sendevent {info['path']} 1 330 1")
        first = None
        for step in range(steps + 1):
            t = step / steps
            dist = start + (end - start) * t
            x1, y1, x2, y2 = points(dist)
            if first is None:
                first = (x1, y1, x2, y2)
            lines.append(f"sendevent {info['path']} 3 47 0")
            lines.append(f"sendevent {info['path']} 3 57 1001")
            lines.append(f"sendevent {info['path']} 3 53 {x1}")
            lines.append(f"sendevent {info['path']} 3 54 {y1}")
            lines.append(f"sendevent {info['path']} 3 48 6")
            lines.append(f"sendevent {info['path']} 3 47 1")
            lines.append(f"sendevent {info['path']} 3 57 1002")
            lines.append(f"sendevent {info['path']} 3 53 {x2}")
            lines.append(f"sendevent {info['path']} 3 54 {y2}")
            lines.append(f"sendevent {info['path']} 3 48 6")
            lines.append(f"sendevent {info['path']} 0 0 0")
            if step < steps:
                lines.append("sleep 0.02")
        lines.append(f"sendevent {info['path']} 3 47 0")
        lines.append(f"sendevent {info['path']} 3 57 -1")
        lines.append(f"sendevent {info['path']} 3 47 1")
        lines.append(f"sendevent {info['path']} 3 57 -1")
        lines.append(f"sendevent {info['path']} 1 330 0")
        lines.append(f"sendevent {info['path']} 0 0 0")
        script = "\n".join(lines) + "\n"
        local = common.ANALYSIS / "pinch.sh"
        local.parent.mkdir(parents=True, exist_ok=True)
        local.write_text(script)
        su_ok(f"sh '{local}'", timeout=30)
        return {
            "ok": True,
            "device": info["path"],
            "matrix": matrix,
            "start": start,
            "end": end,
            "first_raw": first,
            "script": rel(local),
        }

    # ----------------------------------------------------------------- logs

    def logs(self) -> str:
        proc = su("logcat -d -v brief", timeout=30)
        return proc.stdout

    def log_tail(self, lines: int = 400, needle: str | None = None) -> str:
        text = self.logs()
        rows = text.splitlines()
        if needle:
            rows = [row for row in rows if needle in row]
        return "\n".join(rows[-lines:])

    def pid(self) -> str:
        proc = su(f"pidof {PKG}", timeout=15)
        return proc.stdout.strip()

    def abort_count(self) -> int:
        return self.logs().count("Abort message:")

    def state_line(self) -> str:
        text = self.logs()
        rows = [row for row in text.splitlines() if "GAME_STATE " in row]
        return rows[-1] if rows else ""


# ------------------------------------------------------------------ CLI


def _print(payload) -> None:
    if isinstance(payload, (dict, list)):
        print(json.dumps(payload, indent=2))
    else:
        print(payload)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="action", required=True)

    sub.add_parser("check")
    info = sub.add_parser("info")
    info.add_argument("--json", action="store_true")
    inst = sub.add_parser("install")
    inst.add_argument("--apk", type=Path, default=DEFAULT_APK)
    launch = sub.add_parser("launch")
    launch.add_argument("--apk", type=Path, default=DEFAULT_APK)
    launch.add_argument("--install", action="store_true")
    launch.add_argument("--json", action="store_true")
    shot = sub.add_parser("screenshot")
    shot.add_argument("out", type=Path)
    shot.add_argument("--json", action="store_true")
    tap = sub.add_parser("tap")
    tap.add_argument("x", type=float)
    tap.add_argument("y", type=float)
    swipe = sub.add_parser("swipe")
    swipe.add_argument("x1", type=float)
    swipe.add_argument("y1", type=float)
    swipe.add_argument("x2", type=float)
    swipe.add_argument("y2", type=float)
    swipe.add_argument("--ms", type=int, default=300)
    pinch = sub.add_parser("pinch")
    pinch.add_argument("cx", type=float)
    pinch.add_argument("cy", type=float)
    pinch.add_argument("start", type=float)
    pinch.add_argument("end", type=float)
    pinch.add_argument("--ms", type=int, default=420)
    logs = sub.add_parser("logs")
    logs.add_argument("--out", type=Path)
    logs.add_argument("--grep", default=None)
    logs.add_argument("--lines", type=int, default=400)
    state = sub.add_parser("state")
    state.add_argument("--json", action="store_true")

    args = parser.parse_args(argv)
    try:
        if args.action == "check":
            device = Device()
            _print(
                {
                    "root": device.root,
                    "package": device.package_version(),
                    "display": device.display(),
                    "focus": device.focus(),
                    "apk": {
                        "path": rel(DEFAULT_APK),
                        "exists": DEFAULT_APK.exists(),
                        "bytes": DEFAULT_APK.stat().st_size if DEFAULT_APK.exists() else 0,
                    },
                }
            )
            return 0
        if args.action == "info":
            device = Device()
            display = device.display()
            payload = {"display": display, "focus": device.focus(), "package": device.package_version()}
            if display["w"] and display["h"]:
                payload["insets"] = device.insets(display["w"], display["h"])
                payload["insets"]["density"] = display["density"]
            _print(payload)
            return 0
        if args.action == "install":
            print(Device().install(args.apk))
            return 0
        if args.action == "launch":
            print(json.dumps(Device().launch(apk=args.apk, install=args.install), indent=2))
            return 0
        if args.action == "screenshot":
            device = Device()
            display = device.display()
            info = device.screenshot(args.out, display if display["w"] > display["h"] else None)
            print(json.dumps(info, indent=2))
            return 0
        if args.action == "tap":
            Device().tap(args.x, args.y)
            return 0
        if args.action == "swipe":
            Device().swipe(args.x1, args.y1, args.x2, args.y2, args.ms)
            return 0
        if args.action == "pinch":
            print(json.dumps(Device().pinch(args.cx, args.cy, args.start, args.end, args.ms), indent=2))
            return 0
        if args.action == "logs":
            device = Device()
            text = device.log_tail(args.lines, args.grep)
            if args.out:
                args.out.parent.mkdir(parents=True, exist_ok=True)
                args.out.write_text(text + "\n")
                print(rel(args.out))
            else:
                print(text)
            return 0
        if args.action == "state":
            device = Device()
            print(json.dumps({"focus": device.focus(), "state": device.state_line()}, indent=2))
            return 0
    except QaError as error:
        common.fail(str(error))
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
