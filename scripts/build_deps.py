#!/usr/bin/env python3
"""Verify the prebuilt/native inputs the APK links against (K-16).

`build-apk.sh` used to copy ``third_party/sdl3-android-build/libSDL3.so``,
``third_party/vulkan-loader/libvulkan.so``, the generated debug keystore and the
Termux ``libiconv``/``libc++_shared`` without recording what they were. None of
those files is tracked, so a fresh clone silently built a different APK.

The expected hashes live in ``scripts/build-deps.json`` (tracked). A mismatch is
fatal: restore the audited artifact or update the manifest deliberately.

Usage:
  scripts/build_deps.py verify [--json]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "scripts" / "build-deps.json"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify() -> dict:
    manifest = json.loads(MANIFEST.read_text())
    results = []
    ok = True
    for raw, spec in manifest.get("files", {}).items():
        path = Path(os.path.expandvars(raw))
        if not path.is_absolute():
            path = ROOT / path
        required = bool(spec.get("required"))
        expected = spec.get("sha256")
        entry = {
            "path": raw,
            "required": required,
            "expected_sha256": expected,
            "expected_bytes": spec.get("bytes"),
            "provider": spec.get("provider"),
        }
        if not path.is_file():
            entry["status"] = "MISSING"
            entry["ok"] = not required and expected is None
            if required or expected is not None:
                ok = False
                entry["reason"] = "required build input is absent"
            results.append(entry)
            continue
        actual = sha256_file(path)
        entry["actual_sha256"] = actual
        entry["actual_bytes"] = path.stat().st_size
        if expected is None:
            entry["status"] = "UNPINNED_PRESENT"
            entry["ok"] = not required
            if required:
                ok = False
                entry["reason"] = "present but no expected sha256 pinned"
        elif actual != expected:
            entry["status"] = "HASH_MISMATCH"
            entry["ok"] = False
            ok = False
            entry["reason"] = "bytes differ from the audited artifact"
        elif spec.get("bytes") is not None and path.stat().st_size != spec["bytes"]:
            entry["status"] = "SIZE_MISMATCH"
            entry["ok"] = False
            ok = False
            entry["reason"] = "size differs from the audited artifact"
        else:
            entry["status"] = "OK"
            entry["ok"] = True
        results.append(entry)
    return {"ok": ok, "manifest": str(MANIFEST.relative_to(ROOT)), "results": results}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("action", choices=["verify"], nargs="?", default="verify")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    report = verify()
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        for entry in report["results"]:
            print(f"{entry['status']:<15} {entry['path']}")
        print("BUILD_DEPS_OK" if report["ok"] else "BUILD_DEPS_MISMATCH")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
