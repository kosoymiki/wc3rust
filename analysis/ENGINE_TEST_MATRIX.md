# ENGINE TEST MATRIX — 2026-11-12

All runs use `scripts/safe-test.sh <crate>`: one crate, detached,
`CARGO_BUILD_JOBS=1`, `CARGO_PROFILE_TEST_DEBUG=0`, `--test-threads=1`,
`ulimit -v 4.5 GB`, log `analysis/test-<crate>.log`, marker `SAFE_TEST_DONE`.

Why: two uncapped in-session `cargo test` runs triggered the kernel OOM killer
(dmesg: `wc3_assetdb-986 ... anon-rss:7749064kB`). The guard plus the allocation
fixes below removed the 7.7 GB spike; subsequent runs stayed inside the cap and
did not log a new OOM.

| crate | result | evidence |
|---|---|---|
| `wc3-mpq` | **3/3 pass** | `analysis/test-wc3-mpq.log` |
| `wc3-formats` | **20/20 pass** | `analysis/test-wc3-formats.log` |
| `wc3-assetdb` | **1/1 pass** | `analysis/test-wc3-assetdb.log` |
| `wc3-path` | **4/4 pass** | `analysis/test-wc3-path.log` |
| `wc3-sim` | **5/5 pass** | `analysis/test-wc3-sim.log` |
| `wc3-jass` | **4/4 pass** | `analysis/test-wc3-jass.log` |
| `wc3-replay` | **4/4 pass** | `analysis/test-wc3-replay.log` |
| `wc3-input` | **4/4 pass** | `analysis/test-wc3-input.log` |
| `wc3-audio` | **4/4 pass** | `analysis/test-wc3-audio.log` |
| `wc3-anim` | **3/3 pass** | `analysis/test-wc3-anim.log` |
| `wc3-render` | **1/1 pass** (Vulkan offscreen) | `analysis/test-wc3-render.log` |
| `wc3-adrenotools` | 27/27 pass (earlier direct run) | `cargo test -p wc3-adrenotools` |
| APK | PASS on RMX3709 | `analysis/android-p0.log`, `analysis/p0-apk-final.png` |
| `driver-probe` | 0 failures | `analysis/adrenotools-probe.out` |
| `apps/headless` | **core pass** (map/sim/replay; JASS parses 399 functions) | `analysis/headless.out` |
| `apps/bench` | **pass**: MPQ 25 ms, MDX 1 ms, BLP 3 ms, map 5 ms, A* 0 ms, 400 units x1000 ticks 419 ms | `analysis/bench.out` |

## Root causes fixed while validating

1. **MPQ**: hash lookup used hash type 1 instead of type 0 for the start slot;
   uncompressed files were routed through the sector table; compressed sectors
   need the one-byte codec tag (`0x02` zlib, `0x10` bzip2); W3M maps have a
   `HM3W` header before the MPQ (`open_at` base offset).
2. **MDX**: material chunk has no total-size prefix; node/geoset/material
   inclusive sizes are now validated; EVTS has a `KEVT` track after the node;
   ATCH has an outer size around a nested Node; unknown node tracks are skipped
   to the node end. Counts are checked against remaining bytes before allocation.
3. **BLP**: BLP1 `content_type`/`alpha_bits` fields; JPEG stream is
   `header_size` bytes of JPEG header followed by mip 0 data; paletted alpha is
   8/4/1-bit. This removed the multi-gigabyte JPEG allocation attempt.
4. **SLK**: X/Y fields are stateful across `C;` lines (`C;X2;K...` reuses Y).
5. **Object data**: version 1 has no variation field.


## Headless / bench (observed)

`analysis/headless.out`:
```
MAP terrain=129x129 tileset=V tiles=16641
MAP pathing=512x512
MAP script_bytes=221222
SIM tick=300 hash=7a746942b92ac811 ... verify_hash=7a746942b92ac811 match=true
REPLAY bytes=142 verified_hash=7a746942b92ac811 match=true
JASS parse=ok functions=399 globals=294
JASS main=error unknown function GetCameraMargin
HEADLESS_RC=0
```

`analysis/bench.out`:
```
BENCH mpq_open_ms=25 blocks=10560 listfile=10523
BENCH mdx_read_parse_ms=1 bytes=105022 geosets=5 bones=25 faces=372
BENCH blp_decode_ms=3 bytes=98795 rgba=262144 mips=1
BENCH map_open_ms=5 terrain=129x129 units=0
BENCH astar_ms=0 path_cells=145
BENCH sim_1000_ticks_ms=419 units=400 tick_us=419.80
```

## Remaining explicit boundary from this run

`war3map.wts` (map strings) uses MPQ PKWARE implode (compression 0x08); the reader
returns `UnsupportedCompression(0x0008)`, so map names stay as `TRIGSTR_nnn`.
All geometry/gameplay/script files load (zlib). Next step: port `pklib/explode.c`
from the StormLib donor (MIT) behind the existing `MpqError::UnsupportedCompression`
branch, with a WTS round-trip test.
