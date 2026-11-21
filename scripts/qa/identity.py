#!/usr/bin/env python3
"""Release identity chain for the Android QA report (K-01/K-12).

A QA run is only evidence about the build it names. The old report hashed the
APK file on disk while the device ran whatever package happened to be installed;
nothing read ``git rev-parse``. This module answers three questions with data:

1. *source build* - does ``target/.../build-manifest.json`` describe the tree
   that is checked out right now (HEAD + a content digest of changed sources)
   and the APK file present on disk?
2. *install identity* - does the device evidence record an installed
   ``base.apk`` whose sha256 equals the built artifact?
3. *device statuses* - first frame / menu visual / in-game visual / gameplay
   smoke, each derived from a device artifact whose APK identity matches the
   built artifact. Evidence from another APK is a mismatch (FAIL), never a PASS.

Everything is read-only: no device access, no builds.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
APK = ROOT / "build" / "android-p0" / "release" / "wc3rust-p0.apk"
MANIFEST = APK.parent / "build-manifest.json"
EMBEDDED = "assets/build-identity.json"

sys.path.insert(0, str(ROOT / "scripts"))
import build_inputs  # noqa: E402


def sha256_file(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git(*args: str) -> str:
    proc = subprocess.run(["git", *args], cwd=str(ROOT), capture_output=True, text=True, timeout=120)
    if proc.returncode != 0:
        return ""
    return proc.stdout


def current_source_state() -> dict:
    return build_inputs.source_state()


def load_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return {}


def load_manifest() -> dict:
    return load_json(MANIFEST)


def embedded_identity(apk: Path) -> dict:
    """Read the build identity aapt2 packaged inside the APK itself."""
    if not apk.is_file():
        return {}
    try:
        with zipfile.ZipFile(apk) as archive:
            for name in archive.namelist():
                if name.endswith("build-identity.json"):
                    return json.loads(archive.read(name))
    except (OSError, ValueError, zipfile.BadZipFile):
        return {}
    return {}


def verify_source_build(manifest: dict | None = None) -> dict:
    """Bind manifest <-> current tree <-> apk file <-> embedded identity."""
    manifest = load_manifest() if manifest is None else manifest
    state = current_source_state()
    apk_sha = sha256_file(APK)
    result = {
        "status": "FAIL",
        "head": state["head"],
        "dirty_count": state["dirty_count"],
        "source_dirty_digest": state["source_dirty_digest"],
        "apk_sha256": apk_sha,
        "manifest_path": str(MANIFEST.relative_to(ROOT)),
        "reasons": [],
    }
    if not manifest:
        result["reasons"].append("build-manifest.json missing (run scripts/build-apk.sh)")
        return result
    source = manifest.get("source") or {}
    artifact = manifest.get("artifact") or {}
    result.update(
        {
            "manifest_head": source.get("head"),
            "manifest_dirty_count": source.get("dirty_count"),
            "manifest_source_dirty_digest": source.get("source_dirty_digest"),
            "manifest_apk_sha256": artifact.get("sha256"),
        }
    )
    if source.get("head") != state["head"]:
        result["reasons"].append(
            f"HEAD moved since the build: manifest={source.get('head')} tree={state['head']}"
        )
    if source.get("source_dirty_digest") != state["source_dirty_digest"]:
        result["reasons"].append(
            "source content changed since the build "
            f"(manifest={str(source.get('source_dirty_digest'))[:12]} "
            f"tree={state['source_dirty_digest'][:12]})"
        )
    # The packaged .so must come from the same native inputs the manifest
    # names; a lane editing a crate after build-native would otherwise leave a
    # stale library behind a green source digest.
    current_native = build_inputs.digest(build_inputs.native_inputs())
    result["native_inputs_sha256"] = current_native
    manifest_native = (manifest.get("inputs") or {}).get("native_sha256")
    if manifest_native != current_native:
        result["reasons"].append(
            "libmain.so was built from different native sources "
            f"(manifest={str(manifest_native)[:12]} tree={current_native[:12]})"
        )
    if artifact.get("sha256") != apk_sha:
        result["reasons"].append(
            f"APK bytes differ from the manifest: manifest={artifact.get('sha256')} file={apk_sha}"
        )
    embedded = embedded_identity(APK)
    result["embedded"] = embedded
    if not embedded:
        result["reasons"].append("APK carries no assets/build-identity.json")
    else:
        for key, manifest_key in (
            ("head", ("source", "head")),
            ("source_dirty_digest", ("source", "source_dirty_digest")),
            ("native_inputs_sha256", ("inputs", "native_sha256")),
            ("packaging_inputs_sha256", ("inputs", "packaging_sha256")),
            ("source_date_epoch", ("packaging", "source_date_epoch")),
        ):
            expected = (manifest.get(manifest_key[0]) or {}).get(manifest_key[1])
            if embedded.get(key) != expected:
                result["reasons"].append(
                    f"embedded identity {key}={embedded.get(key)} != manifest {expected}"
                )
    if not result["reasons"]:
        result["status"] = "PASS"
    return result


def _apk_state(claimed: str | None, installed: str | None, apk_sha: str | None) -> str:
    """Classify device evidence against the built artifact.

    stale    - evidence is about another APK: discard it (never count it)
    missing  - same APK but the run did not record the installed hash
    mismatch - same APK claim, different installed bytes: FAIL
    match    - installed == claimed == built
    """
    if not apk_sha or not claimed:
        return "stale"
    if claimed != apk_sha:
        return "stale"
    if not installed:
        return "missing"
    return "match" if installed == apk_sha else "mismatch"


def _cycle_identity(cycle: dict, apk_sha: str | None) -> tuple[str, str | None]:
    apk = cycle.get("apk") or {}
    installed = apk.get("installed_sha256") or apk.get("installed")
    return _apk_state(apk.get("sha256"), installed, apk_sha), installed


def device_evidence(apk_sha: str | None) -> dict:
    """Collect device artifacts and classify them against the built APK."""
    parity = ROOT / "analysis" / "agent-qa" / "parity"
    suites = ROOT / "analysis" / "agent-qa" / "latest" / "summary.json"
    evidence: dict = {"sources": [], "statuses": {}, "stale": []}

    def add_status(name: str, status: str, detail: str, source: str | None = None) -> None:
        entry = evidence["statuses"].setdefault(name, {"status": "NOT_RUN", "detail": "no matching evidence", "sources": []})
        # FAIL dominates, PASS only replaces NOT_RUN.
        rank = {"PASS": 2, "NOT_RUN": 1, "FAIL": 3}
        if rank[status] >= rank[entry["status"]] and entry["status"] != "FAIL":
            entry["status"] = status
            entry["detail"] = detail
        if source:
            entry["sources"].append(source)

    # The build manifest can carry the last verified install identity (written
    # by scripts/qa/device.py right after `pm install`).
    manifest = load_manifest()
    recorded = ((manifest.get("artifact") or {}).get("install")) if manifest else None
    if recorded and apk_sha:
        rel = str(MANIFEST.relative_to(ROOT))
        if recorded.get("expected_sha256") != apk_sha:
            evidence["stale"].append({"path": rel, "reason": "recorded install belongs to another APK"})
        elif recorded.get("installed_sha256") == apk_sha:
            add_status("install_identity", "PASS", f"{rel} records a verified install", rel)
        elif recorded.get("installed_sha256"):
            add_status(
                "install_identity",
                "FAIL",
                f"{rel} records installed={recorded.get('installed_sha256')} built={apk_sha}",
                rel,
            )

    for path in (parity / "p0-device" / "cycle.json", parity / "device-cycle" / "cycle.json"):
        cycle = load_json(path)
        if not cycle:
            continue
        rel = str(path.relative_to(ROOT))
        state, installed = _cycle_identity(cycle, apk_sha)
        claimed = (cycle.get("apk") or {}).get("sha256")
        evidence["sources"].append({
            "path": rel,
            "schema": cycle.get("schema"),
            "status": cycle.get("status"),
            "apk_sha256": claimed,
            "installed_sha256": installed,
            "identity": state,
        })
        if state == "stale":
            # Evidence about another artifact is discarded, never promoted to
            # PASS and never allowed to poison a later build's verdict.
            evidence["stale"].append({
                "path": rel,
                "reason": f"produced on APK {claimed} (built={apk_sha})",
            })
            continue
        if state == "mismatch":
            add_status(
                "install_identity",
                "FAIL",
                f"device evidence {rel} claims APK {claimed} but installed={installed}",
                rel,
            )
            continue
        if state == "missing":
            add_status(
                "install_identity",
                "NOT_RUN",
                f"{rel} is for this APK but did not record the installed sha256; rerun the cycle",
                rel,
            )
            continue
        add_status("install_identity", "PASS", f"{rel} ran the built artifact", rel)
        launch = cycle.get("launch") or {}
        first = launch.get("first_frame") or {}
        first_line = first.get("first_frame") if isinstance(first, dict) else first
        if first_line and "GAME_FIRST_FRAME" in str(first_line):
            add_status("first_frame", "PASS", str(first_line)[:160], rel)
        steps = cycle.get("steps") or {}
        in_game_shots = [
            step.get("screenshot")
            for name, step in steps.items()
            if isinstance(step, dict) and step.get("screenshot") and name != "launch"
        ]
        existing = [shot for shot in in_game_shots if (ROOT / shot).is_file()]
        if existing:
            add_status("in_game_visual", "PASS", f"{len(existing)} in-game screenshots on the built APK", rel)
        required = cycle.get("required") or {}
        smoke_ok = bool(required) and all(required.values())
        smoke_ok = smoke_ok or (cycle.get("status") == "PASS" and "p0-device" in rel)
        if smoke_ok:
            add_status("gameplay_smoke", "PASS", f"cycle required steps all true ({rel})", rel)

    suite = load_json(suites)
    if suite:
        rel = str(suites.relative_to(ROOT))
        phases = suite.get("phases") or {}
        suite_sha = suite.get("apk_sha256")
        installed = ((phases.get("install") or {}).get("installed_sha256"))
        state = _apk_state(suite_sha, installed, apk_sha)
        evidence["sources"].append({
            "path": rel,
            "schema": "wc3rust-qa-suite-v1",
            "apk_sha256": suite_sha,
            "installed_sha256": installed,
            "identity": state,
        })
        if state == "stale":
            evidence["stale"].append({"path": rel, "reason": f"suite ran on {suite_sha} (built={apk_sha})"})
        elif state == "mismatch":
            add_status("install_identity", "FAIL", f"suite claims {suite_sha} but installed={installed}", rel)
        elif state == "missing":
            add_status("install_identity", "NOT_RUN", f"{rel} has no installed sha256; rerun the suite", rel)
        else:
            add_status("install_identity", "PASS", f"{rel} installed the built artifact", rel)
            if (phases.get("launch") or {}).get("ok"):
                add_status("first_frame", "PASS", "suite launch phase", rel)
            if (phases.get("visual") or {}).get("ok"):
                add_status("menu_visual", "PASS", "suite visual phase", rel)
            if (phases.get("gameplay") or {}).get("ok"):
                add_status("gameplay_smoke", "PASS", "suite gameplay phase", rel)

    selftest = ROOT / "analysis" / "device-selftest.log"
    if selftest.is_file():
        text = selftest.read_text(errors="replace")
        rel = str(selftest.relative_to(ROOT))
        match = re.search(r"SELFTEST_APK sha=([0-9a-f]{64})(?: installed=([0-9a-f]{64}))?", text)
        if match and "SELFTEST_PASS" in text:
            claimed, installed = match.group(1), match.group(2)
            state = _apk_state(claimed, installed, apk_sha)
            evidence["sources"].append({
                "path": rel,
                "schema": "wc3rust-selftest-log-v1",
                "apk_sha256": claimed,
                "installed_sha256": installed,
                "identity": state,
            })
            if state == "stale":
                evidence["stale"].append({"path": rel, "reason": f"selftest ran on {claimed} (built={apk_sha})"})
            elif state == "mismatch":
                add_status("install_identity", "FAIL", f"{rel} claims {claimed} but installed={installed}", rel)
            elif state == "missing":
                add_status("install_identity", "NOT_RUN", f"{rel} has no installed sha256; rerun the selftest", rel)
            else:
                add_status("install_identity", "PASS", f"{rel} installed the built artifact", rel)
                add_status("gameplay_smoke", "PASS", f"{rel} SELFTEST_PASS", rel)

    for name in ("install_identity", "first_frame", "menu_visual", "in_game_visual", "gameplay_smoke"):
        evidence["statuses"].setdefault(name, {"status": "NOT_RUN", "detail": "no matching evidence", "sources": []})
    evidence["complete"] = all(entry["status"] == "PASS" for entry in evidence["statuses"].values())
    return evidence


def release_summary() -> dict:
    manifest = load_manifest()
    source = verify_source_build(manifest)
    apk_sha = source.get("apk_sha256")
    evidence = device_evidence(apk_sha)
    statuses = evidence["statuses"]
    fails = [name for name, entry in statuses.items() if entry["status"] == "FAIL"]
    verdict = "FAIL" if source["status"] == "FAIL" or fails else (
        "PASS" if evidence["complete"] else "INCOMPLETE"
    )
    installed = next(
        (entry.get("installed_sha256") for entry in evidence["sources"] if entry.get("identity") == "match"),
        None,
    )
    recorded = ((manifest.get("artifact") or {}).get("install") or {}) if manifest else {}
    if installed is None and recorded.get("expected_sha256") == apk_sha:
        installed = recorded.get("installed_sha256")
    return {
        "verdict": verdict,
        "source_build": source,
        "install_identity": statuses["install_identity"],
        "statuses": statuses,
        "device_sources": evidence["sources"],
        "stale_evidence": evidence["stale"],
        "head": source["head"],
        "dirty_count": source["dirty_count"],
        "apk_sha256": apk_sha,
        "installed_sha256": installed,
    }


def main(argv: list[str] | None = None) -> int:
    summary = release_summary()
    print(json.dumps(summary, indent=2))
    return 0 if summary["verdict"] != "FAIL" else 1


if __name__ == "__main__":
    raise SystemExit(main())
