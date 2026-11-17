# BASE.md — one base, one canonical owner per concern

Status: **v0 EXECUTION BASE — PUBLISHED** (2026-11-12, resumed 2026-11-13). Canonical targets below are binding for layer merges. Layer manifests may propose corrections; the dispatcher folds accepted corrections into v1/v2. Every layer executes in its OWNER_PATHS (§12) now; tests run per layer after its scope compiles, full suite after this base is green.

Authority: this file + `analysis/tech-lead/WC3RE_MERGE_MATRIX.md`. Where a layer
manifest and this file disagree, this file wins until v2.

## 0. Non-negotiable rules

1. **One implementation per concern.** A second reader/parser/renderer is
   deleted or reduced to a delegating compatibility facade. No copy that keeps
   its own byte logic.
2. **No invented data.** Every UI, terrain, model, stat, map and script value in
   the base comes from the operator's original Warcraft III files through the
   canonical parser named below. Procedural fallbacks are allowed only when no
   install is present, must be labelled `source=fallback`, and are never part of
   the data base.
3. **One writer per file.** Each layer owns its listed paths; everything else is
   read-only. Changes to read-only files are delivered as a patch with exact
   lines to the dispatcher.
4. **No tests / no device runs until the freeze announcement.** Implementation,
   port manifests and provenance docs continue now. `cargo test`, APK installs,
   and device automation wait for the dispatcher's freeze message.
5. **Evidence only.** Every port manifest entry cites source `file:line` and
   target `file:line`; no README claims, no stubs, no TODO-as-delivery.

## 1. One crate graph (workspace members only)

```
wc3-mpq        (container + codecs; no workspace deps)
wc3-core       (ids/math primitives)
wc3-path       (pathing primitives)
wc3-audio      (PCM/mp3 decode primitives; no MPQ dependency)
wc3-formats    -> wc3-mpq, glam, jpeg-decoder
wc3-sim        -> wc3-core, wc3-path
wc3-anim       -> wc3-formats, glam
wc3-re         -> wc3-mpq, wc3-formats, flate2, bzip2
wc3-jass       -> wc3-sim, wc3-core, wc3-re          (legacy VM + canonical adapter)
wc3-data       -> wc3-formats                         (workspace member since v1.1)
wc3-assetdb    -> wc3-mpq, wc3-formats, wc3-audio
wc3-render     -> wc3-formats, wc3-sim, wc3-anim
wc3-replay     -> wc3-sim, glam
wc3-input      -> wc3-core, glam
wc3-game       -> wc3-core, wc3-formats, wc3-re, wc3-assetdb,
                  wc3-path, wc3-sim, wc3-anim, wc3-input
herofolends-android -> wc3-game
```

No crate may depend on `wc3-re` except `wc3-jass` (canonical adapter) and
`wc3-game` (RE/engine bridge); no crate may depend on `wc3-game`.

## 2. Canonical owners

