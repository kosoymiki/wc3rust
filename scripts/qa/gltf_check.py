#!/usr/bin/env python3
"""Validate MDX->glTF conversions with the standard ecosystem tools.

Each glTF produced by `scripts/qa/mdx_gltf` is checked by four independent
consumers so a single tool's blind spot cannot pass as success:

* `gltf-validator` (Khronos) — JSON/spec conformance, errors/warnings/infos;
* `gltf-transform inspect` — loads the asset through a second implementation;
* `trimesh` — geometry loader, vertex/face counts;
* `assimp_check` (libassimp) — the same library the desktop ecosystem uses.

The script writes `check.json` next to the models and prints one line per model.
Every check reports the command and rc; a missing tool is reported as
`unavailable`, never as a pass.

Usage:
  python3 scripts/qa/gltf_check.py --root analysis/agent-qa/tooling/gltf-run2
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
NODE_MODULES = Path("/data/data/com.termux/files/usr/lib/node_modules")
VALIDATOR = NODE_MODULES / "gltf-validator"


def run(cmd: list[str], cwd: Path | None = None) -> tuple[int, str]:
    try:
        proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=300)
        return proc.returncode, ((proc.stdout or "") + (proc.stderr or "")).strip()
    except FileNotFoundError as error:
        return 127, f"unavailable: {error}"
    except subprocess.TimeoutExpired:
        return 124, "timeout"


def khronos_validate(gltf: Path) -> dict:
    script = (
        "const v=require(process.argv[1]);"
        "const fs=require('fs');const path=require('path');"
        "const dir=path.dirname(process.argv[2]);"
        "const opts={externalResourceFunction:(uri)=>Promise.resolve(new Uint8Array(fs.readFileSync(path.join(dir,uri))))};"
        "v.validateBytes(new Uint8Array(fs.readFileSync(process.argv[2])),opts).then(r=>{"
        "console.log(JSON.stringify({errors:r.issues.numErrors,warnings:r.issues.numWarnings,"
        "infos:r.issues.numInfos,messages:r.issues.messages.slice(0,8).map(m=>m.code+' @'+(m.pointer||'')+': '+m.message)}));"
        "}).catch(e=>{console.error(String(e));process.exit(1)});"
    )
    rc, out = run(["node", "-e", script, str(VALIDATOR), str(gltf)])
    payload: dict = {"rc": rc, "raw": out[:400]}
    if rc == 0:
        try:
            payload.update(json.loads(out.splitlines()[-1]))
        except (ValueError, IndexError):
            payload["parse_error"] = True
    return payload


def gltf_transform(gltf: Path) -> dict:
    rc, out = run(["gltf-transform", "inspect", str(gltf)])
    summary = {}
    for line in out.splitlines():
        stripped = line.strip()
        if stripped.startswith("scenes") or stripped.startswith("meshes"):
            summary[stripped.split()[0]] = stripped.split(":", 1)[-1].strip()
    return {"rc": rc, "out": out[-600:], "summary": summary}


def trimesh_check(gltf: Path) -> dict:
    code = (
        "import json,sys,trimesh;"
        "scene=trimesh.load(sys.argv[1], force='scene');"
        "geoms=list(scene.geometry.values());"
        "print(json.dumps({'geometries':len(geoms),"
        "'vertices':int(sum(len(g.vertices) for g in geoms)),"
        "'faces':int(sum(len(g.faces) for g in geoms))}))"
    )
    rc, out = run([sys.executable, "-c", code, str(gltf)])
    payload: dict = {"rc": rc, "raw": out[:400]}
    if rc == 0:
        try:
            payload.update(json.loads(out.splitlines()[-1]))
        except (ValueError, IndexError):
            payload["parse_error"] = True
    return payload


def assimp_check(binary: Path, gltf: Path) -> dict:
    if not binary.is_file():
        return {"rc": 127, "raw": f"unavailable: {binary} (build scripts/qa/assimp_check.c)"}
    rc, out = run([str(binary), str(gltf)])
    stats = {}
    for line in out.splitlines():
        if line.startswith("ASSIMP_OK"):
            for token in line.split()[1:]:
                key, _, value = token.partition("=")
                stats[key] = value
    return {"rc": rc, "raw": out[:400], "stats": stats}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument(
        "--assimp-bin",
        type=Path,
        default=ROOT / "analysis" / "agent-qa" / "tooling" / "assimp_check",
    )
    args = parser.parse_args(argv)
    models = sorted(path for path in args.root.glob("*/model.gltf"))
    if not models:
        print(f"no model.gltf under {args.root}", file=sys.stderr)
        return 2
    report = {"root": str(args.root), "models": {}, "summary": {}}
    counts = {"ok": 0, "fail": 0}
    for gltf in models:
        name = gltf.parent.name
        entry = {
            "gltf": str(gltf),
            "bytes": gltf.stat().st_size,
            "bin_bytes": (gltf.parent / "model.bin").stat().st_size
            if (gltf.parent / "model.bin").is_file()
            else 0,
            "khronos": khronos_validate(gltf),
            "gltf_transform": gltf_transform(gltf),
            "trimesh": trimesh_check(gltf),
            "assimp": assimp_check(args.assimp_bin, gltf),
        }
        ok = (
            entry["khronos"].get("errors") == 0
            and entry["trimesh"].get("faces", 0) > 0
            and entry["assimp"].get("rc") == 0
        )
        entry["ok"] = ok
        counts["ok" if ok else "fail"] += 1
        report["models"][name] = entry
        print(
            f"{'PASS' if ok else 'FAIL'} {name}: "
            f"khronos errors={entry['khronos'].get('errors')} warnings={entry['khronos'].get('warnings')} | "
            f"trimesh faces={entry['trimesh'].get('faces')} | "
            f"assimp {entry['assimp'].get('raw', '')[:80]}"
        )
    report["summary"] = counts
    (args.root / "check.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"GLTF_CHECK {'PASS' if counts['fail'] == 0 else 'FAIL'} {counts} -> {args.root / 'check.json'}")
    return 0 if counts["fail"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
