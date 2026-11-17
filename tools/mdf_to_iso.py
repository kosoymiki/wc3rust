#!/usr/bin/env python3
"""Convert a raw 2352-byte/sector Alcohol MDF image to a 2048-byte ISO.

Measured on the retail disc: the image begins with a **1536-byte header**, then
raw mode-1 sectors: 12 sync bytes + 4 header + 2048 data + 288 EDC/ECC. Plain
ISO tools refuse the file, which is why `7z l -tiso` answered "Is not archive".

The converter finds the first sync pattern instead of assuming offset 0, locates
the ISO9660 primary volume descriptor (`\\x01CD001`) and derives how many leading
sectors to skip so the PVD lands on ISO sector 16.

usage: mdf_to_iso.py <image.mdf> <out.iso>
"""
import sys
from pathlib import Path

# Alcohol images on these discs come in two framings: plain raw mode-1
# (2352 bytes/sector) and raw + 96 bytes of subchannel (2448). The primary
# volume descriptor tells us which one is in use, so it is detected, not assumed.
SECTOR_CANDIDATES = (2352, 2448)
SECTOR_DATA = 2048
SYNC = bytes([0x00] + [0xFF] * 10 + [0x00])


def first_sync(f, probe: int = 1 << 20) -> int:
    head = f.read(probe)
    return head.find(SYNC)


def find_pvd(f, base: int, sectors: int, sector_raw: int) -> int:
    for index in range(min(sectors, 64)):
        f.seek(base + index * sector_raw)
        head = f.read(24)
        if head[16:17] == b"\x01" and head[17:22] == b"CD001":
            return index
    return -1


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    size = src.stat().st_size
    with src.open("rb") as f:
        base = first_sync(f)
        if base < 0:
            print("no sync pattern in the first MiB: not a raw CD image")
            return 1
        pvd = -1
        sector_raw = 0
        for candidate in SECTOR_CANDIDATES:
            sectors = (size - base) // candidate
            pvd = find_pvd(f, base, sectors, candidate)
            if pvd >= 0:
                sector_raw = candidate
                break
        if sector_raw == 0:
            print("no ISO9660 primary volume descriptor; image is not a raw ISO")
            return 1
        sectors = (size - base) // sector_raw
        if pvd < 0:
            print("no ISO9660 primary volume descriptor; image is not mode-1 ISO")
            return 1
        skip = pvd - 16
        print(f"base={base} sector={sector_raw} pvd=raw[{pvd}] skip={skip} sectors={sectors}")
        f.seek(base)
        written = 0
        with dst.open("wb") as out:
            for index in range(sectors):
                sector = f.read(sector_raw)
                if len(sector) < sector_raw:
                    break
                if index < skip:
                    continue
                out.write(sector[16:16 + SECTOR_DATA])
                written += SECTOR_DATA
                if index % 50000 == 0:
                    print(f"  sector {index}/{sectors}", flush=True)
    print(f"wrote {dst} bytes={written} ({written / 1048576:.1f} MiB)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
