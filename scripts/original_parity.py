#!/usr/bin/env python3
"""Cross-check every wc3rust layer against the ORIGINAL game data.

The `analysis/port-parity/*` units prove that our output did not change. This
script proves something different: that what we claim matches the shipped game
itself (the StormLib-extracted tree `analysis/war3_full3`, its listfile, SLK/MDX/
BLP files, campaign maps, common.j and the operator's PE samples).

Where an independent implementation is practical (listfile walk, SLK cells, MDX
chunks, BLP header, PE headers, MPQ container listing, file counts) the check
parses the original with a small standalone Python parser instead of asking our
own Rust code to grade itself. Where no oracle exists on Termux (GPU render,
layered TTS install) the layer is reported NOT_APPLICABLE with the exact reason
— never silently skipped.

Rust tools are built ONCE per crate (`scripts/h2-cargo build`) and the resulting
binaries are executed directly: under the shared device queue many `cargo run`
calls would serialise on the build lock and time out without proving anything.

Usage: python3 scripts/original_parity.py [--json]
Exit:  0 all applicable checks PASS, 1 any check FAIL.
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import re
import shutil
import sqlite3
import struct
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FULL = ROOT / "analysis" / "war3_full3"
DATA = ROOT / "analysis" / "war3_data"
LISTFILE = ROOT / "analysis" / "war3_mpq_listing.txt"
TARGET = ROOT / "target" / "debug"
CARGO = ROOT / "scripts" / "h2-cargo"

results: list[dict] = []


def add(layer: str, name: str, verdict: str, detail: str, **extra) -> None:
    entry = {"layer": layer, "check": name, "verdict": verdict, "detail": detail}
    entry.update(extra)
    results.append(entry)


def run(args: list[str], timeout: int = 1800, env: dict | None = None) -> tuple[int, str, str]:
    proc = subprocess.run(
        args, cwd=str(ROOT), stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        timeout=timeout, env=env,
    )
    return (
        proc.returncode,
        proc.stdout.decode("utf-8", "replace"),
        proc.stderr.decode("utf-8", "replace"),
    )


# ----------------------------------------------------------------- built tools
# (package, kind, tool names). One `h2-cargo build` per package keeps the shared
# build queue to one acquisition per crate instead of one per check.
TOOL_SPECS: list[tuple[str, str, tuple[str, ...]]] = [
    ("wc3-formats", "example",
     ("slk_info", "slk_dump", "mdx_probe", "blp_info", "blp_hash", "map_corpus_audit", "map_files")),
    ("wc3-anim", "example", ("geoset_alpha",)),
    ("wc3-game", "example", ("campaign_report",)),
    ("wc3-sim", "example", ("native_status", "native_sig_dump")),
    ("wc3-audio", "bin", ("audio_event_table",)),
    ("wc3-data", "bin", ("dump_gamedata",)),
    ("wc3-data", "example", ("unit_sound_sets",)),
    ("wc3-guest", "example", ("pe_inspect",)),
    ("wc3-mpq", "example", ("compare_oracle", "codec_census")),
]

_tools: dict[str, Path] = {}
_build: dict[str, tuple[int, str]] = {}


def tool_path(pkg: str, kind: str, name: str) -> Path:
    return TARGET / ("examples" if kind == "example" else "") / name


def prepare_tools() -> None:
    """Build every tool in ONE cargo invocation; fall back to per-crate builds.

    The shared FIFO (`scripts/h2-cargo`) admits one cargo at a time, so eight
    separate builds cost eight queue waits (observed: more than an hour on a busy
    device with 15 agents). Cargo accepts repeated `-p` plus target-name filters
    in one invocation, which keeps the whole tool set to a single slot. If that
    invocation fails (a crate is broken, or a filter is rejected), the per-crate
    path still localises the failure so only the broken crate's checks fail.
    """
    combined = [str(CARGO), "build", "--offline"]
    for pkg, kind, names in TOOL_SPECS:
        combined += ["-p", pkg]
        for name in names:
            combined += [f"--{kind}", name]
    rc, out, err = run(combined, timeout=7200)
    if rc == 0:
        for pkg, kind, names in TOOL_SPECS:
            _build[pkg] = (0, "")
            for name in names:
                _tools[f"{pkg}:{name}"] = tool_path(pkg, kind, name)
        return
    _build.clear()
    _tools.clear()
    for pkg, kind, names in TOOL_SPECS:
        args = [str(CARGO), "build", "--offline", "-p", pkg]
        for name in names:
            args += [f"--{kind}", name]
        rc, out, err = run(args, timeout=5400)
        _build[pkg] = (rc, (err or out).strip().splitlines()[-1][:300] if (err or out).strip() else "")
        for name in names:
            _tools[f"{pkg}:{name}"] = tool_path(pkg, kind, name)


def run_tool(key: str, *args: str, timeout: int = 1800, env: dict | None = None) -> tuple[int, str, str]:
    path = _tools.get(key)
    pkg = key.split(":", 1)[0]
    rc_build, why = _build.get(pkg, (127, "not built"))
    if rc_build != 0:
        # A stale binary from an earlier build would hide a broken crate: a
        # package that does not build has no verified output.
        return 127, "", f"{key}: {pkg} build failed (rc={rc_build} {why})"
    if path is None or not path.is_file():
        return 127, "", f"{key}: binary missing (build rc={rc_build} {why})"
    return run([str(path), *args], timeout=timeout, env=env)


def tool_ok(key: str) -> bool:
    path = _tools.get(key)
    return bool(path and path.is_file())


# ----------------------------------------------------------------- independent SLK reader
def unquote(text: str) -> str:
    text = text.strip()
    if len(text) >= 2 and text[0] == '"' and text[-1] == '"':
        return text[1:-1].replace('""', '"')
    return text


def slk_cells(path: Path) -> dict[tuple[int, int], str]:
    """Stateful SYLK cells: `C;X<x>;Y<y>;K<value>` (x/y persist when omitted)."""
    text = path.read_bytes().decode("utf-8", "replace")
    cells: dict[tuple[int, int], str] = {}
    cx = cy = 0
    for line in text.splitlines():
        line = line.rstrip("\r")
        if len(line) < 2:
            continue
        parts = line.split(";")
        if parts[0] not in ("C", "F"):
            continue
        x, y, value, has = cx, cy, "", False
        for field in parts[1:]:
            if field.startswith("X"):
                try:
                    x = int(field[1:])
                except ValueError:
                    x = 0
            elif field.startswith("Y"):
                try:
                    y = int(field[1:])
                except ValueError:
                    y = 0
            elif field.startswith("K"):
                value = unquote(field[1:])
                has = True
        if has and x > 0 and y > 0:
            cells[(x - 1, y - 1)] = value
        cx, cy = x, y
    return cells


def slk_table(path: Path) -> list[dict[str, str]]:
    """Rows keyed by the header row (Y1); the first column is the row id."""
    cells = slk_cells(path)
    if not cells:
        return []
    max_x = max(x for x, _ in cells) + 1
    max_y = max(y for _, y in cells) + 1
    columns = [cells.get((x, 0), "") for x in range(max_x)]
    rows: list[dict[str, str]] = []
    for y in range(1, max_y):
        row = {columns[x]: cells.get((x, y), "") for x in range(max_x) if columns[x]}
        if row.get(columns[0]):
            rows.append(row)
    return rows


def slk_rows(path: Path) -> int:
    """Number of data rows (`Y2..`); `Y1` is the header."""
    text = path.read_bytes().decode("latin-1", "replace")
    if not text.startswith("ID;"):
        raise ValueError("not an SLK")
    ys = {int(m.group(1)) for m in re.finditer(r";Y(\d+);", text)}
    return max(0, len(ys) - 1)


def num_or_none(text: str | None) -> float | None:
    text = (text or "").strip()
    if text in ("", "-"):
        return None
    try:
        return float(text)
    except ValueError:
        return None


def norm(path: str) -> str:
    return path.replace("\\", "/").lstrip("/").lower()


_tree_paths: set[str] | None = None


def tree_paths() -> set[str]:
    global _tree_paths
    if _tree_paths is None:
        _tree_paths = {
            norm(str(p.relative_to(FULL)))
            for p in FULL.rglob("*")
            if p.is_file()
        }
    return _tree_paths


# ----------------------------------------------------------------- listfile vs tree
def check_inventory() -> None:
    if not LISTFILE.is_file() or not FULL.is_dir():
        add("mpq/inventory", "listfile-vs-tree", "NOT_APPLICABLE", "listfile or extracted tree missing")
        return
    listed = set()
    for line in LISTFILE.read_text(encoding="utf-8", errors="replace").splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0] == "FILE":
            name = parts[1]
            if name.startswith("("):  # MPQ metadata pseudo-entries
                continue
            listed.add(norm(name))
    present = tree_paths()
    missing = sorted(listed - present)
    extra = sorted(present - listed)
    verdict = "PASS" if not missing and not extra else "FAIL"
    add("mpq/inventory", "listfile-vs-tree", verdict,
        f"listed={len(listed)} present={len(present)} missing={len(missing)} extra={len(extra)}",
        missing_sample=missing[:5], extra_sample=extra[:5])


# ----------------------------------------------------------------- SLK rows
def check_slk() -> None:
    d = DATA / "TerrainArt"
    if not d.is_dir():
        add("formats/slk", "terrain-tables", "NOT_APPLICABLE", "analysis/war3_data/TerrainArt missing")
        return
    rc, out, _ = run_tool("wc3-formats:slk_info", str(d))
    ours = {}
    for line in out.splitlines():
        m = re.match(r"(\S+\.slk) rows=(\d+)", line)
        if m:
            ours[m.group(1)] = int(m.group(2))
    mismatches = []
    checked = 0
    for f in sorted(d.glob("*.slk")):
        try:
            expected = slk_rows(f)
        except Exception:
            continue
        checked += 1
        got = ours.get(f.name)
        if got != expected:
            mismatches.append({"file": f.name, "ours": got, "original": expected})
    verdict = "PASS" if rc == 0 and not mismatches and checked else "FAIL"
    add("formats/slk", "terrain-tables", verdict,
        f"tables={checked} mismatches={len(mismatches)} rc={rc}", mismatches=mismatches[:5])


# ----------------------------------------------------------------- MDX geosets
def mdx_chunk_sizes(path: Path) -> dict[bytes, int]:
    data = path.read_bytes()
    pos = 4
    sizes: dict[bytes, int] = {}
    while pos + 8 <= len(data):
        chunk = data[pos:pos + 4]
        size = struct.unpack_from("<I", data, pos + 4)[0]
        sizes[chunk] = size
        pos += 8 + size
    return sizes


def mdx_geosets(path: Path) -> int:
    """v800: one GEOS chunk holds every geoset, each block starts with VRTX."""
    data = path.read_bytes()
    pos = 4
    while pos + 8 <= len(data):
        chunk = data[pos:pos + 4]
        size = struct.unpack_from("<I", data, pos + 4)[0]
        if chunk == b"GEOS":
            body = data[pos + 8:pos + 8 + size]
            return body.count(b"VRTX")
        pos += 8 + size
    return 0


def check_mdx() -> None:
    models = [
        FULL / "Units/Creeps/Assassin/Assassin.mdx",
        FULL / "Units/Human/Footman/Footman.mdx",
        FULL / "Buildings/Orc/GreatHall/GreatHall.mdx",
    ]
    models = [m for m in models if m.is_file()]
    if not models:
        add("anim/mdx", "geoset-count", "NOT_APPLICABLE", "no reference models in the extracted tree")
        return
    rows = []
    mismatches = []
    for model in models:
        rc, out, _ = run_tool("wc3-anim:geoset_alpha", str(model))
        ours = sum(1 for line in out.splitlines() if line.startswith("geoset "))
        expected = mdx_geosets(model)
        # mdx_probe reports sequences and textures; the shipped v800 record sizes
        # (SEQS 132 B, TEXS 268 B) give the independent numbers from the chunks.
        prc, pout, _ = run_tool("wc3-formats:mdx_probe", str(model))
        seqs = textures = None
        m = re.search(r"name=.*?seqs=(\d+) geosets=\d+ materials=\d+ textures=(\d+)", pout)
        if m:
            seqs, textures = int(m.group(1)), int(m.group(2))
        chunks = mdx_chunk_sizes(model)
        independent_seqs = chunks.get(b"SEQS", 0) // 132
        independent_textures = chunks.get(b"TEXS", 0) // 268
        entry = {"model": model.name, "ours": ours, "original": expected,
                 "seqs": seqs, "seqs_original": independent_seqs,
                 "textures": textures, "textures_original": independent_textures,
                 "rc": rc, "probe_rc": prc}
        rows.append(entry)
        if (rc != 0 or prc != 0 or ours != expected
                or seqs != independent_seqs or textures != independent_textures):
            mismatches.append(entry)
    verdict = "PASS" if not mismatches else "FAIL"
    add("anim/mdx", "geoset-count", verdict,
        f"models={len(models)} mismatches={len(mismatches)}", rows=rows, mismatches=mismatches)


# ----------------------------------------------------------------- BLP header
def blp_header(path: Path) -> tuple[str, int, int]:
    with path.open("rb") as handle:
        head = handle.read(20)
    magic = head[0:4].decode("latin-1")
    if magic == "BLP1":
        width, height = struct.unpack_from("<II", head, 12)
    else:
        width, height = struct.unpack_from("<II", head, 4)
    return magic, width, height


def check_blp() -> None:
    files = sorted(FULL.rglob("*.blp"))[:5]
    if not files:
        add("render/blp", "header", "NOT_APPLICABLE", "no BLP in the extracted tree")
        return
    mismatches = []
    rows = []
    for f in files:
        magic, width, height = blp_header(f)
        rc, out, _ = run_tool("wc3-formats:blp_info", str(f))
        m = re.search(r"(\d+)x(\d+)", out)
        ours = (int(m.group(1)), int(m.group(2))) if m else None
        rows.append({"file": f.name, "original": [width, height], "ours": ours, "magic": magic, "rc": rc})
        if rc != 0 or ours != (width, height) or magic not in ("BLP1", "BLP2"):
            mismatches.append(rows[-1])
    verdict = "PASS" if not mismatches else "FAIL"
    add("render/blp", "header", verdict, f"files={len(files)} mismatches={len(mismatches)}", rows=rows)


# ----------------------------------------------------------------- campaign
def check_campaign() -> None:
    maps = sorted((DATA / "Maps" / "Campaign").glob("*.w3m")) + sorted((DATA / "Maps" / "Campaign").glob("*.w3x"))
    if not maps:
        add("campaign/maps", "chapter-count", "NOT_APPLICABLE", "campaign maps missing")
        return
    env = dict(os.environ)
    env["WC3RUST_DATA"] = "analysis/war3_data"
    env["WC3RUST_CAMPAIGN"] = "analysis/war3_data/Maps/Campaign"
    rc, out, _ = run_tool("wc3-game:campaign_report", "--mission", "Prologue01", "--frames", "20", env=env)
    m = re.search(r"chapters=(\d+)", out)
    ours = int(m.group(1)) if m else None
    expected = len(maps)
    verdict = "PASS" if ours == expected else "FAIL"
    add("campaign/maps", "chapter-count", verdict,
        f"ours={ours} original={expected} rc={rc}", stdout_head=out.splitlines()[:2])


# ----------------------------------------------------------------- jass natives
def check_natives() -> None:
    common = FULL / "Scripts" / "common.j"
    if not common.is_file():
        add("jass/natives", "declared-vs-registry", "NOT_APPLICABLE", "Scripts/common.j missing")
        return
    text = common.read_text(encoding="utf-8", errors="replace")
    pat = re.compile(r"^\s*(?:constant\s+)?native\s+([A-Za-z0-9_]+)\s+takes", re.M)
    names = {m.group(1) for m in pat.finditer(text)}
    rc, out, _ = run_tool("wc3-sim:native_status", timeout=900)
    registry = set()
    try:
        registry = {n["name"] for n in json.loads(out[out.index("{"):]).get("natives", [])}
    except Exception:
        pass
    missing = sorted(names - registry)
    extra = len(registry - names)
    # The registry is documented as a superset (donor common.j 1.32.9 + engine
    # registrations). The failure condition is a MISSING original native.
    verdict = "PASS" if rc == 0 and not missing else "FAIL"
    add("jass/natives", "declared-vs-registry", verdict,
        f"original_unique={len(names)} registry_total={len(registry)} missing={len(missing)} extra_documented={extra} rc={rc}",
        missing=missing[:10])


# ----------------------------------------------------------------- maps corpus
def check_maps() -> None:
    maps_dir = DATA / "Maps"
    files = [p for p in maps_dir.rglob("*") if p.suffix.lower() in (".w3m", ".w3x")]
    if not files:
        add("formats/maps", "corpus-count", "NOT_APPLICABLE", "no maps")
        return
    rc, out, _ = run_tool("wc3-formats:map_corpus_audit", str(maps_dir))
    m = re.search(r"maps=(\d+)", out)
    ours = int(m.group(1)) if m else None
    verdict = "PASS" if ours == len(files) and rc == 0 else "FAIL"
    add("formats/maps", "corpus-count", verdict,
        f"ours={ours} original={len(files)} rc={rc}", head=out.splitlines()[:2])


# ----------------------------------------------------------------- retail archive
# The operator's shipped archive, extracted from the retail installer image
# (Alcohol MDF, 2448-byte sector stride, data at +16; War3.mpq at LBA 110071).
# It is the ground truth BEHIND the StormLib tree: every byte we read from the
# archive is compared with the independently extracted file on disk.
REAL_MPQ_DEFAULT = Path("/data/data/com.termux/files/home/.local/share/wc3-install/War3.mpq")
STORM_ORACLE = FULL


def real_archive() -> Path | None:
    """$WC3_MPQ first, then the extracted-install default; only a real MPQ counts."""
    candidates: list[Path] = []
    env = os.environ.get("WC3_MPQ")
    if env:
        candidates.append(Path(env))
    candidates.append(REAL_MPQ_DEFAULT)
    for path in candidates:
        try:
            if path.is_file() and path.read_bytes()[:4] == b"MPQ\x1a":
                return path
        except OSError:
            continue
    return None


def check_real_archive() -> None:
    archive = real_archive()
    if archive is None:
        add("mpq/real-archive", "retail-vs-stormlib-tree", "NOT_APPLICABLE",
            "no retail archive at $WC3_MPQ or "
            f"{REAL_MPQ_DEFAULT} (extract the operator's installer image to run this audit)")
        return
    if not STORM_ORACLE.is_dir():
        add("mpq/real-archive", "retail-vs-stormlib-tree", "NOT_APPLICABLE",
            f"StormLib oracle tree {STORM_ORACLE.relative_to(ROOT)} missing")
        return
    rc, out, err = run_tool("wc3-mpq:compare_oracle", "--mpq", str(archive),
                            "--oracle", str(STORM_ORACLE), timeout=7200)
    m = re.search(r"RESULT files=(\d+) matched=(\d+) mismatched=(\d+) read_fail=(\d+) "
                  r"extra_in_archive=(\d+) bytes=(\d+)", out)
    if not m:
        add("mpq/real-archive", "retail-vs-stormlib-tree", "FAIL",
            f"no RESULT line rc={rc}", stderr=err.strip()[:300])
        return
    files, matched, mismatched, read_fail, extra, total_bytes = (int(g) for g in m.groups())
    ok = rc == 0 and files > 0 and matched == files and mismatched == 0 and read_fail == 0
    add("mpq/real-archive", "retail-vs-stormlib-tree",
        "PASS" if ok else "FAIL",
        f"files={files} matched={matched} mismatched={mismatched} read_fail={read_fail} "
        f"extra_in_archive={extra} bytes={total_bytes} rc={rc}",
        archive=str(archive), mismatches=out.count("  MISMATCH "))


def check_codec_census() -> None:
    archive = real_archive()
    if archive is None:
        add("mpq/codec-census", "decode-all-file-codecs", "NOT_APPLICABLE",
            "no retail archive at $WC3_MPQ or "
            f"{REAL_MPQ_DEFAULT} (extract the operator's installer image to run this audit)")
        return
    rc, out, err = run_tool("wc3-mpq:codec_census", str(archive), timeout=7200)
    m = re.search(r": files=(\d+) failures=(\d+) decoded_bytes=(\d+)", out)
    if not m:
        add("mpq/codec-census", "decode-all-file-codecs", "FAIL",
            f"no census line rc={rc}", stderr=err.strip()[:300])
        return
    files, failures, decoded = (int(g) for g in m.groups())
    histogram = [line.strip() for line in out.splitlines() if re.match(r"\s+\S+: \d+$", line)]
    ok = rc == 0 and files > 0 and failures == 0
    add("mpq/codec-census", "decode-all-file-codecs",
        "PASS" if ok else "FAIL",
        f"files={files} failures={failures} decoded_bytes={decoded} "
        f"codecs={len(histogram)} rc={rc}",
        archive=str(archive), histogram=histogram[:12])


# ----------------------------------------------------------------- audio: SoundInfo SLK
SOUNDINFO = FULL / "UI" / "SoundInfo"


def soundinfo_defs() -> dict[str, list[tuple[str, dict[str, str]]]]:
    """SoundName (lowercase) -> [(table, row)] from the shipped SoundInfo SLKs.

    Only tables carrying the `SoundName`/`FileNames` schema are ingested; the
    directory also holds environment/EAX tables that share neither column.
    """
    defs: dict[str, list[tuple[str, dict[str, str]]]] = {}
    for path in sorted(SOUNDINFO.glob("*.slk")):
        try:
            rows = slk_table(path)
        except Exception:
            continue
        if not rows:
            continue
        columns = set(rows[0].keys())
        if "SoundName" not in columns or "FileNames" not in columns:
            continue
        for row in rows:
            name = row.get("SoundName", "").strip().lower()
            if name:
                defs.setdefault(name, []).append((path.name, row))
    return defs


def soundinfo_candidates(entries: list[tuple[str, dict[str, str]]]) -> set[str]:
    candidates: set[str] = set()
    for _, row in entries:
        base = row.get("DirectoryBase", "").strip()
        for name in row.get("FileNames", "").split(","):
            name = name.strip()
            if name:
                candidates.add(norm(base + name))
    return candidates


def check_audio_events() -> None:
    key = "wc3-audio:audio_event_table"
    if not tool_ok(key):
        add("audio/soundinfo", "event-files-vs-SoundInfo", "FAIL", "audio_event_table binary missing")
        return
    defs = soundinfo_defs()
    if not defs:
        add("audio/soundinfo", "event-files-vs-SoundInfo", "NOT_APPLICABLE",
            "no SoundInfo SLK tables with the SoundName/FileNames schema")
        return
    rc, out, err = run_tool(key)
    tree = tree_paths()
    rows = 0
    gaps = []
    mismatches: list[dict] = []
    files_checked = 0
    for line in out.splitlines():
        if not line.startswith("EVENT_ROW"):
            continue
        kv = dict(token.split("=", 1) for token in line.split()[1:] if "=" in token)
        event = kv.get("event", "?")
        if kv.get("gap") == "1":
            gaps.append(event)
            continue
        if kv.get("missing") == "1":
            mismatches.append({"event": event, "problem": "event did not start", "row": line[:200]})
            continue
        rows += 1
        def_name = kv.get("def", "-")
        file_name = kv.get("file", "-")
        if file_name != "-":
            files_checked += 1
            if norm(file_name) not in tree:
                mismatches.append({"event": event, "problem": "resolved file absent from tree",
                                   "file": file_name})
        if def_name.startswith("direct:"):
            if norm(def_name.split(":", 1)[1]) != norm(file_name):
                mismatches.append({"event": event, "problem": "direct path differs",
                                   "def": def_name, "file": file_name})
            continue
        if def_name == "-":
            continue
        entries = defs.get(def_name.lower())
        if not entries:
            mismatches.append({"event": event, "problem": "def not in SoundInfo SLK", "def": def_name})
            continue
        if norm(file_name) not in soundinfo_candidates(entries):
            mismatches.append({"event": event, "problem": "file not in def row", "def": def_name,
                               "file": file_name,
                               "expected": sorted(soundinfo_candidates(entries))[:4]})
    summary = next((line for line in out.splitlines() if line.startswith("EVENT_TABLE ")), "")
    if rc != 0:
        mismatches.append({"problem": "audio_event_table rc != 0", "rc": rc, "stderr": err.strip()[:300]})
    if not summary:
        mismatches.append({"problem": "EVENT_TABLE summary missing"})
    verdict = "PASS" if not mismatches else "FAIL"
    add("audio/soundinfo", "event-files-vs-SoundInfo", verdict,
        f"tables={len({t for e in defs.values() for t, _ in e})} defs={len(defs)} events={rows} gaps={len(gaps)} files={files_checked} mismatches={len(mismatches)} rc={rc}",
        gaps=sorted(gaps), mismatches=mismatches[:5])


def check_audio_voices() -> None:
    """Unit voice families: our resolved `unitSound` vs UnitUI.slk/UnitAckSounds.slk."""
    ui_path = FULL / "Units" / "unitUI.slk"
    ack_path = SOUNDINFO / "UnitAckSounds.slk"
    key = "wc3-data:unit_sound_sets"
    if not ui_path.is_file() or not ack_path.is_file() or not tool_ok(key):
        add("audio/voices", "unitVoice-vs-UnitAckSounds", "NOT_APPLICABLE",
            "unitUI.slk / UnitAckSounds.slk / unit_sound_sets binary missing")
        return
    ui = {row.get("unitUIID", ""): row.get("unitSound", "") for row in slk_table(ui_path)}
    ack = slk_table(ack_path)
    tree = tree_paths()
    rc, out, _ = run_tool(key, "analysis/war3_full3")
    mismatches: list[dict] = []
    checked = 0
    resolved_sets: set[str] = set()
    for line in out.splitlines():
        m = re.match(r"SOUND_SET def=(\S+) .*unitSound=Some\(\"([^\"]*)\"\)", line)
        if not m:
            continue
        uid, resolved = m.group(1), m.group(2)
        expected = ui.get(uid, "")
        checked += 1
        resolved_sets.add(resolved)
        if expected != resolved:
            mismatches.append({"unit": uid, "ours": resolved, "original": expected})
        rows = [row for row in ack if row.get("SoundName", "").startswith(resolved)]
        if not rows:
            mismatches.append({"unit": uid, "problem": "voice set absent from UnitAckSounds",
                               "set": resolved})
            continue
        for row in rows:
            base = row.get("DirectoryBase", "")
            for name in row.get("FileNames", "").split(","):
                name = name.strip()
                if name and norm(base + name) not in tree:
                    mismatches.append({"unit": uid, "problem": "voice file absent from tree",
                                       "file": norm(base + name)})
    # Tree-wide coverage is reported, not gated: the RoC extraction legitimately
    # lacks TFT-only voice rows (Heroranger, Chaos*) whose defs are still shipped.
    tree_files = 0
    tree_missing: list[str] = []
    for row in ack:
        base = row.get("DirectoryBase", "")
        for name in row.get("FileNames", "").split(","):
            name = name.strip()
            if not name:
                continue
            tree_files += 1
            if norm(base + name) not in tree:
                tree_missing.append(norm(base + name))
    if not checked:
        mismatches.append({"problem": "unit_sound_sets produced no SOUND_SET rows"})
    if rc != 0:
        mismatches.append({"problem": "unit_sound_sets rc != 0", "rc": rc})
    verdict = "PASS" if not mismatches else "FAIL"
    add("audio/voices", "unitVoice-vs-UnitAckSounds", verdict,
        f"units={checked} sets={len(resolved_sets)} ack_defs={len(ack)} ack_files={tree_files} "
        f"ack_files_missing_tree={len(tree_missing)} mismatches={len(mismatches)} rc={rc}",
        missing_tree_sample=sorted(set(tree_missing))[:5], mismatches=mismatches[:5])


# ----------------------------------------------------------------- sim: UnitBalance/UnitWeapons
BALANCE_FIELDS = [
    ("hp", "HP"), ("mana", "manaN"), ("level", "level"),
    ("gold", "goldcost"), ("lumber", "lumbercost"),
    ("food_used", "fused"), ("food_provided", "fmade"),
    ("speed", "spd"), ("armor", "def"),
]


def check_sim_units() -> None:
    key = "wc3-data:dump_gamedata"
    bal_path = FULL / "Units" / "UnitBalance.slk"
    data_path = FULL / "Units" / "UnitData.slk"
    if not bal_path.is_file() or not tool_ok(key):
        add("sim/units", "stats-vs-UnitBalance", "NOT_APPLICABLE",
            "UnitBalance.slk or dump_gamedata binary missing")
        return
    balance = slk_table(bal_path)
    if not balance:
        add("sim/units", "stats-vs-UnitBalance", "NOT_APPLICABLE", "UnitBalance.slk has no rows")
        return
    race = {row.get("unitID", ""): row.get("race", "") for row in slk_table(data_path)}
    tmpdir = tempfile.mkdtemp(prefix="h2-origparity-sim-")
    db_path = Path(tmpdir) / "units.sqlite"
    try:
        rc, out, err = run_tool(key, "--root", "analysis/war3_full3", "sqlite", str(db_path))
        rows: dict[str, tuple] = {}
        if db_path.is_file():
            with sqlite3.connect(str(db_path)) as db:
                for row in db.execute(
                        "select id, race, hp, mana, level, gold, lumber, food_used, "
                        "food_provided, speed, armor from units"):
                    rows[row[0]] = row
        else:
            err = err or out
        mismatches: list[dict] = []
        ids = {row.get("unitBalanceID", "") for row in balance}
        for row in balance:
            uid = row.get("unitBalanceID", "")
            got = rows.get(uid)
            if got is None:
                mismatches.append({"unit": uid, "problem": "missing from our dump"})
                continue
            if (race.get(uid, "") or "") != (got[1] or ""):
                mismatches.append({"unit": uid, "field": "race",
                                   "original": race.get(uid, ""), "ours": got[1]})
            for index, (field, column) in enumerate(BALANCE_FIELDS, start=2):
                expected = num_or_none(row.get(column))
                actual = None if got[index] is None else float(got[index])
                if expected != actual:
                    mismatches.append({"unit": uid, "field": field,
                                       "original": expected, "ours": actual})
        extra = sorted(set(rows) - ids)
        if rc != 0:
            mismatches.append({"problem": "dump_gamedata rc != 0", "rc": rc,
                               "stderr": err.strip()[:300]})
        verdict = "PASS" if not mismatches else "FAIL"
        add("sim/units", "stats-vs-UnitBalance", verdict,
            f"units={len(balance)} fields={len(BALANCE_FIELDS)} compared={len(balance) * (len(BALANCE_FIELDS) + 1)} mismatches={len(mismatches)} extra={len(extra)} rc={rc}",
            extra=extra[:5], mismatches=mismatches[:5])
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


WEAPON_SAMPLE_STRIDE = 11


def check_sim_weapons() -> None:
    key = "wc3-data:dump_gamedata"
    path = FULL / "Units" / "UnitWeapons.slk"
    if not path.is_file() or not tool_ok(key):
        add("sim/weapons", "damage-vs-UnitWeapons", "NOT_APPLICABLE",
            "UnitWeapons.slk or dump_gamedata binary missing")
        return
    weapons = {row.get("unitWeapID", ""): row for row in slk_table(path)}
    ids = sorted(weapons)
    if not ids:
        add("sim/weapons", "damage-vs-UnitWeapons", "NOT_APPLICABLE", "UnitWeapons.slk has no rows")
        return
    indexes = sorted({0, len(ids) - 1} | set(range(0, len(ids), WEAPON_SAMPLE_STRIDE)))
    sample = [ids[i] for i in indexes]

    def dump(uid: str) -> tuple[str, tuple[int, str, str]]:
        return uid, run_tool(key, "--root", "analysis/war3_full3", "unit", uid)

    pattern = re.compile(
        r'weapon Primary: Some\("([^"]*)"\)/Some\("([^"]*)"\) '
        r'dmg=Some\((-?\d+)\)\.\.Some\((-?\d+)\) avg=Some\(([-0-9.]+)\) '
        r'dps=Some\(([-0-9.]+)\) range=Some\((-?[0-9.]+)\) cool=Some\((-?[0-9.]+)\)'
    )
    observed: dict[str, tuple[int, str, str]] = {}
    with ThreadPoolExecutor(max_workers=6) as pool:
        for uid, result in pool.map(dump, sample):
            observed[uid] = result
    mismatches: list[dict] = []
    checked = 0
    for uid in sample:
        row = weapons[uid]
        expected = {
            "min": num_or_none(row.get("mindmg1")),
            "max": num_or_none(row.get("maxdmg1")),
            "range": num_or_none(row.get("rangeN1")),
            "cool": num_or_none(row.get("cool1")),
            "atk": (row.get("atkType1", "") or "").strip(),
            "type": (row.get("weapTp1", "") or "").strip(),
        }
        rc, out, err = observed[uid]
        m = pattern.search(out)
        if rc != 0 or not m:
            mismatches.append({"unit": uid, "problem": "no weapon row", "rc": rc,
                               "stderr": err.strip()[:160]})
            continue
        checked += 1
        got = {
            "min": float(m.group(3)), "max": float(m.group(4)),
            "range": float(m.group(7)), "cool": float(m.group(8)),
            # mdx/slk print order: `Some(weapTp1)/Some(atkType1)`.
            "atk": m.group(2), "type": m.group(1),
        }
        for field in ("min", "max", "range", "atk", "type"):
            if expected[field] in ("", "_") and field in ("atk", "type"):
                continue
            if expected[field] is None:
                continue
            if got[field] != expected[field]:
                mismatches.append({"unit": uid, "field": field,
                                   "original": expected[field], "ours": got[field]})
        if expected["cool"] is not None and abs(got["cool"] - expected["cool"]) > 0.005:
            mismatches.append({"unit": uid, "field": "cool",
                               "original": expected["cool"], "ours": got["cool"]})
        if expected["range"] is not None and abs(got["range"] - expected["range"]) > 0.01:
            mismatches.append({"unit": uid, "field": "range",
                               "original": expected["range"], "ours": got["range"]})
    verdict = "PASS" if not mismatches else "FAIL"
    add("sim/weapons", "damage-vs-UnitWeapons", verdict,
        f"rows={len(weapons)} sample={len(sample)} checked={checked} stride={WEAPON_SAMPLE_STRIDE} mismatches={len(mismatches)}",
        sample=sample, mismatches=mismatches[:5])


# ----------------------------------------------------------------- map containers
INNER_NAMES = [
    "war3map.j", "scripts\\war3map.j", "war3map.w3e", "war3map.w3i",
    "war3map.w3u", "war3map.w3t", "war3map.w3a", "war3map.w3d", "war3map.w3b",
    "war3map.w3h", "war3map.w3q", "war3map.wts", "war3map.wtg", "war3map.wct",
    "war3map.w3r", "war3map.w3c", "war3map.w3s", "war3map.doo",
    "war3mapUnits.doo", "war3map.mmp", "war3map.imp", "war3mapMap.blp",
]


def mpq_listing(path: Path) -> dict[str, dict]:
    """Independent MPQ view (mpyq): name -> {present, sha256, size} for INNER_NAMES."""
    import mpyq  # noqa: PLC0415 - optional oracle, reported as NOT_APPLICABLE when absent

    data = path.read_bytes()
    base = data.find(b"MPQ\x1a")
    if base < 0:
        raise ValueError("no MPQ signature in the map container")
    archive = mpyq.MPQArchive(io.BytesIO(data[base:]), listfile=False)
    out: dict[str, dict] = {}
    for name in INNER_NAMES:
        try:
            entry = archive.get_hash_table_entry(name)
        except Exception:
            entry = None
        if not entry or entry.block_table_index == 0xFFFFFFFF:
            out[name] = {"present": False}
            continue
        record: dict = {"present": True}
        try:
            payload = archive.read_file(name)
            record["sha256"] = hashlib.sha256(payload).hexdigest()
            record["size"] = len(payload)
        except Exception as error:  # encrypted/unsupported compression: presence only
            record["unreadable"] = f"{type(error).__name__}: {error}"[:120]
        out[name] = record
    return out


def check_map_contents() -> None:
    maps = sorted((DATA / "Maps").rglob("*.w3m")) + sorted((DATA / "Maps").rglob("*.w3x"))
    if not maps:
        add("formats/map-contents", "inner-files-vs-independent-mpq", "NOT_APPLICABLE", "no maps")
        return
    key = "wc3-formats:map_files"
    if not tool_ok(key):
        add("formats/map-contents", "inner-files-vs-independent-mpq", "FAIL", "map_files binary missing")
        return
    try:
        import mpyq  # noqa: F401, PLC0415 - capability probe
    except Exception as error:
        add("formats/map-contents", "inner-files-vs-independent-mpq", "NOT_APPLICABLE",
            f"mpyq not installed ({error}); install it to enable the independent MPQ oracle")
        return
    tmpdir = Path(tempfile.mkdtemp(prefix="h2-origparity-maps-"))
    mismatches: list[dict] = []
    compared = 0
    bytes_compared = 0
    try:
        for index, map_path in enumerate(maps):
            outdir = tmpdir / f"map{index:02d}"
            outdir.mkdir(parents=True, exist_ok=True)
            rc, out, err = run_tool(key, str(map_path), str(outdir))
            ours: dict[str, int] = {}
            for line in out.splitlines():
                m = re.match(r"FILE (\S+) bytes=(\d+) out=", line)
                if m:
                    ours[norm(m.group(1))] = int(m.group(2))
            extracted = {
                norm(str(p.relative_to(outdir))): p
                for p in outdir.rglob("*") if p.is_file()
            }
            try:
                theirs = mpq_listing(map_path)
            except Exception as error:
                mismatches.append({"map": map_path.name, "problem": "mpyq failed",
                                   "detail": f"{type(error).__name__}: {error}"[:160]})
                continue
            if rc != 0:
                mismatches.append({"map": map_path.name, "problem": "map_files rc != 0",
                                   "rc": rc, "stderr": err.strip()[:160]})
            for name in INNER_NAMES:
                key_name = norm(name)
                reference = theirs[name]
                if reference["present"] != (key_name in ours):
                    mismatches.append({"map": map_path.name, "file": key_name,
                                       "problem": "presence differs",
                                       "mpyq": reference["present"], "ours": key_name in ours})
                    continue
                if not reference["present"]:
                    continue
                compared += 1
                if "sha256" not in reference:
                    continue
                if key_name not in extracted:
                    mismatches.append({"map": map_path.name, "file": key_name,
                                       "problem": "extracted file missing"})
                    continue
                payload = extracted[key_name].read_bytes()
                if len(payload) != reference["size"]:
                    mismatches.append({"map": map_path.name, "file": key_name,
                                       "problem": "size differs", "mpyq": reference["size"],
                                       "ours": len(payload)})
                elif hashlib.sha256(payload).hexdigest() != reference["sha256"]:
                    mismatches.append({"map": map_path.name, "file": key_name,
                                       "problem": "sha256 differs"})
                else:
                    bytes_compared += 1
                if ours[key_name] != len(payload):
                    mismatches.append({"map": map_path.name, "file": key_name,
                                       "problem": "reported size differs",
                                       "report": ours[key_name], "extracted": len(payload)})
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)
    verdict = "PASS" if not mismatches and compared else "FAIL"
    add("formats/map-contents", "inner-files-vs-independent-mpq", verdict,
        f"maps={len(maps)} present_compared={compared} sha256_compared={bytes_compared} mismatches={len(mismatches)}",
        mismatches=mismatches[:5])


def check_map_bytes() -> None:
    """The game data maps must be the shipped bytes (war3_data vs war3_full3)."""
    original = FULL / "Maps"
    data = DATA / "Maps"
    if not original.is_dir() or not data.is_dir():
        add("formats/map-bytes", "data-tree-vs-shipped-tree", "NOT_APPLICABLE",
            "analysis/war3_full3/Maps or analysis/war3_data/Maps missing")
        return
    left = {str(p.relative_to(data)): p for p in data.rglob("*") if p.is_file()}
    right = {str(p.relative_to(original)): p for p in original.rglob("*") if p.is_file()}
    missing = sorted(set(right) - set(left))
    extra = sorted(set(left) - set(right))
    mismatches = []
    for name in sorted(set(left) & set(right)):
        a = hashlib.sha256(left[name].read_bytes()).hexdigest()
        b = hashlib.sha256(right[name].read_bytes()).hexdigest()
        if a != b:
            mismatches.append(name)
    verdict = "PASS" if not missing and not extra and not mismatches else "FAIL"
    add("formats/map-bytes", "data-tree-vs-shipped-tree", verdict,
        f"files={len(left)} original={len(right)} missing={len(missing)} extra={len(extra)} sha256_mismatches={len(mismatches)}",
        missing=missing[:5], extra=extra[:5], mismatches=mismatches[:5])


# ----------------------------------------------------------------- guest PE
def pe_files() -> list[Path]:
    found = [
        p for p in (ROOT / "analysis").rglob("*")
        if p.is_file() and p.suffix.lower() in (".exe", ".dll")
    ]
    return sorted(found, key=lambda p: str(p.relative_to(ROOT)))


def pe_parse(data: bytes) -> dict:
    def u16(offset: int) -> int:
        return struct.unpack_from("<H", data, offset)[0]

    def u32(offset: int) -> int:
        return struct.unpack_from("<I", data, offset)[0]

    if data[0:2] != b"MZ":
        raise ValueError("no MZ signature")
    e_lfanew = u32(0x3C)
    if data[e_lfanew:e_lfanew + 4] != b"PE\0\0":
        raise ValueError("no PE signature")
    coff = e_lfanew + 4
    machine = u16(coff)
    sections_count = u16(coff + 2)
    optional_size = u16(coff + 16)
    optional = coff + 20
    pe32 = u16(optional) == 0x10B
    image_base = u32(optional + 28) if pe32 else struct.unpack_from("<Q", data, optional + 24)[0]
    entry_rva = u32(optional + 16)
    size_of_image = u32(optional + 56)
    directories = optional + (96 if pe32 else 112)
    section_table = optional + optional_size
    sections = []
    for index in range(sections_count):
        base = section_table + index * 40
        sections.append({
            "name": data[base:base + 8].rstrip(b"\0").decode("latin-1"),
            "virtual_address": u32(base + 12),
            "virtual_size": u32(base + 8),
            "raw_size": u32(base + 16),
            "raw_pointer": u32(base + 20),
        })

    def rva_to_offset(rva: int) -> int | None:
        for section in sections:
            span = max(section["virtual_size"], section["raw_size"])
            if section["virtual_address"] <= rva < section["virtual_address"] + span:
                return section["raw_pointer"] + rva - section["virtual_address"]
        return None

    export_rva, export_size = u32(directories), u32(directories + 4)
    exports: list[dict] = []
    named: dict[int, str] = {}
    if export_rva and export_size:
        directory = rva_to_offset(export_rva)
        if directory is not None:
            base = u32(directory + 16)
            functions = u32(directory + 20)
            names = u32(directory + 24)
            functions_off = rva_to_offset(u32(directory + 28))
            names_off = rva_to_offset(u32(directory + 32))
            ordinals_off = rva_to_offset(u32(directory + 36))
            if names and names_off is not None and ordinals_off is not None:
                for index in range(names):
                    name_off = rva_to_offset(u32(names_off + index * 4))
                    slot = u16(ordinals_off + index * 2)
                    if name_off is None:
                        continue
                    named[slot] = data[name_off:data.index(b"\0", name_off)].decode("latin-1")
            if functions_off is not None:
                for index in range(functions):
                    rva = u32(functions_off + index * 4)
                    if rva == 0:
                        continue
                    exports.append({
                        "ordinal": base + index,
                        "name": named.get(index),
                        "rva": rva,
                        "forwarder": export_rva <= rva < export_rva + export_size,
                    })
    import_rva, import_size = u32(directories + 8), u32(directories + 12)
    imports: list[dict] = []
    if import_rva and import_size:
        cursor = rva_to_offset(import_rva)
        while cursor is not None:
            original = u32(cursor)
            name_rva = u32(cursor + 12)
            first = u32(cursor + 16)
            if not original and not name_rva and not first:
                break
            name_off = rva_to_offset(name_rva)
            if name_off is None:
                break
            module = data[name_off:data.index(b"\0", name_off)].decode("latin-1")
            thunk = rva_to_offset(original or first)
            step = 4 if pe32 else 8
            while thunk is not None:
                value = u32(thunk) if pe32 else struct.unpack_from("<Q", data, thunk)[0]
                if value == 0:
                    break
                if value >> (31 if pe32 else 63):
                    imports.append({"module": module, "name": None, "ordinal": value & 0xFFFF})
                else:
                    hint_off = rva_to_offset(value & 0x7FFFFFFF)
                    if hint_off is None:
                        imports.append({"module": module, "name": None, "ordinal": None})
                    else:
                        start = hint_off + 2
                        imports.append({"module": module,
                                        "name": data[start:data.index(b"\0", start)].decode("latin-1"),
                                        "ordinal": None})
                thunk += step
            cursor += 20
    return {
        "machine": machine, "image_base": image_base, "size_of_image": size_of_image,
        "entry_rva": entry_rva, "sections": sections, "exports": exports, "imports": imports,
    }


def check_guest_pe() -> None:
    key = "wc3-guest:pe_inspect"
    files = pe_files()
    if not files:
        add("guest/pe", "headers-vs-independent-parser", "NOT_APPLICABLE",
            "no PE sample under analysis; run with the operator's War3.exe to enable this check")
        return
    if not tool_ok(key):
        add("guest/pe", "headers-vs-independent-parser", "FAIL", "pe_inspect binary missing")
        return
    checked = 0
    mismatches: list[dict] = []
    for path in files:
        rel = str(path.relative_to(ROOT))
        rc, out, err = run_tool(key, str(path))
        if rc != 0:
            mismatches.append({"file": rel, "problem": "pe_inspect rc != 0", "rc": rc,
                               "stderr": err.strip()[:160]})
            continue
        head: dict[str, int] = {}
        sections: list[dict] = []
        exports: list[dict] = []
        named = ordinal_only = None
        modules: list[str] = []
        imports: list[dict] = []
        for line in out.splitlines():
            if line.startswith("  .") or (line.startswith("  ") and " va=" in line and " vsize=" in line):
                m = re.match(r"\s+(\S+)\s+va=0x([0-9a-f]+) vsize=0x([0-9a-f]+) raw=0x([0-9a-f]+)@0x([0-9a-f]+)", line)
                if m:
                    sections.append({
                        "name": m.group(1),
                        "virtual_address": int(m.group(2), 16),
                        "virtual_size": int(m.group(3), 16),
                        "raw_size": int(m.group(4), 16),
                        "raw_pointer": int(m.group(5), 16),
                    })
                continue
            m = re.match(r"(bytes|machine|image_base|size_of_image|entry_rva|sections|exports|imports)\s+(\S+)", line)
            if m:
                head[m.group(1)] = int(m.group(2), 0)
                continue
            m = re.match(r"\s+named=(\d+) ordinal_only=(\d+)", line)
            if m:
                named, ordinal_only = int(m.group(1)), int(m.group(2))
                continue
            m = re.match(r"\s+modules=(\d+) \[(.*)\]", line)
            if m:
                modules = [item.strip().strip('"') for item in m.group(2).split(",") if item.strip()]
                continue
            m = re.match(r"\s+#(\d+) (\S+) @ 0x([0-9a-f]+)", line)
            if m:
                exports.append({"ordinal": int(m.group(1)), "name": m.group(2),
                                "rva": int(m.group(3), 16)})
                continue
            if line.startswith("  …"):
                continue
            if line.startswith("  ") and "!" in line and not re.match(r"\s+#\d+ ", line):
                module, _, symbol = line.strip().partition("!")
                imports.append({"module": module, "symbol": symbol})
                continue
        reference = pe_parse(path.read_bytes())
        checked += 1
        if head.get("bytes") != path.stat().st_size:
            mismatches.append({"file": rel, "field": "bytes", "ours": head.get("bytes"),
                               "original": path.stat().st_size})
        for field in ("machine", "image_base", "size_of_image", "entry_rva"):
            if head.get(field) != reference[field]:
                mismatches.append({"file": rel, "field": field,
                                   "ours": head.get(field), "original": hex(reference[field])})
        if sections != reference["sections"]:
            mismatches.append({"file": rel, "field": "sections",
                               "ours": sections[:2], "original": reference["sections"][:2]})
        expected_exports = reference["exports"]
        expected_named = sum(1 for e in expected_exports if e["name"] is not None)
        expected_ordinal = len(expected_exports) - expected_named
        if head.get("exports") != len(expected_exports):
            mismatches.append({"file": rel, "field": "exports",
                               "ours": head.get("exports"), "original": len(expected_exports)})
        if named is not None and (named != expected_named or ordinal_only != expected_ordinal):
            mismatches.append({"file": rel, "field": "exports/named",
                               "ours": [named, ordinal_only],
                               "original": [expected_named, expected_ordinal]})
        for index, entry in enumerate(expected_exports[:12]):
            if index >= len(exports):
                mismatches.append({"file": rel, "field": "export-entry",
                                   "problem": "missing", "index": index})
                continue
            expected_name = entry["name"] if entry["name"] is not None else "<ordinal>"
            if (exports[index]["ordinal"] != entry["ordinal"]
                    or exports[index]["name"] != expected_name
                    or exports[index]["rva"] != entry["rva"]):
                mismatches.append({"file": rel, "field": "export-entry", "index": index,
                                   "ours": exports[index],
                                   "original": {"ordinal": entry["ordinal"],
                                                "name": expected_name, "rva": entry["rva"]}})
        if head.get("imports") != len(reference["imports"]):
            mismatches.append({"file": rel, "field": "imports",
                               "ours": head.get("imports"), "original": len(reference["imports"])})
        expected_modules = sorted({i["module"].lower() for i in reference["imports"]})
        if sorted({m.lower() for m in modules}) != expected_modules:
            mismatches.append({"file": rel, "field": "import/modules",
                               "ours": sorted({m.lower() for m in modules}),
                               "original": expected_modules})
        for index, entry in enumerate(reference["imports"][:12]):
            if index >= len(imports):
                mismatches.append({"file": rel, "field": "import-entry",
                                   "problem": "missing", "index": index})
                continue
            expected_symbol = (entry["name"] if entry["name"] is not None
                               else f"#{entry['ordinal']}" if entry["ordinal"] is not None
                               else "<nameless>")
            if (imports[index]["module"].lower() != entry["module"].lower()
                    or imports[index]["symbol"] != expected_symbol):
                mismatches.append({"file": rel, "field": "import-entry", "index": index,
                                   "ours": imports[index],
                                   "original": {"module": entry["module"], "symbol": expected_symbol}})
    if not checked and not mismatches:
        add("guest/pe", "headers-vs-independent-parser", "NOT_APPLICABLE", "no PE parsed")
        return
    verdict = "PASS" if not mismatches else "FAIL"
    add("guest/pe", "headers-vs-independent-parser", verdict,
        f"files={len(files)} checked={checked} mismatches={len(mismatches)}",
        files=[str(p.relative_to(ROOT)) for p in files], mismatches=mismatches[:5])


# ----------------------------------------------------------------- byte-level sub-checks
def run_subcheck(layer: str, name: str, script: str) -> None:
    path = ROOT / script
    if not path.is_file():
        add(layer, name, "NOT_APPLICABLE", f"{script} missing")
        return
    env = dict(os.environ)
    env["WC3RUST_BIN_DIR"] = str(TARGET)
    rc, out, _ = run([sys.executable, str(path), "--json"], timeout=1800, env=env)
    ok = rc == 0
    detail = "rc=%d" % rc
    try:
        payload = json.loads(out[out.index("{"):])
        if "failures" in payload:
            detail = f"compared={payload.get('compared', payload.get('lines', payload.get('tables')))} failures={payload['failures']} rc={rc}"
        elif "mismatches" in payload:
            value = payload["mismatches"]
            count = value if isinstance(value, int) else len(value)
            shape = ""
            if "original" in payload and "registry" in payload:
                shape = (f"original={payload['original']} registry={payload['registry']} "
                         f"missing={len(payload.get('missing') or [])} ")
            elif "lines" in payload or "compared" in payload:
                shape = f"lines={payload.get('lines', payload.get('compared'))} "
            elif "tables" in payload:
                shape = f"tables={payload.get('tables')} "
            detail = f"{shape}mismatches={count} rc={rc}"
    except Exception:
        pass
    add(layer, name, "PASS" if ok else "FAIL", detail)


def check_byte_level() -> None:
    run_subcheck("jass/signatures", "signatures-vs-common.j", "scripts/native_sig_compare.py")
    run_subcheck("render/blp", "pixels-vs-independent-decoder", "scripts/blp_pixel_compare.py")
    run_subcheck("formats/slk", "cell-values-vs-independent-parser", "scripts/slk_value_compare.py")


# ----------------------------------------------------------------- not applicable
def check_device_only() -> None:
    add("input/router", "vs-original", "NOT_APPLICABLE",
        "platform layer: the original game has no touch router; parity is device evidence (operator-bugs scenarios)")
    add("audio/layered", "vs-original", "NOT_APPLICABLE",
        "needs the layered War3/War3x install (TTS/proxy layers); device audio stats are the evidence")
    add("render/terrain", "vs-original", "NOT_APPLICABLE",
        "needs a GPU adapter; device frame comparison (GAME_TERRAIN_*, screenshots) is the evidence")


def main() -> int:
    as_json = "--json" in sys.argv[1:]
    prepare_tools()
    check_inventory()
    check_slk()
    check_mdx()
    check_blp()
    check_campaign()
    check_natives()
    check_maps()
    check_real_archive()
    check_codec_census()
    check_audio_events()
    check_audio_voices()
    check_sim_units()
    check_sim_weapons()
    check_map_contents()
    check_map_bytes()
    check_guest_pe()
    check_byte_level()
    check_device_only()

    failures = [r for r in results if r["verdict"] == "FAIL"]
    applicable = [r for r in results if r["verdict"] != "NOT_APPLICABLE"]
    build_errors = {pkg: {"rc": rc, "detail": why} for pkg, (rc, why) in sorted(_build.items()) if rc != 0}

    if as_json:
        print(json.dumps({"ok": not failures, "checks": results,
                          "applicable": len(applicable), "failures": len(failures),
                          "not_applicable": len(results) - len(applicable),
                          "build_errors": build_errors},
                         ensure_ascii=False, indent=2))
    else:
        for r in results:
            print(f"ORIG_PARITY {r['verdict']:<14} layer={r['layer']:<20} check={r['check']:<34} {r['detail']}")
        print(f"ORIG_PARITY_SUMMARY applicable={len(applicable)} failures={len(failures)} "
              f"not_applicable={len(results) - len(applicable)}")
        for pkg, info in build_errors.items():
            print(f"ORIG_PARITY_BUILD {pkg} rc={info['rc']} {info['detail']}")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