| Concern | Canonical owner | Public entry points | Everyone else |
|---|---|---|---|
| MPQ header/hash/block tables, decryption, sectors, listfile | `wc3-mpq` | `MpqArchive::{open, open_at, read, stream, meta, listfile, files}`, `MpqIndex`, `MpqFileReader` | `wc3-re::mpqex::MpqEx` is a facade; `mapfiles`/`assetdb` call `MpqArchive` |
| MPQ codecs | `wc3-mpq::{pkware,huffman,adpcm}` | `pkware::explode`, `huffman::decompress`, `adpcm::decompress` | `wc3-re::mpqex::{pkware,huffman,adpcm}` re-export only |
| BLP1/BLP2 decode | `wc3-formats::blp` | `decode -> BlpImage{width,height,rgba,mipmaps}` | `wc3-re::blp` adds probe/encoding; `wc3-assetdb` caches decoded images |
| MDX v800 parse/anim tracks | `wc3-formats::mdx` | `MdxModel::parse`, `Node/Bone/Sequence/Geoset/Material/Layer/Texture`, `Track::sample` | `wc3-re::mdx` adds chunk scanner/summary; `wc3-anim` samples tracks |
| SLK grid | `wc3-formats::slk::SlkTable` (primitive) + `wc3-formats::gamedata::Sheet` (strict typed) | `SlkTable::parse`, `Sheet::parse/cell/row_id` | `wc3-re::slk::Sheet` delegates parsing and adds the row-key view |
| Object data `.w3u/.w3a/.w3t/.w3q/.w3b/.w3d/.w3h` | `wc3-formats::gamedata::ObjectDataFile` | `ObjectDataFile::parse(kind, bytes)`, `ObjectChange`, `ChangeValue` | `wc3-re::mapobjects` projects it; `wc3-formats::objects` projects it; `wc3-data` types it |
| Object metadata | `wc3-formats::gamedata::MetaTable` | `MetaTable::parse`, `resolve_field`, `merge_overlay` | `wc3-re::objectdata` builds the install DB; `wc3-data` consumes |
| `.wts` grammar | `wc3-formats::gamedata::Wts` | `Wts::parse/get/resolve/iter` | `wc3-re::wts` adds escape decoding + multi-reference resolution |
| INI/text tables | `wc3-formats::gamedata::TextTable` | `TextTable::parse/sections/get/values` | `wc3-re::txt` adds ordered sections, preamble and quoted lists; `skin`/`trigger` consume it |
| FDF documents | `wc3-formats::fdf` | `FdfDocument::parse_named`, `walk`, props | FDF layer consumes; `wc3-re::skin::audit_fdf_decorate` audits |
| W3E terrain / W3I info / WPM pathing | `wc3-formats::{w3e,w3i,wpm}` | `Terrain::parse`, `MapInfo::parse`, `PathingMap::parse` | `MapBundle` and `W3Map` embed these; no re-parse |
| W3M/W3X container + inner files | `wc3-re::mapfiles::MapBundle` | `MapBundle::open`, `entries`, `terrain`, `info`, `pathing`, `wts`, `objects`, `script`, `localized_info`, `discover_maps` | `wc3-formats::map::W3Map` is the legacy compatibility view; `open_map_archive`/`map_mpq_base` shared |
| Storm multi-archive/priority/loose tree | `wc3-re::storm::StormFs` | `open_wc3_install`, `add_archive`, `add_loose_dir`, `read`, `info`, `listfile`, `search` | `engine`, `objectdata`, `icons`, `scan` consume |
| Storm ordinal facade | `wc3-re::storm_api` | `StormApi::call`, `STORM_EXPORTS`, `GAME_IMPORTS_STORM`, `validate_exports` | `engine`/`host` |
| Host launcher model | `wc3-re::host` | `HostServices::call/launch`, `HOST_IMPORTS` | `engine` |
| Game.dll boot/loop model | `wc3-re::engine` | `Engine::boot/tick/enter_map/shutdown`, `EngineConfig` | `wc3-game::re` |
| Install object database | `wc3-re::objectdata` | `ObjectDb::load_dir/load_storm`, `record`, `field_by_id`, `apply_map_objects`, `icon_paths` | `icons`, `scan`, `wc3-game::re` |
| Icon audit | `wc3-re::icons` | `audit`, `resolve_icon`, `IconAudit` | HUD layer |
| Skin/FDF decoration | `wc3-re::skin` | `SkinTable::{load,parse,resolve,decorate}`, `audit_fdf_decorate` | FDF/HUD |
| TerrainArt tiles | `wc3-re::terrain` | `TerrainArt::load_dir`, `TerrainTile::texture_path/variant_uv` | world/terrain renderer |
| TriggerData registry | `wc3-re::trigger` | `TriggerData::load_dir/parse`, `GuiFunction`, `TriggerStrings` | HUD/touch action editor |
| JASS2 lexer/parser/VM/natives | `wc3-re::jass` | `parse`, `Program`, `Runtime::{new,run_function,run_main,execute_trigger}`, `World`, `Value`, `natives` | `wc3-jass::canonical` adapter; `engine`/`ai` execute |
| AI scripts | `wc3-re::ai` | `AiScript::parse_file`, `run_init`, `scan_ai_dir` | `engine`, `scan` |
| Whole-install scan | `wc3-re::scan` | `ScanReport::run`, `ok`, JSON/markdown | QA gate |
| Typed game database | `wc3-data` | `GameDatabase`, typed units/abilities/items/upgrades/…, `Source`, `validate` | `wc3-game` (via future bridge) |
| Asset cache/provider | `wc3-assetdb` | `AssetDb::{open_mpq,open_dir,read,texture,model}`, LRU stats, `catalog` | `wc3-game::assets` |
| Simulation | `wc3-sim` | sim world/commands/AI | `wc3-game::game` |
| Touch/input, gestures, camera math | `wc3-input` | `TouchInput`, `TouchLayout`, `TouchButton`, `hit_button`, `Camera`, `minimap_to_world`, `world_to_minimap` | `wc3-game` consumes; `wc3-game::camera` re-exports `Camera` |
| Animation runtime | `wc3-anim` | `Animator`, sequence sampling over `wc3-formats::mdx` | `wc3-game`, `wc3-render` |

