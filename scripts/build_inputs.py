#!/usr/bin/env python3
"""Deterministic inventory of what an Android release build actually consumes.

The old stale-native guard in ``build-apk.sh`` was an mtime walk over
``crates/`` and ``apps/``:

* it included ``examples/`` (a newer example file blocked packaging even though
  the cdylib never compiles it), and
* it missed vendored runtime code (``third_party/wgpu-hal`` via
  ``[patch.crates-io]``) and ``Cargo.lock``.

This tool hashes the real input set instead: the path-dependency closure of the
``wc3rust-android`` cdylib (parsed from the manifests, not a filesystem guess),
the lockfile, the vendored patch, and the packaging tree. Contents, not mtimes,
decide whether ``libmain.so`` matches the tree.

Usage:
  scripts/build_inputs.py native                 # digest of the cdylib inputs
  scripts/build_inputs.py native --json          # + file list and per-file sha
  scripts/build_inputs.py native --newer-than F  # mtime fallback (no stamp)
  scripts/build_inputs.py packaging --json       # APK packaging inputs
  scripts/build_inputs.py source-state --json    # HEAD + dirty source digest
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP_MANIFEST = ROOT / "apps" / "android-native" / "Cargo.toml"
APP_ROOT = ROOT / "apps" / "android" / "app" / "src" / "main"

# Dependencies in these sections are compiled into (or run for) the cdylib.
# dev-dependencies are excluded: they are inputs of `cargo test`, never of the
# shipped library, so editing them must not block packaging.
DEP_SECTION = re.compile(
    r"^(dependencies|build-dependencies|target\..*\.dependencies|target\..*\.build-dependencies)$"
)
PATCH_SECTION = re.compile(r"^patch\..+$")
PATH_LINE = re.compile(r'\bpath\s*=\s*"([^"]+)"')


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_manifest(path: Path) -> tuple[set[Path], list[str]]:
    """Return path dependencies declared by one manifest.

    The parser is intentionally small: it only needs the section header and the
    ``path = "..."`` lines inside dependency-like sections. Anything else in a
    Cargo manifest (versions, features, lints) does not change the input set.
    """
    deps: set[Path] = set()
    sections: list[str] = []
    section = ""
    try:
        lines = path.read_text(errors="replace").splitlines()
    except OSError:
        return deps, sections
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped[1:-1].strip()
            sections.append(section)
            continue
        if not (DEP_SECTION.match(section) or PATCH_SECTION.match(section)):
            continue
        match = PATH_LINE.search(line)
        if not match:
            continue
        dep = (path.parent / match.group(1)).resolve()
        if dep.is_file():  # a path dep always points at a directory
            dep = dep.parent
        deps.add(dep)
    return deps, sections


def path_closure(start: Path) -> tuple[list[Path], set[Path]]:
    """Walk the path-dependency closure starting at the app manifest."""
    queue = [start]
    seen: set[Path] = set()
    manifests: list[Path] = []
    while queue:
        manifest = queue.pop()
        if manifest in seen or not manifest.is_file():
            continue
        seen.add(manifest)
        manifests.append(manifest)
        deps, _ = parse_manifest(manifest)
        for dep in sorted(deps):
            queue.append(dep / "Cargo.toml")
    # Root workspace manifests can add [patch.crates-io] entries to the graph.
    root_manifest = ROOT / "Cargo.toml"
    if root_manifest.is_file() and root_manifest not in seen:
        seen.add(root_manifest)
        manifests.append(root_manifest)
        deps, _ = parse_manifest(root_manifest)
        for dep in sorted(deps):
            queue.append(dep / "Cargo.toml")
    # Drain the patch deps found in the root manifest as well.
    while queue:
        manifest = queue.pop()
        if manifest in seen or not manifest.is_file():
            continue
        seen.add(manifest)
        manifests.append(manifest)
        deps, _ = parse_manifest(manifest)
        for dep in sorted(deps):
            queue.append(dep / "Cargo.toml")
    return sorted(manifests), seen


def package_dir(manifest: Path) -> Path:
    return manifest.parent


def iter_tree(base: Path) -> list[Path]:
    files: list[Path] = []
    if not base.is_dir():
        return files
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d not in ("target", ".git"))
        for name in sorted(filenames):
            files.append(Path(dirpath) / name)
    return files


def native_inputs() -> list[Path]:
    files: list[Path] = []
    manifests, dirs = path_closure(APP_MANIFEST)
    for manifest in manifests:
        files.append(manifest)
        pkg = package_dir(manifest)
        build_rs = pkg / "build.rs"
        if build_rs.is_file():
            files.append(build_rs)
        for path in iter_tree(pkg / "src"):
            rel = path.relative_to(pkg).as_posix()
            # Binary targets are compiled by `cargo build --bins`/tests, never
            # into the cdylib. Lanes edit src/bin/* probes constantly; they must
            # not invalidate the release library.
            if rel == "src/main.rs" or rel.startswith("src/bin/"):
                continue
            files.append(path)
    # The lockfile decides which registry versions are compiled.
    files.append(ROOT / "Cargo.lock")
    cargo_config = ROOT / ".cargo" / "config.toml"
    if cargo_config.is_file():
        files.append(cargo_config)
    # Vendored crates reachable through [patch.crates-io] carry their own
    # tree; path_closure() already added their manifest/src, this keeps any
    # non-Rust file the build may read (include_str!/build scripts) in scope.
    for dep in sorted(dirs):
        if dep == ROOT:
            continue
        try:
            dep.relative_to(ROOT)
        except ValueError:
            continue
        patch_manifest = dep / "Cargo.toml"
        if patch_manifest.is_file() and "third_party" in dep.parts:
            files.extend(iter_tree(dep))
    unique = {path.resolve() for path in files if path.is_file()}
    return sorted(unique)


def packaging_inputs() -> list[Path]:
    files = iter_tree(APP_ROOT)
    for name in (
        "build-apk.sh",
        "build_inputs.py",
        "apk_pack.py",
        "build-deps.json",
        "build_deps.py",
        "release_security_summary.py",
    ):
        candidate = ROOT / "scripts" / name
        if candidate.is_file():
            files.append(candidate)
    return sorted({path.resolve() for path in files if path.is_file()})


def digest(paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in paths:
        rel = path.relative_to(ROOT).as_posix()
        digest.update(rel.encode())
        digest.update(b"\0")
        digest.update(sha256_file(path).encode())
        digest.update(b"\n")
    return digest.hexdigest()


def describe(paths: list[Path]) -> dict:
    return {
        "digest": digest(paths),
        "count": len(paths),
        "roots": sorted({path.relative_to(ROOT).parts[0] for path in paths}),
        "files": [
            {"path": path.relative_to(ROOT).as_posix(), "sha256": sha256_file(path)}
            for path in paths
        ],
    }


def git(*args: str) -> str:
    proc = subprocess.run(
        ["git", *args], cwd=str(ROOT), capture_output=True, text=True, timeout=120
    )
    if proc.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} rc={proc.returncode}: {proc.stderr.strip()}")
    return proc.stdout


def source_state() -> dict:
    """HEAD + a content digest of every source change since HEAD.

    The digest covers tracked modifications/deletions and *untracked build
    inputs* (a new source file under crates/ or apps/android-native/ is part
    of the build). Unrelated untracked analysis/report files are recorded in
    the status list but do not change the digest: a lane writing a report must
    not invalidate a release manifest.
    """
    head = git("rev-parse", "HEAD").strip()
    status_lines = [line for line in git("status", "--porcelain").splitlines() if line]
    tracked = [line for line in status_lines if not line.startswith("??")]
    untracked = [line[3:] for line in status_lines if line.startswith("??")]
    native = {p.relative_to(ROOT).as_posix() for p in native_inputs()}
    packaging = {p.relative_to(ROOT).as_posix() for p in packaging_inputs()}
    inputs = native | packaging
    rows: list[str] = []
    for line in tracked:
        status, path = line[:2], line[3:]
        if " -> " in path:  # porcelain rename: "R  old -> new"
            path = path.split(" -> ", 1)[1]
        target = ROOT / path
        content = sha256_file(target) if target.is_file() else "DELETED"
        rows.append(f"{status}\t{path}\t{content}")
    untracked_inputs = 0
    untracked_input_paths: list[str] = []
    for entry in untracked:
        # Native/packaging inputs are enumerated from the filesystem, so a new
        # untracked source file is already part of the inventory and matches
        # here; reports and other untracked artifacts do not. Git collapses a
        # wholly-untracked directory (e.g. `.cargo/`) to one entry ending in
        # "/", so expand those against the input set.
        paths = sorted(inputs) if entry.endswith("/") else [entry]
        if entry.endswith("/"):
            paths = [path for path in paths if path.startswith(entry)]
        for path in paths:
            if path not in inputs:
                continue
            target = ROOT / path
            content = sha256_file(target) if target.is_file() else "MISSING"
            rows.append(f"??\t{path}\t{content}")
            untracked_inputs += 1
            untracked_input_paths.append(path)
    rows.sort()
    dirty_digest = hashlib.sha256("\n".join(rows).encode()).hexdigest()
    return {
        "head": head,
        "dirty_count": len(status_lines),
        "tracked_dirty_count": len(tracked),
        "untracked_input_count": untracked_inputs,
        "source_dirty_digest": dirty_digest,
        "dirty_paths": [line[3:] for line in tracked],
        "untracked_input_paths": untracked_input_paths,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("kind", choices=["native", "packaging", "source-state"])
    parser.add_argument("--json", action="store_true", help="print the full description")
    parser.add_argument("--newer-than", type=Path, help="print native inputs newer than this artifact")
    args = parser.parse_args(argv)

    if args.kind == "source-state":
        print(json.dumps(source_state(), indent=2))
        return 0

    paths = native_inputs() if args.kind == "native" else packaging_inputs()
    if args.newer_than:
        reference = args.newer_than.stat().st_mtime if args.newer_than.exists() else 0.0
        newer = [p for p in paths if p.stat().st_mtime > reference]
        print(json.dumps([str(p.relative_to(ROOT)) for p in newer], indent=2))
        return 1 if newer else 0
    if args.json:
        print(json.dumps(describe(paths), indent=2))
    else:
        print(digest(paths))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
