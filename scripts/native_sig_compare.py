#!/usr/bin/env python3
"""Byte-level signature check: shipped common.j vs our native registry.

Parses every `native <Name> takes <params> returns <ret>` declaration in the
game's own Scripts/common.j, encodes it in the same scheme as registry_gen.rs
(V/B/I/R/S/C/Hname;) and diffs it against `cargo run -p wc3-sim --example
native_sig_dump`. Any difference on a name the game declares is a port bug.

Usage: python3 scripts/native_sig_compare.py [--json]
Exit:  0 no mismatches, 1 mismatches (listed).
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMON = ROOT / "analysis" / "war3_full3" / "Scripts" / "common.j"
# original_parity builds every tool once and exports the directory: executing the
# built binary keeps this check off the shared cargo queue. Standalone runs fall
# back to `cargo run`.
BIN_DIR = os.environ.get("WC3RUST_BIN_DIR")

SCALAR = {
    "nothing": "V", "boolean": "B", "integer": "I", "real": "R",
    "string": "S", "code": "C",
}


def enc_type(jass: str) -> str:
    jass = jass.strip().lower()
    if jass in SCALAR:
        return SCALAR[jass]
    # handle subtype, e.g. `unit` -> Hunit;
    return f"H{jass};"


def enc_params(text: str) -> str:
    text = text.strip()
    if not text or text.lower() == "nothing":
        return ""
    out = []
    for part in text.split(","):
        # `<type> <name>`; arrays are not used by shipped common.j natives
        pieces = part.split()
        if not pieces:
            continue
        out.append(enc_type(pieces[0]))
    return "".join(out)


def parse_common(text: str) -> dict[str, tuple[str, str]]:
    decls: dict[str, tuple[str, str]] = {}
    pattern = re.compile(
        r"^\s*(?:constant\s+)?native\s+([A-Za-z0-9_]+)\s+takes\s+(.*?)\s+returns\s+([A-Za-z0-9_]+)\s*$",
        re.M,
    )
    for m in pattern.finditer(text):
        decls[m.group(1)] = (enc_params(m.group(2)), enc_type(m.group(3)))
    return decls


def dump_registry() -> dict[str, tuple[str, str]]:
    exe = Path(BIN_DIR) / "examples" / "native_sig_dump" if BIN_DIR else None
    if exe is not None and exe.is_file():
        command = [str(exe)]
    else:
        command = ["scripts/h2-cargo", "run", "-q", "-p", "wc3-sim",
                   "--example", "native_sig_dump", "--offline"]
    proc = subprocess.run(
        command, cwd=str(ROOT), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=600,
    )
    rows: dict[str, tuple[str, str]] = {}
    for line in proc.stdout.decode("utf-8", "replace").splitlines():
        parts = line.split("|")
        if len(parts) == 3:
            rows[parts[0]] = (parts[1], parts[2])
    return rows


def main() -> int:
    as_json = "--json" in sys.argv[1:]
    text = COMMON.read_text(encoding="utf-8", errors="replace")
    original = parse_common(text)
    registry = dump_registry()

    missing = sorted(name for name in original if name not in registry)
    mismatches = []
    for name in sorted(original):
        if name not in registry:
            continue
        o_params, o_ret = original[name]
        r_params, r_ret = registry[name]
        if (o_params, o_ret) != (r_params, r_ret):
            mismatches.append({
                "name": name,
                "original": {"params": o_params, "ret": o_ret},
                "registry": {"params": r_params, "ret": r_ret},
            })

    payload = {
        "ok": not missing and not mismatches,
        "original": len(original),
        "registry": len(registry),
        "missing": missing,
        "mismatches": mismatches,
    }
    if as_json:
        print(json.dumps(payload, ensure_ascii=False, indent=2))
    else:
        for item in mismatches:
            print(f"NATIVE_SIG_MISMATCH {item['name']} original={item['original']} registry={item['registry']}")
        for name in missing:
            print(f"NATIVE_SIG_MISSING {name}")
        print(f"NATIVE_SIG_SUMMARY original={len(original)} registry={len(registry)} "
              f"missing={len(missing)} mismatches={len(mismatches)}")
    return 0 if payload["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