## 3. Original-game data provenance

Every row is read from the operator's install (`analysis/war3_full3`,
`analysis/war3_data/War3.mpq`, TFT archives). No values are authored.

| Data | Original source (archive path / bytes) | Format anchor | Canonical parser | Consumers |
|---|---|---|---|---|
| Archives | `War3.mpq`, `War3x.mpq`, `War3xlocal.mpq`, `patch.mpq`; map payload at `MPQ\x1a`, RoC `.w3m` prefixed by `HM3W` (MPQ base 512) | MPQ v0 header, encrypted hash/block tables, sector table | `wc3-mpq::MpqArchive` | everything |
| UI layout | `UI\FrameDef\*.fdf` (e.g. `UI\FrameDef\Human\Console.fdf`) | FDF tokens, `SetPoint`, `DecorateFileNames` | `wc3-formats::fdf` | FDF/HUD |
| UI skin map | `UI\war3skins.txt` (sections `[Main]`, `[Human]`, `[Orc]`, `[NightElf]`, `[Undead]`, `[Default]`) | `[Section] key=value`, `Skins=` list | `wc3-re::skin::SkinTable` | HUD/FDF |
| UI textures | `ReplaceableTextures\CommandButtons\*.blp` (64×64), console BLPs referenced by skins | BLP1 156-byte header, palette/JPEG, mip chain | `wc3-formats::blp` + `wc3-re::blp::probe` | HUD/FDF |
| Terrain tiles | `TerrainArt\Terrain.slk` (`tileID, dir, file, …`) + `TerrainArt\<dir>\<file>.blp` (512×512 2×2 atlas) | SLK `ID;PWXL`; BLP1; W3E tile variation nibble | `wc3-re::terrain::TerrainArt` + `wc3-formats::blp` | world/terrain |
| Terrain mesh | `war3map.w3e` (inside W3M/W3X) | W3E magic/version, ground/corner height, texture layer 4-bit codes, cliff/water | `wc3-formats::w3e::Terrain` | `MapBundle`, `W3Map`, world |
| Pathing | `war3map.wpm` | WPM cell grid | `wc3-formats::wpm::PathingMap` | world/sim |
| Map info | `war3map.w3i` | W3I versioned strings, players, forces | `wc3-formats::w3i::MapInfo` | `MapBundle` |
| Map strings | `war3map.wts` | `STRING n` + `{…}` blocks, `TRIGSTR_n`, escapes | `wc3-formats::gamedata::Wts` (+ `wc3-re::wts` escapes) | `MapBundle::localized_info` |
| Map object overrides | `war3map.w3u/w3t/w3a/w3d/w3b/w3h/w3q` | two sections, NUL strings, v3 sets, extended ability/upgrade/doodad | `wc3-formats::gamedata::ObjectDataFile` | `wc3-re::mapobjects`, `wc3-formats::objects`, `wc3-data` |
| Placed units/doodads | `war3mapUnits.doo`, `war3map.doo` | `W3do` signature, version 7..13, `random_type` payload (0=4 B, 1=8 B, 2=count·8 B, other=0) | `wc3-formats::map` (units/doodads), `Engine::map_spawn` + `JassBridge` | world/game |
| Scripts | `Scripts\common.j`, `Scripts\Blizzard.j`, map `war3map.j`, `Scripts\*.ai` | JASS2 grammar, rawcodes `'hfoo'`, natives | `wc3-re::jass`, `wc3-re::ai` | engine/game/sim |
| Models/anim | `Units\*\*.mdx`, `Buildings\*\*.mdx` | `MDLX` + VERS 800 chunks (SEQS/GLBS/MTLS/TEXS/GEOS/GEOA/BONE/HELP/ATCH/PIVT/PREM/CAMS/EVTS) | `wc3-formats::mdx` | `wc3-re::mdx`, `wc3-anim`, render |
| Model textures | `TEXS` path + `replaceable_id` (1 team colour, 2 team glow) | MDX texture record 268 B | `wc3-re::mdx::referenced_textures` + `blp` | render |
| Stats | `Units\UnitData.slk`, `UnitBalance.slk`, `UnitWeapons.slk`, `UnitAbilities.slk`, `unitUI.slk`, `AbilityData.slk`, `ItemData.slk`, `UpgradeData.slk`, `Doodads\Doodads.slk`, `DestructableData.slk` | SLK grid; key columns `unitID/unitBalanceID/unitWeapID/unitAbilID/unitUIID/alias/itemID/upgradeid` | `wc3-formats::gamedata::Sheet` | `wc3-re::objectdata`, `wc3-data` |
| Editor text fields | `Units\*Func.txt`, `*Strings.txt`, `UnitMetaData.slk`, `MiscData.txt` | INI sections; 4CC modification ids | `wc3-formats::gamedata::{TextTable,MetaTable}` | `wc3-re::objectdata`, `wc3-data` |
| Trigger GUI | `UI\TriggerData.txt`, `UI\TriggerStrings.txt` | sections `TriggerEvents/Conditions/Actions/Calls`, `~param` layouts | `wc3-re::trigger` | HUD/touch |
| Sounds | `Sound\*.wav/.mp3`, `Units\*\*.wav`, `war3map.w3s` | WAV/MP3 + MPQ ADPCM/Huffman sectors | `wc3-audio` + `wc3-mpq` | assets/audio |

