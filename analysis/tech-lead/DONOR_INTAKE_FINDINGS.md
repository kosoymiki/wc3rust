# Donor-intake findings — original game extraction (ledger 386)

Tool: `analysis/agent-re/tools/donorintake` (detached cargo package; reads the
project's own parsers from `crates/`, which are READ-ONLY for this task).
Artifacts: `analysis/agent-re/map_index.json`, `analysis/agent-re/MELEE_VERIFY_JUSTICE.md`,
`analysis/agent-re/original_assets.json`, `analysis/agent-re/MAP_CORPUS.md`.

Each finding states observed evidence, the owning layer, and the exact next
step. Items 1-3 were fixed in-tree by their owner while this task ran; the
tool keeps a labelled fallback for older binaries.

---

## 1. Runtime data tables cover 30/85 neutral placements on the chosen 1v1 map

**Observed** (`analysis/agent-re/melee_verify_justice.json`, `maps verify` on
`(2)Justice.w3x`):

| category | map placements | defined by shipped data | runtime consumed |
|---|---:|---:|---:|
| starts | 2 | 2 | 2 |
| gold mines | 7 | 7 | 7 |
| destructable doodads (incl. 1856 trees) | 2902 | 1997 | 1997 |
| decoration doodads | 905 | — | — |
| neutral creeps | 85 | 83 | 30 |
| shops | 5 | 5 | 0 |
| camps (600-unit clustering) | 26 | — | 11 registered, 2 fully spawnable |

**Gap ids** (defined by `analysis/war3x_data/Units/UnitData.slk`, absent from
`wc3_sim::data::def_of`):
`nfoh, nfsh, nfsp, ngad, nggr, nhdc, nhfp, nhhr, nkot, nmoo, nmr8, nmrl, nomg,
nrdr, nrog, nshe, nsqt, ntav, nwzg` — 58 placements.

`wc3_sim::data` currently ships 103 roster + 18 creep + 1 shop + 12 extra defs
(`crates/wc3-sim/src/{roster,creeps,shops,extras}_gen.rs`), so the TFT neutral
rows are simply not generated yet.

**Owner / next step:** sim data generator (`analysis/agent-sim/gen_creeps.py`,
`gen_shops.py`, `gen_neutrals.py` target `analysis/war3x_data`). Regenerating
`crates/wc3-sim/src/creeps_gen.rs` / `shops_gen.rs` from the TFT tables and
re-running `cargo test -p wc3-sim` closes the gap; `donorintake maps verify`
then reports `parity_ok=true`. No map-side change is needed — the map already
places every camp/shop correctly.

---

## 2. `war3map.doo` version 6 (RoC 1.00/1.03/1.10 beta) — FIXED in-tree

**Observed before the fix:** 15 beta maps failed with
`doodad placement version` (`Beta ROC 100/103/110` sets).
**Byte evidence:** `W3do | 06 00 00 00 | 07 00 00 00 | 96 11 00 00`; record
layout is the v7 layout minus the `life` byte — the second `LTlt` record starts
exactly 41 bytes after the first (`id`4+`var`4+`pos`12+`angle`4+`scale`12+`state`1+`creation`4).

**Current tree:** `crates/wc3-formats/src/map.rs` `parse_doodads_ex` accepts
`6..=13` (owner change landed during the session). The index now reports
`doodad_reader: canonical` for **99/99** maps.
`analysis/agent-re/tools/donorintake/src/doodads.rs` keeps a labelled v6
fallback for binaries whose canonical parser rejects v6.

---

## 3. `war3mapUnits.doo` v7/sub10 hero attributes — FIXED in-tree

**Observed before the fix:** 21 maps (beta RoC 100-110, beta TFT 299) had no
placements (`units_reader: absent`), so start/mine columns were empty.
**Root cause / authority:** War3Net `UnitData.ReadFrom` reads STR/AGI/INT when
`(formatVersion >= v8 && subVersion == v11) || subVersion == v10`; the parser
read them only for `subversion >= 11`. Skipping 12 bytes makes the next field
read `gold` (12500 on a mine) as the item-set count and abort with
"unit data truncated".

**Current tree:** `crates/wc3-formats/src/map.rs:163` now uses
`(version >= 8 && subversion == 11) || subversion == 10`; the index reports
`units_reader: canonical` on 98/99 maps (1 map has no unit file).
`src/units.rs` keeps the corrected reader as a labelled fallback; its test
parses `HailStone` (v7/sub10) as 129 units, 4 starts, 14 mines.

---

## 4. `war3map.w3i` v8-v15 names — FIXED in-tree

**Observed before the fix:** `wc3-formats::w3i::MapInfo::parse` read
`saves`/`editor_version` for every version, so a v11 map's name came back as
`"ple"` (the tail of `"Lost Temple"`, whose strings start at offset 4).
**Current tree:** `crates/wc3-formats/src/w3i.rs:20-35` gates the v18+ prefix
and the v27+ game version. The index additionally uses the full header reader
(`src/w3i.rs`, War3Net layout incl. players/forces) and resolves `TRIGSTR_nn`
through the map's own `war3map.wts`.

---

## 5. Retail sound/UI drift (reported, not gated)

`analysis/agent-re/original_assets.json` — informational families:

- `sounds.unit_voice` 1704/1709. Five upstream data defects, each verified
  against the archive listfiles:
  - `FrostwyrmPissed5,wav` in `UnitAckSounds.slk` (comma typo) → path
    `Units\Undead\FrostWyrm\wav`;
  - `MoonPriestessWarcry1.wav` — the install ships `HeroMoonPriestessWarcry1.wav`;
  - `IceTrollPissed5.wav` — the install ships `IceTrollPissed1..4.wav`;
  - `Buildings\Orc\PigFarm\PigFarmWhat1.wav` — the install ships
    `buildings\Other\PigFarm\PigFarmWhat1.wav`;
  - `SacrificialAltarWhat1.wav` — absent from the install.
- `voices.*` — 18 `unitSound` set names referenced by unit rows do not exist in
  `UnitAckSounds` (`HumanMage`, `Medivh`, `Frostmourne`, `DemonGate`,
  `FountainOfLifeBlood`, …): table-vs-table drift in the shipped data.
- `ui.skin.*` 22 — `war3skins.txt` names menu/glue textures and `.mdl` models
  that this install does not ship (`UI\Glues\MenuBackground.blp`,
  `UI\Console\ConsoleBackground.blp`, `UI\Glues\MainMenu\MainMenuButton\…`).
  The in-game HUD does not read these; the runtime HUD uses its own FDF paths,
  all 86 of which resolve.
- `sounds.UnitAckSounds` 83, `sounds.AnimSounds` 25, `sounds.DialogSounds` 2 —
  whole-library rows whose files are absent from this repack install; the
  runtime only plays the event-mapped subset, which resolves 47/47.

No action for the port; the report keeps them visible so a future install
swap (clean ISO) can be re-audited with the same command.

---

## 6. Install path note

`/data/data/org.herofolends.p0/files/Melee.w3x` is app-uid `0600`; reading it
from Termux needs root. `install_melee_maps.sh` writes a scratch copy inside the
app dir, verifies SHA-256, then `mv`s it over the target (atomic rename, keeps
`Melee.w3x.bak-<stamp>` for rollback). Both shipped paths hash to
`91e22d5ba14b4bcc896f46ac41db31679b8f3226e13f2588b3e617ab1628f1ee`.
