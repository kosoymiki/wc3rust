# INTERIM AUDIT TZ — assignment (WC3Rust_INTERIM_AUDIT_TZ.md, 2026-11-18)

Source: `/storage/emulated/0/Download/WC3Rust_INTERIM_AUDIT_TZ.md` (654 lines, 21 findings
WC3-I-001…021 + §9 unfinished zones). Every item below has exactly one owner; no two lanes
write the same file.

## Rules (same as the fix wave)

- No new tests (`#[test]`, `tests/**`): a finding is proven by `file:line` + a one-shot probe
  under `analysis/full-audit-2026-11-18/probes/<ID>/` + its exact output. Where the TZ asks
  for a "regression test", record the intended test plus `BLOCKED: operator test freeze` and
  the reproducer.
- No device commands (`am`, `input`, `pm`, `screencap`, `logcat -c`). The operator opens the app.
- `scripts/h2-cargo` only. Donor/format data from the shipped archives; no invented values.
- Licence bookkeeping is out (promth2): provenance (repo/path/commit/sha256) only.

## Mapping

| TZ item | subject | owner (ledger) |
|---|---|---|
| I-001 | BLP2 DXT decoder choice ignores alpha encoding | **538** formats |
| I-002 | BLP2 paletted alpha plane | **538** |
| I-003 | BLP1 paletted mip bounds | **538** |
| I-010 | BLP JPEG dimensions vs mip | **538** |
| I-009 | MDX animation track timestamp monotonicity | **538** (MDX parser) |
| I-021 | MPQ listfile dedup O(N²) | **538** |
| I-005 | Replay verification has no visible end_tick bound | **539** sim/replay |
| I-004 | BCn patching creates an RWX memory window | **540** render/gpu |
| I-012 | Fog CPU mask sampling offset vs shader | **540** |
| I-013 | Fog full-screen pass recolours background/sky | **540** |
| I-014 | GPU skinning bone index without bounds check | **540** |
| I-015 | Skin influences silently truncated to five | **540** |
| I-016 | Skeleton allocation from max MDX object ID | **540** |
| I-017 | Normal transform uses world matrix (non-uniform scale) | **540** |
| I-018 | Texture pre-resize averages sRGB bytes | **540** |
| I-019 | NPOT mipmap loses last row/column | **540** |
| I-020 | Map discovery can recurse symlink cycles | **540** (`wc3-game/src/map_registry.rs`, `assets.rs` discovery) |
| I-006 | Android restore: no fallback after the first corrupt checkpoint | **541** platform |
| I-007 | Android migration never reads the old package sandbox | **541** |
| I-008 | Android checkpoint callback masks a save failure | **541** |
| I-011 | MP3 initial pitch + frame arithmetic | **545** (new, audio) |
| §9 network protocol, command validation, `wc3-guest` boundary | **546** (new, audit+fix) |
| §9 tests/fuzz/coverage gaps | **546** (record gaps; no new test files under the freeze) |

## Lane notes

- **538**: the TZ explicitly asks to *verify the BLP2 field tuples against the formal spec and
  reference assets before changing the decoder*. Do that first (stealth-browser for
  `wowdev.wiki/BLP`), then implement; unknown tuples must be a controlled error, not a guess.
  Build byte-level fixtures under `probes/` (raw bytes + expected texel), not under `tests/`.
- **540**: I-004 is the security one (W^X): a patched texture must never live in RWX memory; use
  a non-executable mapping and prove it (`/proc/self/maps` line in the probe).
- **545**: `crates/wc3-audio/**` was audited by lane I but has no fix owner; take I-011 plus the
  media gap of §9 (MP3 frame arithmetic, pitch bounds, AVI/Bink audio sync).
- **546**: read `crates/wc3-sim/src/net.rs`, `wc3-guest`, `wc3-replay` end-to-end; findings in the
  TZ template; fixes only where reachable from the product path. Record coverage, do not claim a
  zone is safe because nothing was found.

## Assembly

The tech-lead merges these into `FINDINGS_P0-P4.md` together with lanes G/H/I/J/K and the first
fix wave, then runs the Red Team phase and the GO/NO-GO.