Byte anchors already proven by tests/scans: BLP1 header `0x9C`, palette at
`0x9C`, mip offsets `0x1C`; BLP2 header `0x94`; MDX chunks `tag[4]+size u32`;
MPQ v0 header 32 B, sector size `512 << shift` (WC3 shift 3); W3M `HM3W` +
`MPQ\x1a` at 512; Human04 `war3map.wts` PKWARE sectors under a zlib mask;
Human04 `war3map.w3u` v1 two sections (11 base + 28 custom).

## 4. Public API the game/base uses

- `wc3_game::Assets::load(mpq, map)` — fast path, consumes `wc3-assetdb`.
- `wc3_game::re::ReLayer::{open, object, icon_audit, localized_map_name, engine_config, boot_engine}` — canonical RE bridge (new).
- `wc3_re::mapfiles::MapBundle` — canonical map view (pending `assets.rs` patch, see `analysis/tech-lead/patches/0001-assets-mapbundle-wts.patch`).
- `wc3_jass::canonical::{parse, run, run_sources, RunReport}` — canonical JASS VM adapter (new); `wc3_jass::{parse,JassRuntime}` remains for `wc3-sim`.
- `wc3_formats::gamedata::*` — typed format layer used by `wc3-data`.
- `wc3_mpq::{MpqArchive, MpqFileReader, MpqIndex, pkware, huffman, adpcm}` — container/codecs.

## 5. Already consolidated (v1)

- `wc3-re::mpqex::{pkware,huffman,adpcm}` → re-exports of `wc3-mpq` (one codec set).
- `wc3-re::mpqex::MpqEx` → facade over `wc3_mpq::MpqArchive`; base detection via `wc3-formats::map::map_mpq_base`.
- `wc3-mpq` sector reader: authoritative per-sector tag (Human04 PKWARE under zlib mask), PKWARE `0x306` EOF acceptance, plaintext dword tail (no padding decrypt).
- `wc3-re::slk::Sheet` → parses via `wc3-formats::slk::SlkTable`, row-key view only.
- `wc3-re::wts::Wts` → parses via `wc3-formats::gamedata::Wts`, adds escapes/multi-reference resolve.
- `wc3-re::txt::TextTable` → parses via `wc3-formats::gamedata::TextTable`, adds preamble/case-insensitive sections/quoted lists.
- `wc3-re::mapobjects::MapObjectData` → projection of `wc3-formats::gamedata::ObjectDataFile`, kind-aware (`parse_as`).
- `wc3-formats::objects::ObjectData` → projection of `gamedata::ObjectDataFile`; fixed `w3d/w3b/w3h` kind table (HiveWE anchor).
- `wc3-formats::map` WTS/object paths → `gamedata::{Wts,ObjectDataFile}`; `map_mpq_base` shared with `StormFs`/`MapBundle`.

