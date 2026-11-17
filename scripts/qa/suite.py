#!/usr/bin/env python3
"""Deterministic QA gate for wc3rust: build -> install -> visual -> gameplay.

Every phase is bounded and writes JSON evidence under `analysis/agent-qa/latest/`.
Exit code 0 only when every enabled hard gate passes.

    python3 -m scripts.qa.suite --build --install --long 0
    python3 -m scripts.qa.suite --no-build --fast
    python3 -m scripts.qa.suite --gallery
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from types import SimpleNamespace

from . import analyze, common
from .device import DEFAULT_APK, Device

LATEST = common.ANALYSIS / "latest"


def _run(cmd: list[str], log: Path, env: dict | None = None, timeout: float = 1800) -> tuple[int, str]:
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as handle:
        proc = subprocess.run(
            cmd,
            cwd=str(common.ROOT),
            stdout=handle,
            stderr=subprocess.STDOUT,
            env={**os.environ, **(env or {})},
            timeout=timeout,
        )
    return proc.returncode, log.read_text(errors="replace")[-4000:]


def fps_from_logs(device: Device) -> float | None:
    line = device.state_line()
    match = re.search(r"fps=([0-9.]+)", line)
    return float(match.group(1)) if match else None


def run(args) -> dict:
    LATEST.mkdir(parents=True, exist_ok=True)
    phases: dict[str, dict] = {}
    device = Device()
    (LATEST / "root.txt").write_text(device.root + "\n")

    # ---------------------------------------------------------------- build
    if args.build:
        print("[qa] build: cargo release + APK")
        rc1, tail1 = _run(
            ["cargo", "build", "--release", "-p", "wc3rust-android"],
            LATEST / "build-cargo.log",
            timeout=1800,
        )
        rc2, tail2 = _run(["bash", "scripts/build-apk.sh"], LATEST / "build-apk.log", timeout=600)
        phases["build"] = {
            "ok": rc1 == 0 and rc2 == 0,
            "cargo_rc": rc1,
            "apk_rc": rc2,
            "apk": common.rel(DEFAULT_APK) if DEFAULT_APK.exists() else None,
            "tail": (tail1[-1200:] if rc1 else "") + (tail2[-800:] if rc2 else ""),
        }
    else:
        installed = "package:" in common.su(f"pm path {common.PKG}", timeout=20).stdout
        phases["build"] = {
            "ok": DEFAULT_APK.exists() or installed,
            "skipped": True,
            "apk": common.rel(DEFAULT_APK) if DEFAULT_APK.exists() else None,
            "installed_fallback": installed,
        }

    if not DEFAULT_APK.exists() and args.install:
        raise SystemExit("APK missing; build first")

    # -------------------------------------------------------------- install
    if args.install:
        print("[qa] install")
        out = device.install(DEFAULT_APK)
        phases["install"] = {"ok": "Success" in out, "output": out[-200:]}
    else:
        phases["install"] = {"ok": True, "skipped": True}

    # --------------------------------------------------------------- launch
    print("[qa] launch")
    launch = device.launch(apk=DEFAULT_APK, install=False)
    common.save_json(LATEST / "launch.json", launch)
    display = launch["display"]
    insets = launch["insets"]
    insets.pop("density", None)
    insets = {k: insets.get(k, 0) for k in ("left", "top", "right", "bottom")}
    phases["launch"] = {"ok": True, "first_frame": launch.get("first_frame"), "display": display, "insets": insets}
    pid0 = device.pid()
    aborts0 = device.abort_count()

    # --------------------------------------------------------------- visual
    print("[qa] visual")
    shot = LATEST / "screen-start.png"
    info, overlays = device.clean_screenshot(shot)
    phases["overlays_after_launch"] = {"ok": True, "overlays": overlays}
    fps = fps_from_logs(device)
    state_line = device.state_line()
    selected = 0
    m = re.search(r"selected=(\d+)", state_line)
    if m:
        selected = int(m.group(1))
    report = analyze.build(shot, insets, display["density"], fps, launch, launch.get("safe_insets"), selected)
    report["analyzed_at"] = common.now_iso()
    annotated = LATEST / "screen-start.annotated.png"
    analyze.annotate(shot, report, annotated)
    report["annotated"] = common.rel(annotated)
    common.save_json(LATEST / "visual.json", {k: v for k, v in report.items() if not k.startswith("_")})
    phases["visual"] = {
        "ok": report["ok"],
        "fails": [g["id"] for g in report["gates"] if not g["ok"] and g["severity"] == "fail"],
        "warns": [g["id"] for g in report["gates"] if not g["ok"] and g["severity"] == "warn"],
        "screenshot": common.rel(shot),
        "annotated": common.rel(annotated),
        "fps": fps,
    }
    pid1 = device.pid()
    aborts1 = device.abort_count()
    crashed = aborts1 > aborts0 or (pid0 and pid1 and pid0 != pid1)
    phases["crash"] = {
        "ok": not crashed,
        "pid": [pid0, pid1],
        "new_aborts": aborts1 - aborts0,
        "tail": [line for line in device.logs().splitlines() if "Abort message" in line][-4:] if crashed else [],
    }
    print(f"[qa] visual {'PASS' if report['ok'] else 'FAIL'} fails={phases['visual']['fails']}")
    if crashed:
        print(f"[qa] CRASH pid {pid0}->{pid1} aborts+{aborts1 - aborts0}")

    # ------------------------------------------------------------- gameplay
    if args.fast:
        phases["gameplay"] = {"ok": True, "skipped": "--fast"}
    else:
        print("[qa] gameplay")
        from . import gameplay

        gp_args = SimpleNamespace(
            apk=DEFAULT_APK,
            install=False,
            shots=str(LATEST / "gameplay"),
            json_out=LATEST / "gameplay.json",
            long=args.long,
            resume=False,
        )
        try:
            gameplay_report = gameplay.run(gp_args)
            common.save_json(LATEST / "gameplay.json", gameplay_report)
            phases["gameplay"] = {
                "ok": gameplay_report["ok"],
                "fails": gameplay_report.get("fails", []),
                "steps": [
                    {"step": s.get("step"), "ok": s.get("ok"), "severity": s.get("severity", "fail")}
                    for s in gameplay_report.get("steps", [])
                    if "screenshot" not in s
                ],
            }
        except Exception as error:  # noqa: BLE001 - gate must report, not crash
            phases["gameplay"] = {"ok": False, "error": f"{type(error).__name__}: {error}"}
        print(f"[qa] gameplay {'PASS' if phases['gameplay']['ok'] else 'FAIL'} {phases['gameplay'].get('fails') or phases['gameplay'].get('error') or ''}")

    # -------------------------------------------------------------- gallery
    gallery_dir = common.ANALYSIS / "gallery"
    if args.gallery:
        print("[qa] gallery: rendering the full 2048 MDX / 691 icon / 111 terrain corpus")
        env = {
            "GALLERY_ROOT": str(common.ROOT / "analysis" / "war3_full"),
            "GALLERY_OUT": str(gallery_dir),
            "GALLERY_LIMIT": "0",
        }
        rc, tail = _run(
            ["cargo", "test", "--release", "-p", "wc3-game", "--test", "asset_gallery", "--", "--ignored", "--nocapture"],
            LATEST / "gallery-render.log",
            env=env,
            timeout=7200,
        )
        phases["gallery_render"] = {"ok": rc == 0, "rc": rc, "tail": tail[-2000:]}
        if rc != 0:
            phases["gallery"] = {"ok": False, "error": "gallery render failed", "log": common.rel(LATEST / "gallery-render.log")}
        else:
            from . import gallery

            gal = gallery.analyze(gallery_dir, False, 128)
            common.save_json(gallery_dir / "gates.json", gal)
            phases["gallery"] = {"ok": gal["ok"], "fails": [g["id"] for g in gal["gates"] if not g["ok"]], "counts": gal["counts"]}
    elif args.gallery is False:
        phases["gallery"] = {"ok": True, "skipped": "--no-gallery"}
    elif (gallery_dir / "models" / "manifest.jsonl").exists():
        from . import gallery

        gal = gallery.analyze(gallery_dir, False, 128)
        common.save_json(gallery_dir / "gates.json", gal)
        phases["gallery"] = {"ok": gal["ok"], "fails": [g["id"] for g in gal["gates"] if not g["ok"]], "counts": gal["counts"]}
    else:
        phases["gallery"] = {"ok": False, "not_built": True, "detail": "run qa-all.sh --gallery to render the corpus"}

    hard = [name for name, phase in phases.items() if not phase.get("ok")]
    summary = {
        "ok": not hard,
        "hard_failures": hard,
        "phases": phases,
        "device": {"display": display, "insets": insets, "density": display["density"]},
        "apk": common.rel(DEFAULT_APK),
        "analyzed_at": common.now_iso(),
    }
    common.save_json(LATEST / "summary.json", summary)
    return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--build", dest="build", action="store_true", default=True)
    parser.add_argument("--no-build", dest="build", action="store_false")
    parser.add_argument("--install", dest="install", action="store_true", default=True)
    parser.add_argument("--no-install", dest="install", action="store_false")
    parser.add_argument("--fast", action="store_true", help="skip the gameplay self-test")
    parser.add_argument("--long", type=int, default=0, help="seconds to wait for a win/lose overlay")
    parser.add_argument("--gallery", dest="gallery", action="store_true", help="render/grade the full asset corpus")
    parser.add_argument("--no-gallery", dest="gallery", action="store_false", help="skip the corpus gallery gate")
    parser.set_defaults(gallery=None)
    args = parser.parse_args(argv)
    summary = run(args)
    print(json.dumps({"ok": summary["ok"], "hard_failures": summary["hard_failures"], "summary": common.rel(LATEST / "summary.json")}, indent=2))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
