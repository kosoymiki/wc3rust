#!/usr/bin/env python3
"""Emit the Android release security summary required by AUD-014/AUD-029.

Every value is read from the built APK (badging + compiled manifest + zip
central directory + signing block), never from the source tree, so a stale or
wrongly merged manifest cannot pass unnoticed.

Usage:
    scripts/release_security_summary.py --apk build/.../app.apk \
        --build-type release --dist dist

Exit status:
    0  summary written; the build type's hard gates passed
    1  summary written; a hard gate failed (e.g. a release APK is debuggable)
    2  the APK could not be inspected
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field

SCHEMA_VERSION = 1


def _run(argv: list[str], binary: bool = False) -> tuple[int, bytes]:
    try:
        proc = subprocess.run(argv, capture_output=True, text=not binary)
    except FileNotFoundError as error:
        raise SystemExit(f"missing tool: {argv[0]} ({error})")
    return proc.returncode, proc.stdout


def _text(argv: list[str]) -> tuple[int, str]:
    rc, out = _run(argv)
    return rc, out  # type: ignore[return-value]


@dataclass
class ManifestNode:
    name: str
    attrs: dict[str, object] = field(default_factory=dict)
    children: list["ManifestNode"] = field(default_factory=list)

    def child(self, name: str) -> "ManifestNode | None":
        return next((node for node in self.children if node.name == name), None)

    def children_named(self, name: str) -> list["ManifestNode"]:
        return [node for node in self.children if node.name == name]


def _attr_value(raw: str) -> object:
    raw = raw.strip()
    match = re.match(r"^\(type (0x[0-9a-fA-F]+)\)(0x[0-9a-fA-F]+|[-0-9]+)$", raw)
    if match:
        value = int(match.group(2), 0)
        if match.group(1).lower() in ("0x12", "0x1f"):  # boolean
            return value != 0
        return value
    if raw.startswith('"'):
        end = raw.find('"', 1)
        return raw[1:end] if end > 0 else raw[1:]
    if raw.startswith("@"):
        return raw
    if raw in ("true", "false"):
        return raw == "true"
    try:
        return int(raw, 0)
    except ValueError:
        return raw


def parse_xmltree(text: str) -> ManifestNode:
    """Parse `aapt2 dump xmltree` into a tree using the element indentation."""
    root = ManifestNode("#document")
    stack: list[tuple[int, ManifestNode]] = [(-1, root)]
    for raw in text.splitlines():
        if not raw.strip() or raw.lstrip().startswith("N: "):
            continue
        indent = len(raw) - len(raw.lstrip(" "))
        line = raw.strip()
        if line.startswith("E: "):
            while stack and stack[-1][0] >= indent:
                stack.pop()
            node = ManifestNode(line[3:].split(" ")[0])
            stack[-1][1].children.append(node)
            stack.append((indent, node))
        elif line.startswith("A: "):
            while stack and stack[-1][0] >= indent:
                stack.pop()
            body = line[3:]
            if "=" not in body:
                continue
            key, value = body.split("=", 1)
            key = key.split("(")[0].rsplit(":", 1)[-1]
            stack[-1][1].attrs[key] = _attr_value(value)
    return root


def parse_badging(text: str) -> dict[str, object]:
    info: dict[str, object] = {
        "package": None,
        "version_code": None,
        "version_name": None,
        "min_sdk": None,
        "target_sdk": None,
        "debuggable": False,
        "native_code": [],
        "launchable_activity": None,
        "permissions": [],
    }
    for line in text.splitlines():
        if line.startswith("package: name="):
            match = re.search(r"name='([^']+)'", line)
            info["package"] = match.group(1) if match else None
            match = re.search(r"versionCode='([^']+)'", line)
            info["version_code"] = match.group(1) if match else None
            match = re.search(r"versionName='([^']+)'", line)
            info["version_name"] = match.group(1) if match else None
        elif line.startswith("sdkVersion:"):
            match = re.search(r"'([^']+)'", line)
            info["min_sdk"] = match.group(1) if match else None
        elif line.startswith("targetSdkVersion:"):
            match = re.search(r"'([^']+)'", line)
            info["target_sdk"] = match.group(1) if match else None
        elif line.startswith("application-debuggable"):
            info["debuggable"] = True
        elif line.startswith("native-code:"):
            info["native_code"] = re.findall(r"'([^']+)'", line)
        elif line.startswith("launchable-activity:"):
            match = re.search(r"name='([^']+)'", line)
            info["launchable_activity"] = match.group(1) if match else None
        elif line.startswith("uses-permission"):
            match = re.search(r"name='([^']+)'", line)
            if match:
                info["permissions"].append(match.group(1))
        elif line.startswith("permission:"):
            match = re.search(r"name='([^']+)'", line)
            if match:
                info["permissions"].append(match.group(1))
    return info


def parse_signing(text: str) -> dict[str, object]:
    info: dict[str, object] = {
        "verified": "Verifies" in text,
        "schemes": {},
        "signer_dn": None,
        "cert_sha256": None,
        "key_algorithm": None,
        "key_size": None,
        "debug_key": False,
    }
    for line in text.splitlines():
        match = re.match(r"Verified using (v[\d.]+) scheme.*: (true|false)", line)
        if match:
            info["schemes"][match.group(1)] = match.group(2) == "true"
        match = re.search(r"(?:Signer #\d+|V[\d.]+ Signer): certificate DN: (.+)$", line)
        if match:
            info["signer_dn"] = match.group(1).strip()
        match = re.search(r"(?:Signer #\d+|V[\d.]+ Signer): certificate SHA-256 digest: ([0-9a-f]+)", line)
        if match:
            info["cert_sha256"] = match.group(1)
        match = re.search(r"(?:Signer #\d+|V[\d.]+ Signer): key algorithm: (.+)$", line)
        if match:
            info["key_algorithm"] = match.group(1).strip()
        match = re.search(r"(?:Signer #\d+|V[\d.]+ Signer): key size \(bits\): (\d+)", line)
        if match:
            info["key_size"] = int(match.group(1))
    dn = info["signer_dn"]
    info["debug_key"] = bool(dn and "Android Debug" in str(dn))
    return info


def parse_native_libs(verbose: str) -> list[dict[str, object]]:
    libs: list[dict[str, object]] = []
    pattern = re.compile(
        r"^\s*(\d+)\s+\S+\s+(\d+)\s+\d+%\s+\S+\s+\S+\s+\S+\s+lib/([^/]+)/(\S+)$"
    )
    for line in verbose.splitlines():
        match = pattern.match(line)
        if not match:
            continue
        libs.append(
            {
                "abi": match.group(3),
                "name": match.group(4),
                "uncompressed_bytes": int(match.group(1)),
                "compressed_bytes": int(match.group(2)),
            }
        )
    return libs


def lib_symbols(apk: str, entry: str) -> dict[str, object]:
    rc, out = _run(["unzip", "-p", apk, entry], binary=True)
    if rc != 0 or not out:
        return {"readable": False}
    with tempfile.NamedTemporaryFile(suffix=".so", delete=False) as handle:
        handle.write(out)
        path = handle.name
    try:
        _, sections = _text(["readelf", "-S", "-W", path])
        _, dynamic = _text(["readelf", "--dyn-syms", "-W", path])
        has_symtab = ".symtab" in sections
        has_debug = ".debug_info" in sections
        dyn_funcs = len(re.findall(r"FUNC\s+\S+\s+\S+\s+\d+\s+\S+\s+\S+\s+\S+\s+\S+", dynamic))
        return {
            "readable": True,
            "bytes": len(out),
            "has_symtab": has_symtab,
            "has_debug_info": has_debug,
            "dynamic_symbols": dyn_funcs,
        }
    finally:
        os.unlink(path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", required=True)
    parser.add_argument("--build-type", choices=("debug", "release"), required=True)
    parser.add_argument("--dist", default="dist")
    args = parser.parse_args()

    apk = os.path.abspath(args.apk)
    if not os.path.isfile(apk):
        print(f"security summary: missing APK {apk}", file=sys.stderr)
        return 2

    rc, badging = _text(["aapt", "dump", "badging", apk])
    if rc != 0:
        print("security summary: aapt dump badging failed", file=sys.stderr)
        return 2
    rc, tree_text = _text(["aapt2", "dump", "xmltree", "--file", "AndroidManifest.xml", apk])
    if rc != 0:
        print("security summary: aapt2 dump xmltree failed", file=sys.stderr)
        return 2
    manifest = parse_xmltree(tree_text)
    app = manifest.child("manifest").child("application") if manifest.child("manifest") else None
    if app is None:
        print("security summary: compiled manifest has no <application>", file=sys.stderr)
        return 2

    info = parse_badging(badging)
    _, signing_text = _text(["apksigner", "verify", "--verbose", "--print-certs", apk])
    signing = parse_signing(signing_text)
    _, zip_verbose = _text(["unzip", "-v", apk])
    native_libs = parse_native_libs(zip_verbose)
    abis = sorted({str(lib["abi"]) for lib in native_libs})

    manifest_debuggable = bool(app.attrs.get("debuggable", False))
    cleartext_attr = app.attrs.get("usesCleartextTraffic")
    target_sdk = int(info["target_sdk"]) if info["target_sdk"] else 0
    if cleartext_attr is None:
        cleartext = bool(target_sdk and target_sdk < 28)
        cleartext_source = f"default for targetSdk={target_sdk}"
    else:
        cleartext = bool(cleartext_attr)
        cleartext_source = "manifest"

    exported: list[dict[str, object]] = []
    for name in ("activity", "activity-alias", "service", "receiver", "provider"):
        for node in app.children_named(name):
            value = node.attrs.get("exported")
            has_filter = node.child("intent-filter") is not None
            if value is None:
                value = has_filter  # pre-31 default
            exported.append(
                {
                    "type": name,
                    "name": node.attrs.get("name"),
                    "exported": bool(value),
                    "launcher": node.attrs.get("name") == info["launchable_activity"],
                }
            )

    symbol_report: dict[str, object] = {}
    for lib in native_libs:
        entry = f"lib/{lib['abi']}/{lib['name']}"
        symbol_report[entry] = lib_symbols(apk, entry)

    debug_abis = [abi for abi in abis if abi not in ("arm64-v8a",)]
    hard_failures: list[str] = []
    warnings: list[str] = []
    if args.build_type == "release":
        if manifest_debuggable or info["debuggable"]:
            hard_failures.append("release APK is debuggable")
        if cleartext:
            hard_failures.append("release APK allows cleartext traffic")
        if debug_abis:
            hard_failures.append(f"release APK packs unexpected ABIs: {debug_abis}")
        if signing["debug_key"]:
            warnings.append("release APK is signed with the Android debug key")
        if not signing["verified"]:
            hard_failures.append("apksigner verify failed")
    else:
        if not (manifest_debuggable or info["debuggable"]):
            hard_failures.append("debug APK is not debuggable (build-type overlay lost)")
    for component in exported:
        if component["exported"] and not component["launcher"]:
            warnings.append(f"exported non-launcher component: {component['type']} {component['name']}")
    if info["permissions"]:
        warnings.append(f"requested permissions: {', '.join(map(str, info['permissions']))}")
    unstripped = [name for name, value in symbol_report.items() if value.get("has_symtab")]
    if unstripped:
        warnings.append(f"native libraries with a full symbol table: {', '.join(unstripped)}")
    if info["target_sdk"] != "35":
        warnings.append(f"targetSdk is {info['target_sdk']}, expected 35")

    summary = {
        "schema_version": SCHEMA_VERSION,
        "generated_from": os.path.basename(apk),
        "apk_sha256": _sha256(apk),
        "build_type": args.build_type,
        "package": info["package"],
        "version_name": info["version_name"],
        "version_code": info["version_code"],
        "min_sdk": info["min_sdk"],
        "target_sdk": info["target_sdk"],
        "debuggable": {"manifest": manifest_debuggable, "badging": info["debuggable"]},
        "signing": signing,
        "cleartext_traffic": {"allowed": cleartext, "source": cleartext_source},
        "exported_components": exported,
        "permissions": info["permissions"],
        "native_libs": native_libs,
        "abi_filters": abis,
        "symbols": symbol_report,
        "gate": {
            "passed": not hard_failures,
            "failures": hard_failures,
            "warnings": warnings,
        },
    }

    dist = os.path.abspath(args.dist)
    os.makedirs(dist, exist_ok=True)
    json_path = os.path.join(dist, f"security-summary-{args.build_type}.json")
    text_path = os.path.join(dist, f"security-summary-{args.build_type}.txt")
    with open(json_path, "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, sort_keys=True)
        handle.write("\n")
    report = render(summary)
    with open(text_path, "w", encoding="utf-8") as handle:
        handle.write(report + "\n")

    print(report)
    print(f"security summary: {json_path}")
    print(f"security summary: {text_path}")
    if hard_failures:
        for failure in hard_failures:
            print(f"SECURITY GATE FAILED: {failure}", file=sys.stderr)
        return 1
    return 0


def _sha256(path: str) -> str:
    import hashlib

    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def render(summary: dict[str, object]) -> str:
    signing = summary["signing"]
    lines = [
        f"== Android release security summary ({summary['build_type']}) ==",
        f"apk            : {summary['generated_from']} sha256={summary['apk_sha256']}",
        f"package        : {summary['package']} {summary['version_name']} ({summary['version_code']})",
        f"sdk            : min={summary['min_sdk']} target={summary['target_sdk']}",
        f"debuggable     : manifest={summary['debuggable']['manifest']} badging={summary['debuggable']['badging']}",
        f"signing        : verified={signing['verified']} schemes={signing['schemes']}",
        f"                 dn={signing['signer_dn']} debug_key={signing['debug_key']}",
        f"                 cert_sha256={signing['cert_sha256']} alg={signing['key_algorithm']}/{signing['key_size']}",
        f"cleartext      : allowed={summary['cleartext_traffic']['allowed']} ({summary['cleartext_traffic']['source']})",
        f"permissions    : {summary['permissions'] or 'none requested'}",
        f"abi filters    : {summary['abi_filters']}",
        "exported       : "
        + (
            ", ".join(
                f"{c['type']}:{c['name']} (launcher)" if c["launcher"] else f"{c['type']}:{c['name']}"
                for c in summary["exported_components"]
                if c["exported"]
            )
            or "none"
        ),
        "native libs    :",
    ]
    for lib in summary["native_libs"]:
        symbols = summary["symbols"].get(f"lib/{lib['abi']}/{lib['name']}", {})
        lines.append(
            f"  {lib['abi']}/{lib['name']}: {lib['uncompressed_bytes']} B"
            f" symtab={symbols.get('has_symtab', 'n/a')}"
            f" debug={symbols.get('has_debug_info', 'n/a')}"
        )
    gate = summary["gate"]
    lines.append(f"gate           : {'PASS' if gate['passed'] else 'FAIL'}")
    for failure in gate["failures"]:
        lines.append(f"  failure: {failure}")
    for warning in gate["warnings"]:
        lines.append(f"  warning: {warning}")
    return "\n".join(lines)


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.exit(main())