## 6. Write-freeze list (v1)

Other agents must not edit these paths; send a patch + exact lines instead.

```
crates/wc3-re/**                                  (RE layer owner: RE agent + dispatcher)
crates/wc3-mpq/**                                 (dispatcher)
crates/wc3-formats/src/{lib,blp,mdx,slk,objects,map,gamedata,w3e,w3i,wpm}.rs
crates/wc3-formats/Cargo.toml                     (dispatcher)
crates/wc3-jass/**                                (dispatcher; sim owner may read)
crates/wc3-game/Cargo.toml
crates/wc3-game/src/lib.rs
crates/wc3-game/src/re.rs                         (dispatcher; game owners read)
crates/wc3-game/src/assets.rs                     (read-only for everyone; patch-only)
analysis/tech-lead/**                             (dispatcher)
```

Layer-owned areas remain writable by their owner (examples): anim
`crates/wc3-anim/**`, `analysis/agent-anim/**`; HUD `crates/wc3-game/src/{hud,mobile_hud,ui,style}.rs`;
world `crates/wc3-game/src/{terrain_render,world_audit,doodads}.rs`;
GPU `crates/wc3-render/**`; FDF `crates/wc3-formats/src/fdf.rs` + UI docs;
data `crates/wc3-data/**`; sim `crates/wc3-sim/**`; touch `crates/wc3-input/**`.
If a layer needs a frozen file, the manifest carries the patch.

## 7. Gates (after freeze)

1. `cargo metadata --no-deps` → one workspace graph; no duplicate `wc3-*` package.
2. `cargo test -p wc3-mpq`, `-p wc3-formats`, `-p wc3-re`, `-p wc3-data`, `-p wc3-jass` (port tests included).
3. `cargo check -p wc3-game --lib`.
4. `wc3re scan` counts unchanged: 3,935 BLP, 2,048 MDX, 33 SLK, 67 TXT, 71 FDF, 44/44 maps, 91/91 AI, 676 TriggerData, 0 errors.
5. `cargo build --release -p herofolends-android`.
6. Per-layer manifest `analysis/tech-lead/merge-<layer>.md` present and consistent with this file.

## 8. Pending decisions (manifest input)

- `wc3-formats::map::W3Map` removal after `assets.rs` patch lands (headless/bench move to `MapBundle`).
- Whether `wc3-data` gains a direct `ObjectDb` bridge or keeps file-path inputs.
- Single animator: `wc3-anim` vs the animator currently inside the game/render path.
- FDF/text/font ownership split (FDF layer manifest).
- GPU: one renderer — `wc3-render` batched path vs `wc3-game/render.rs` (manifest must prove parity).

## 9. Layer dispatch (2026-11-12, dispatcher)

All sessions are children of the tech-lead root `ses_e4095a1b9ffeRpzK387GOaL3Ly`.
Each received a THICK continuation: finish the layer, port onto this base,
write `analysis/tech-lead/merge-<layer>.md`, no tests/device until freeze, one
writer per file.

| session | layer | manifest | state (dispatch) |
|---|---|---|---|
| `ses_e30595420ffdGjgXSvyWSGFoT0` | anim / MDX+Animator | `merge-anim.md` | DONE layer / waiting base; chunk patch applied; 0003/0004 pending |
| `ses_e30595424ffe9AqABZXayrv0Rb` | HUD / console grid | `merge-hud.md` | DONE layer / waiting base; tests listed |
| `ses_e30595423ffdGkcohq9P8MIxiL` | world / terrain | `merge-world.md` | PARTIAL layer; doodad parser applied (3.3); 3.1/3.2 patches pending |
| `ses_e30595409ffe2wSkLaUsMd22EV` | QA acceptance | `merge-qa.md` | running; manifest pending |
| `ses_e30424702ffe4bTUAPcwFK1lIp` | RE engine | `merge-re.md` | PARTIAL implementation; object DB now uses typed `gamedata::Sheet`; compile-checked |
| `ses_e3059540affeOQRBuuqdX6w6Vh` | assets/audio | `merge-assets.md` | DONE implementation; P1 BLP mips applied (syntax fixed); P2 assets patch pending |
| `ses_e30595412ffdOJEvwiBqSyRYDi` | GPU/render | `merge-gpu.md` (also `merge-render.md` v1) | running; `merge-render.md` present (19 KB) |
| `ses_e3059541bffeFozxokptFv8d5b` | FDF/text/fonts | `merge-fdf.md` | running; manifest pending |
| `ses_e3059541fffejS1YGmXDWBw12Q` | data | `merge-data.md` | running; manifest pending |
| `ses_e3059541fffdUO0AGBqY6t1juG` | sim | `merge-sim.md` | running; manifest pending |
| `ses_e30595423ffe7cc9wy7YBbXx9q` | touch | `merge-touch.md` | PARTIAL layer; patches 0001/0002 in `analysis/agent-touch/patches/`; apply after HUD game.rs rewrite |

