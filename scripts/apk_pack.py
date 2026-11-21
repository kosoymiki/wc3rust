#!/usr/bin/env python3
"""Deterministically assemble the unsigned APK.

`cp` + `zip -r` stamp the wall clock into every ZIP entry, so two builds of the
same tree produced different APK hashes (K-03). This packer rewrites the whole
archive with:

  * one fixed timestamp derived from SOURCE_DATE_EPOCH (default: the HEAD
    commit time),
  * a fixed entry order (sorted by name),
  * fixed Unix permissions and no extra fields,

while preserving each aapt2 entry's compression method (resources.arsc must stay
stored) and setting the same method for the added classes.dex / native libs.
zipalign and apksigner are deterministic on this input (measured in the audit).

Usage:
  scripts/apk_pack.py --base base.apk --dex classes.dex --lib-dir lib \
      --out unsigned.apk --epoch 1794900000
"""

from __future__ import annotations

import argparse
import hashlib
import json
import time
import zipfile
from pathlib import Path

# ZIP stores MS-DOS local time; the earliest representable date is 1980-01-01.
ZIP_MIN_EPOCH = 315532800
DEFAULT_COMPRESS = zipfile.ZIP_DEFLATED


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fixed_info(name: str, compress_type: int, epoch: int) -> zipfile.ZipInfo:
    stamp = time.gmtime(max(epoch, ZIP_MIN_EPOCH))
    info = zipfile.ZipInfo(name, date_time=(stamp.tm_year, stamp.tm_mon, stamp.tm_mday,
                                          stamp.tm_hour, stamp.tm_min, stamp.tm_sec))
    info.compress_type = compress_type
    # Regular file, 0644; create_system 0 keeps the "version made by" field
    # stable across hosts.
    info.external_attr = 0o644 << 16
    info.create_system = 0
    info.extra = b""
    info.comment = b""
    return info


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--base", type=Path, required=True, help="aapt2-linked base.apk")
    parser.add_argument("--dex", type=Path, required=True, help="classes.dex from d8")
    parser.add_argument("--lib-dir", type=Path, required=True, help="directory holding lib/<abi>/*.so")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--epoch", type=int, required=True)
    args = parser.parse_args(argv)

    entries: dict[str, tuple[bytes, int]] = {}
    with zipfile.ZipFile(args.base) as base:
        for info in base.infolist():
            if info.is_dir():
                continue
            entries[info.filename] = (base.read(info), info.compress_type)

    entries["classes.dex"] = (args.dex.read_bytes(), DEFAULT_COMPRESS)
    for path in sorted(args.lib_dir.rglob("*")):
        if not path.is_file():
            continue
        name = path.relative_to(args.lib_dir).as_posix()
        entries[f"lib/{name}"] = (path.read_bytes(), DEFAULT_COMPRESS)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.out, "w") as out:
        for name in sorted(entries):
            payload, compress = entries[name]
            out.writestr(fixed_info(name, compress, args.epoch), payload)

    print(json.dumps({
        "out": str(args.out),
        "sha256": sha256_file(args.out),
        "bytes": args.out.stat().st_size,
        "entries": len(entries),
        "epoch": args.epoch,
    }))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