## 10. Conflict register (central)

| # | conflict | owner | resolution |
|---|---|---|---|
| C1 | `wc3-assetdb/src/lib.rs:259` passes `&MpqIndex` to `MpqSource::open_with_index(MpqIndex)` (E0308) | assets | assets layer fixes in its turn; if not fixed, dispatcher patch `lib.rs:259` drop the `&` |
| C2 | `wc3-game/src/assets.rs` still uses `wc3_formats::map::W3Map`; `MapBundle` patch ready | dispatcher/tech-lead | apply `analysis/tech-lead/patches/0001-assets-mapbundle-wts.patch` at freeze; do not edit assets.rs before |
| C3 | anim wants `MdxModel::chunks()` + `node_object_count()` in `wc3-formats::mdx` | anim | anim delivers patch; canonical move is chunk scanner from `wc3-re::mdx` into formats + re-export (no second scanner) |
| C4 | anim: two `MeshData` types (`wc3-gpu` vs `wc3-game::assets`) | GPU/anim | GPU manifest decides; one type, other side delegates |
| C5 | anim: alpha rule (any in-range key vs >=2 differing keys) | anim | pick runtime rule as canonical, delete other or keep as named diagnostic |
| C6 | HUD: local `SkinTable`, icon path list, generated chrome; `HudLayout` old geometry | HUD | HUD manifest already lists deletion; consumes `wc3_re::skin/icons` + real BLPs |
| C7 | render/world: `MapBundle` did not expose `war3map.doo` | world/render | **resolved**: `wc3-formats::map::parse_doodads` + `MapBundle.doodads`; world deletes `parse_doo` and delegates |
| C8 | `doodads.rs` opens `AssetDb` for `war3map.doo` (duplicate reader) | world | consume `MapBundle.doodads` / `ReLayer`; world applies its §3.3 call-site change |
| C9 | `wc3-re` natives.rs has 26 unreachable-pattern warnings (duplicate match arms) | RE/dispatcher | cleanup after freeze; behavior-neutral, verified by scan |
| C10 | anim patch 0003 removes `Track` from the assets import but `MeshLayer.alpha_track: Option<Track<f32>>` still needs it | dispatcher | consolidated assets patch keeps `use wc3_formats::mdx::Track;`; patch must compile |
| C11 | HUD manifest claims `assets.rs` `BlpImage` lacks `mipmaps`; current `wc3-formats::blp::BlpImage` has `mipmaps` | dispatcher | re-check at freeze against the real compile error; do not act on the stale line |
| C12 | touch patch 0001 rewrites `game.rs` pointer paths; HUD owns the file | HUD/touch | HUD applies `analysis/agent-touch/patches/0001-game-input-delegates.patch` after its rewrite; dispatcher applies 0002 camera facade |
| C13 | manifest references `WC3RE_MERGE_MATRIX.md` as absent | dispatcher | published 2026-11-12 (`analysis/tech-lead/WC3RE_MERGE_MATRIX.md`, 718 lines); no action |
| C14 | `wc3-game/src/render.rs:777` passes `&Vec3` where `Vec3` is required | GPU/render | render owner fixes; dispatcher verifies `cargo check -p wc3-game --lib` at freeze |
| C18 | world §3.1 height must match rendered triangle split; §3.2 water clock in render.rs | world/render | assets height patch folded into consolidated assets patch? no — separate one-hunk patch; render clock owned by GPU/render |
| C15 | three assets.rs patches overlap at imports/`Assets::load`: MapBundle (0001), anim (0003), assets P2 | dispatcher | consolidated patch ready: `analysis/tech-lead/patches/0006-assets-consolidated.patch` (504 lines, syntax-validated with rustfmt; supersedes 0001+0003+P2). Tech-lead applies at freeze; do not stack the three originals. |
| C16 | `wc3-data` was a nested workspace root (`[workspace]`), excluded from the root graph | dispatcher | removed nested marker + added to root members; `cargo metadata` now 28 packages, no duplicates |
| C17 | assets P1 applied had a duplicated `} else {` syntax error | dispatcher | fixed at `wc3-formats/src/blp.rs`; `cargo check -p wc3-formats` rc=0; `blp_info` decodes BTNFootman |

## 11. v1.1 additions (2026-11-12 17:2x)

- Patch `0002-mdx-chunks-node-objects.patch` applied by the dispatcher: the MDX
  chunk walker and `node_object_count` now live in `wc3-formats::mdx`
  (`MdxModel::chunks`, `MdxModel::node_object_count`, `Chunk`). `wc3-re::mdx::chunks`
  projects that list (payload offset − 8, String tag); `MdxSummary` gains
  `node_objects`. One byte walker.
- `wc3-formats::objects` and `wc3-re::mapobjects` are both projections of
  `gamedata::ObjectDataFile`; the kind table fix (`w3d/w3b/w3h`) is verified by
  `gamedata` test.

## 12. EXECUTION OWNERSHIP (v0, 2026-11-12 — supersedes manifest-only mode)

The base is published; no layer waits for a perfect document. Each layer has an
exclusive execution scope and merges its own code against this file and
`WC3RE_MERGE_MATRIX.md`. Manifests remain the report, not the gate. One writer
per path; conflicts go to the dispatcher with exact file:line.

| layer | session | EXCLUSIVE execution paths (write) | must not touch | layer compile gate |
|---|---|---|---|---|
| anim | `ses_e30595420ffdGjgXSvyWSGFoT0` | `crates/wc3-formats/src/mdx.rs`, `crates/wc3-anim/**` | `wc3-game/assets.rs`, `model_render.rs`, `wc3-gpu/**` | `cargo check -p wc3-formats -p wc3-anim` |
| world | `ses_e30595423ffdGkcohq9P8MIxiL` | `crates/wc3-game/src/{terrain_render,render,model_render,doodads}.rs`, terrain sections of `assets.rs` (`height_at`, terrain atlas/tiles/detail) | `wc3-re/**`, `wc3-formats/**`, HUD files | `cargo check -p wc3-game --lib` (shared; report errors outside scope) |
| HUD | `ses_e30595424ffe9AqABZXayrv0Rb` | `crates/wc3-game/src/{mobile_hud,game,hud,ui}.rs` | `assets.rs`, `terrain_render.rs`, `render.rs`, `model_render.rs`, `wc3-re/**` | `cargo check -p wc3-game --lib` (shared) |
| QA | `ses_e30595409ffe2wSkLaUsMd22EV` | `scripts/qa/**`, `analysis/agent-qa/**` | `crates/**` | scripts parse; no device |
| RE | `ses_e30424702ffe4bTUAPcwFK1lIp` | `crates/wc3-re/**` (incl. JASS/AI/engine/scan/mapfiles/objectdata), `crates/wc3-jass/**` | `wc3-mpq/**`, `wc3-formats/**` (except `wc3-re` re-exports), `wc3-game/**` | `cargo check -p wc3-re -p wc3-jass`; `wc3re scan` `unimplemented={}`; `wc3re parity` 44/44 |
| assets | `ses_e3059540affeOQRBuuqdX6w6Vh` | `crates/wc3-mpq/**`, `crates/wc3-assetdb/**`, `crates/wc3-audio/**` | `wc3-formats/src/blp.rs` (dispatcher applied P1), `wc3-game/assets.rs` | `cargo check -p wc3-mpq -p wc3-assetdb -p wc3-audio` |
| GPU | `ses_e30595412ffdOJEvwiBqSyRYDi` | `crates/wc3-gpu/**`, `crates/wc3-render/**`; `wc3-game/render.rs` integration only AFTER world releases it | `terrain_render/model_render/doodads` until world handoff | `cargo check -p wc3-gpu -p wc3-render` |
| FDF | `ses_e3059541bffeFozxokptFv8d5b` | `crates/wc3-formats/src/fdf.rs`, `crates/wc3-game/src/text.rs`, font functions in `hud.rs` (coordinate with HUD) | `ui.rs`, `mobile_hud.rs`, `game.rs` | `cargo check -p wc3-formats` |
| data | `ses_e3059541fffejS1YGmXDWBw12Q` | `crates/wc3-data/**`, `crates/wc3-formats/src/gamedata.rs` | `wc3-re/**`, `wc3-game/**` | `cargo check -p wc3-data -p wc3-formats` |
| sim | `ses_e3059541fffdUO0AGBqY6t1juG` | `crates/wc3-sim/**`, new `crates/wc3-game/src/sim_bridge.rs` | `game.rs` (HUD applies the shim patch), `hud.rs` | `cargo check -p wc3-sim` |
| touch | `ses_e30595423ffe7cc9wy7YBbXx9q` | `crates/wc3-input/**`, `crates/wc3-game/src/camera.rs`, input functions in `game.rs` (coordinate with HUD) | `mobile_hud.rs`, `hud.rs`, `ui.rs`, `terrain_render/**` | `cargo check -p wc3-input` |

Shared-file protocol:
1. `assets.rs`: world owns terrain sections; dispatcher owns imports +
   `Assets::load` (map/storm/db) + MeshData/anim declarations. World sends its
   section diff; dispatcher applies the consolidated assets patch (0006) first,
   then world rebases on it.
2. `game.rs`: HUD owns the file. Touch and sim deliver exact patches; HUD applies
   them in its scope and reports the combined rc.
3. `hud.rs`: HUD owns the file. FDF delivers the font-function patch; HUD
   applies it.
4. `render.rs`/`model_render.rs`: world owns first; GPU integrates after world
   reports the handoff. No parallel edits.
5. `wc3-re/**`: RE owns; anim/fdf/data touch only via dispatcher-approved patch.

Execution rules:
- Merge code now. Compile your scope when it is self-contained. Tests per layer
  only after its scope compiles; full suite only after the base is green.
- No device runs, no screenshots, no APK installs.
- Report rc + exact files and remaining conflicts to the dispatcher; do not
  wait for a perfect BASE.md.

## 13. Integration rc (first pass, 2026-11-13)

| gate | rc | evidence |
|---|---:|---|
| `cargo metadata --no-deps` | 0 | 28 packages, no duplicate names |
| `cargo check` 10 core crates (mpq/formats/re/jass/data/sim/input/anim/gpu/render) | 0 | `integration_v0.log` |
| `cargo check -p wc3-assetdb -p wc3-audio` | 0 | `chk_assets2.log` |
| `cargo check -p wc3-game --lib` | 0 | `game_v0.log` |
| `cargo test` 8 core crates | 0 | 316 tests / 35 suites (`full_base_test.log`) |
| `cargo test -p wc3-render --lib` | 0 | 25 passed |
| `cargo test -p wc3-game --lib` | 0 | **63 passed, 5 ignored**; world doodads 2,509/2,509, seam ratio 1.301; ui_fdf 10/10 |
| `wc3re scan` | 0 | overall true (111.59 s), 3,935 BLP / 2,048 MDX / 33 SLK / 67 TXT / 71 FDF / 44 maps / 91 AI / 1,182 icons / 0 errors; machine counts identical to baseline `analysis/agent-re/scan.json` |
| release APK | 0 | `cargo build --release -p herofolends-android` finished; libmain.so 7.3 MB; final rebuild after last fixes |

## 14. Final green (2026-11-13)

- `cargo metadata --no-deps`: 28 packages, 0 duplicates.
- Core 8-crate suite: **322 tests pass**, rc 0.
- `wc3-render --lib`: 25 pass, rc 0.
- `wc3-game`: **63 lib tests pass** (5 ignored), all integration suites pass, rc 0
  (`ui_fdf` 10/10; world doodads 2,509/2,509; seam 1.301).
- `wc3re scan`: overall **true** (111.59 s), counts identical to
  `analysis/agent-re/scan.json`, 0 map errors.
- Release APK: `cargo build --release -p herofolends-android` finished; final
  rebuild after the last ObjectDb fix (log `apk_build_final.log`).
- No device runs performed.
